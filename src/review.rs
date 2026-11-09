//! What can be reviewed: a comparison between two points in a repository.

use rune_kit::diff::{FileDiff, Side};
use serde::{Deserialize, Serialize};

use crate::git::{GitError, Repo, Result};

/// Context enough to hold any file whole, and small enough not to overflow git's sums on it.
const WHOLE_FILE: &str = "--unified=100000000";

/// Lines kept around each change in a folded diff, as `git diff` keeps by default.
const CONTEXT: u32 = 3;
/// Separates fields in git's formatted output.
const FIELD: char = '\x1f';

/// A comparison, as a patch of what changed between its two points.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Review {
    /// `HEAD` against the worktree: everything not committed.
    Changes,
    /// The index against the worktree: what is not staged.
    Unstaged,
    /// A commit against its first parent.
    Commit(String),
    /// What `branch` added since it left `base`.
    Branch { base: String, branch: String },
    /// A stash against the commit it was made on.
    Stash(String),
}

/// A review as read from the repository.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded {
    pub review: Review,
    pub patch: String,
    /// The commit under review, or `HEAD`'s for uncommitted work.
    pub commit: Option<String>,
}

impl Review {
    /// What the local branch `branch` added since it left the branch it most likely grew from:
    /// of the other local branches and `fallback`, the one it has the fewest commits beyond. Ties
    /// go to `fallback`, then to the branch fewest commits ahead. Branches that grew from
    /// `branch` don't count.
    pub fn of_branch(repo: &Repo, branch: &str, fallback: &str) -> Result<Review> {
        let tip = format!("refs/heads/{branch}");
        let counts = repo.run(&["rev-list", "--left-right", "--count", &format!("{fallback}...{tip}")])?;
        let fallback_counts = ahead_and_beyond(&counts).ok_or_else(|| GitError::new(format!("unexpected rev-list output: {counts}")))?;
        let format = format!("--format=%(refname:short)%1f%(ahead-behind:{tip})");
        let refs = repo.run(&["for-each-ref", &format, "refs/heads"])?;
        let others = refs.lines().filter_map(|line| {
            let (name, counts) = line.split_once(FIELD)?;
            let (ahead, beyond) = ahead_and_beyond(counts)?;
            let grew_from_branch = beyond == 0 && ahead > 0;
            (name != branch && !grew_from_branch).then_some((beyond, true, ahead, name))
        });
        let (fallback_ahead, fallback_beyond) = fallback_counts;
        let candidates = others.chain([(fallback_beyond, false, fallback_ahead, fallback)]);
        let (_, _, _, base) = candidates.min().expect("the fallback is always a candidate");
        Ok(Review::Branch { base: base.to_owned(), branch: branch.to_owned() })
    }

    /// The two points being compared, for people, the older first.
    pub fn sides(&self) -> (String, String) {
        match self {
            Review::Changes => ("HEAD".to_owned(), "working tree".to_owned()),
            Review::Unstaged => ("Staged".to_owned(), "working tree".to_owned()),
            Review::Commit(commit) | Review::Stash(commit) => (format!("{commit}^"), commit.clone()),
            Review::Branch { base, branch } => (base.clone(), branch.clone()),
        }
    }

    /// [`Review::sides`] as one line.
    pub fn title(&self) -> String {
        let (old, new) = self.sides();
        format!("{old} vs {new}")
    }

    pub fn load(self, repo: &Repo) -> Result<Loaded> {
        let commit = match &self {
            Review::Changes | Review::Unstaged => repo.head_commit()?,
            Review::Commit(commit) | Review::Stash(commit) => repo.commit(commit)?,
            Review::Branch { branch, .. } => repo.commit(branch)?,
        };
        Ok(Loaded { patch: self.patch(repo)?, commit, review: self })
    }

    /// The contents of `path` on one side of the comparison as it stands now, if it has that file.
    pub fn file(&self, repo: &Repo, side: Side, path: &str) -> Option<String> {
        match (self, side) {
            (Review::Changes | Review::Unstaged, Side::New) => repo.worktree_file(path),
            (Review::Changes, Side::Old) => blob(repo, "HEAD", path),
            (Review::Unstaged, Side::Old) => blob(repo, "", path),
            (Review::Commit(commit), Side::New) => blob(repo, commit, path),
            // Untracked files sit in the stash's third parent.
            (Review::Stash(stash), Side::New) => blob(repo, stash, path).or_else(|| blob(repo, &format!("{stash}^3"), path)),
            (Review::Commit(commit) | Review::Stash(commit), Side::Old) => blob(repo, &format!("{commit}^"), path),
            (Review::Branch { branch, .. }, Side::New) => blob(repo, branch, path),
            (Review::Branch { base, branch }, Side::Old) => {
                let fork = repo.run(&["merge-base", base, branch]).ok()?;
                blob(repo, fork.trim(), path)
            }
        }
    }

    /// The comparison as `git diff` output, each changed file whole.
    pub fn patch(&self, repo: &Repo) -> Result<String> {
        match self {
            Review::Changes => {
                let base = match repo.head_commit()? {
                    Some(commit) => commit,
                    None => empty_tree(repo)?,
                };
                Ok(diff(repo, &[&base])? + &untracked_patch(repo)?)
            }
            Review::Unstaged => Ok(diff(repo, &[])? + &untracked_patch(repo)?),
            Review::Commit(commit) => repo.run(&["show", "--no-ext-diff", WHOLE_FILE, "--format=", "--diff-merges=first-parent", commit]),
            Review::Branch { base, branch } => diff(repo, &[&format!("{base}...{branch}")]),
            Review::Stash(stash) => repo.run(&["stash", "show", "--no-ext-diff", WHOLE_FILE, "--patch", "--include-untracked", &stash_commit(repo, stash)?]),
        }
    }
}

/// Commits another point has that a branch lacks, and the other way round, from two counts.
fn ahead_and_beyond(counts: &str) -> Option<(u32, u32)> {
    let mut counts = counts.split_whitespace().map(str::parse);
    Some((counts.next()?.ok()?, counts.next()?.ok()?))
}

/// The files of a [`Review::patch`], folded to [`CONTEXT`] lines around each change, with every
/// fold able to open.
pub fn files(patch: &str) -> Vec<FileDiff> {
    rune_kit::diff::parse(patch).into_iter().map(|file| file.fold(CONTEXT)).collect()
}

/// The full hash of the stash `stash`, for git's stash commands: they take an argument of only
/// digits, as a short hash can be, for a place in the stash list.
pub fn stash_commit(repo: &Repo, stash: &str) -> Result<String> {
    repo.commit(stash)?.ok_or_else(|| GitError::new(format!("Stash {stash} is gone.")))
}

fn diff(repo: &Repo, revisions: &[&str]) -> Result<String> {
    let args: Vec<&str> = ["diff", "--no-ext-diff", WHOLE_FILE].iter().chain(revisions).copied().collect();
    repo.run(&args)
}

/// The file at `path` in `revision`, or in the index when `revision` is empty.
fn blob(repo: &Repo, revision: &str, path: &str) -> Option<String> {
    repo.run(&["cat-file", "blob", &format!("{revision}:{path}")]).ok()
}

/// The tree with nothing in it, what a repository without commits compares against.
fn empty_tree(repo: &Repo) -> Result<String> {
    Ok(repo.run(&["hash-object", "-t", "tree", "--stdin"])?.trim().to_owned())
}

/// Files git does not track yet, each as a patch adding it.
fn untracked_patch(repo: &Repo) -> Result<String> {
    let paths = repo.run(&["ls-files", "--others", "--exclude-standard", "-z"])?;
    let mut patch = String::new();
    for path in paths.split_terminator('\0') {
        let no_index = ["diff", "--no-ext-diff", "--no-index", "--", "/dev/null", path];
        patch.push_str(&repo.run_accepting(&no_index, &[1])?);
    }
    Ok(patch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::Fixture;

    /// The paths a patch touches, in order.
    fn paths(patch: &str) -> Vec<String> {
        rune_kit::diff::parse(patch).iter().map(|file| file.path().to_owned()).collect()
    }

    fn with_staged_unstaged_and_untracked() -> Fixture {
        let fixture = Fixture::new();
        fixture.commit("staged.txt", "one\n", "first");
        fixture.commit("unstaged.txt", "one\n", "second");
        fixture.write("staged.txt", "two\n");
        fixture.git(&["add", "staged.txt"]);
        fixture.write("unstaged.txt", "two\n");
        fixture.write("untracked.txt", "new\n");
        fixture
    }

    #[test]
    fn changes_cover_everything_uncommitted() {
        let fixture = with_staged_unstaged_and_untracked();
        let patch = Review::Changes.patch(&fixture.repo).unwrap();
        assert_eq!(paths(&patch), ["staged.txt", "unstaged.txt", "untracked.txt"]);
    }

    #[test]
    fn unstaged_leaves_out_what_is_staged() {
        let fixture = with_staged_unstaged_and_untracked();
        let patch = Review::Unstaged.patch(&fixture.repo).unwrap();
        assert_eq!(paths(&patch), ["unstaged.txt", "untracked.txt"]);
    }

    #[test]
    fn unstaged_compares_against_the_staged_version() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "committed\n", "first");
        fixture.write("a.txt", "staged\n");
        fixture.git(&["add", "a.txt"]);
        fixture.write("a.txt", "worktree\n");
        let patch = Review::Unstaged.patch(&fixture.repo).unwrap();
        assert!(patch.contains("-staged") && patch.contains("+worktree") && !patch.contains("committed"), "{patch}");
    }

    #[test]
    fn changes_work_before_the_first_commit() {
        let fixture = Fixture::new();
        fixture.write("staged.txt", "one\n");
        fixture.git(&["add", "staged.txt"]);
        fixture.write("untracked.txt", "two\n");
        let patch = Review::Changes.patch(&fixture.repo).unwrap();
        assert_eq!(paths(&patch), ["staged.txt", "untracked.txt"]);
    }

    #[test]
    fn a_commit_shows_only_its_own_changes() {
        let fixture = Fixture::new();
        fixture.commit("first.txt", "one\n", "first");
        fixture.commit("second.txt", "two\n", "second");
        assert_eq!(paths(&Review::Commit("HEAD".into()).patch(&fixture.repo).unwrap()), ["second.txt"]);
        assert_eq!(paths(&Review::Commit("HEAD~1".into()).patch(&fixture.repo).unwrap()), ["first.txt"]);
    }

    #[test]
    fn a_merge_commit_shows_what_it_brought_in() {
        let fixture = Fixture::new();
        fixture.commit("base.txt", "one\n", "base");
        fixture.git(&["checkout", "-q", "-b", "feature"]);
        fixture.commit("feature.txt", "two\n", "feature");
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit("main.txt", "three\n", "main");
        fixture.git(&["merge", "-q", "--no-edit", "feature"]);
        assert_eq!(paths(&Review::Commit("HEAD".into()).patch(&fixture.repo).unwrap()), ["feature.txt"]);
    }

    #[test]
    fn a_branch_shows_what_it_added_since_it_left_the_base() {
        let fixture = Fixture::new();
        fixture.commit("base.txt", "one\n", "base");
        fixture.git(&["checkout", "-q", "-b", "feature"]);
        fixture.commit("feature.txt", "two\n", "feature");
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit("main.txt", "three\n", "moved on");
        let review = Review::Branch { base: "main".into(), branch: "feature".into() };
        assert_eq!(paths(&review.patch(&fixture.repo).unwrap()), ["feature.txt"]);
    }

    #[test]
    fn a_branch_is_compared_with_the_branch_it_grew_from() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.git(&["branch", "old"]);
        fixture.git(&["checkout", "-q", "-b", "feature"]);
        fixture.commit("b.txt", "two\n", "feature");
        fixture.commit("c.txt", "three\n", "more feature");
        fixture.git(&["checkout", "-q", "-b", "agent"]);
        fixture.commit("d.txt", "four\n", "agent");
        fixture.git(&["checkout", "-q", "-b", "fresh", "main"]);
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit("m.txt", "moved\n", "main moved on");
        let base = |branch: &str| match Review::of_branch(&fixture.repo, branch, "main").unwrap() {
            Review::Branch { base, .. } => base,
            review => panic!("{review:?}"),
        };

        assert_eq!(base("agent"), "feature");
        assert_eq!(base("feature"), "main", "agent grew from feature, and old ties with main");
        assert_eq!(base("fresh"), "main", "fresh has nothing beyond old or main, and ties go to main");

        fixture.git(&["merge", "-q", "--no-ff", "-m", "merge", "feature"]);
        assert_eq!(base("feature"), "main", "main took feature in");
    }

    #[test]
    fn a_stash_shows_tracked_and_untracked_files() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.write("a.txt", "two\n");
        fixture.write("new.txt", "three\n");
        fixture.git(&["stash", "push", "-q", "--include-untracked"]);
        let stash = fixture.git(&["rev-parse", "--short", "stash@{0}"]);
        let patch = Review::Stash(stash.trim().to_owned()).patch(&fixture.repo).unwrap();
        assert_eq!(paths(&patch), ["a.txt", "new.txt"]);
    }

    #[test]
    fn a_stash_whose_short_hash_is_only_digits_shows_itself() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.write("a.txt", "two\n");
        fixture.git(&["stash", "push", "-q"]);
        let stash = fixture.stash_with_digits_only_hash();

        let patch = Review::Stash(stash).patch(&fixture.repo).unwrap();

        assert!(patch.contains("+two"), "{patch}");
    }

    #[test]
    fn files_fold_to_three_lines_around_a_change_and_can_open() {
        let fixture = Fixture::new();
        let lines = |changed: &str| (1..=20).map(|n| if n == 10 { format!("{changed}\n") } else { format!("{n}\n") }).collect::<String>();
        fixture.commit("a.txt", &lines("10"), "first");
        fixture.commit("a.txt", &lines("ten"), "second");
        fixture.write("a.txt", &lines("TEN"));
        let folded = |review: Review| {
            let files = files(&review.patch(&fixture.repo).unwrap());
            let file = &files[0];
            (file.expandable(), file.hunks.iter().map(|hunk| (hunk.new_start, hunk.lines.len())).collect::<Vec<_>>())
        };
        assert_eq!(folded(Review::Commit("HEAD".into())), (true, vec![(7, 8)]));
        assert_eq!(folded(Review::Changes), (true, vec![(7, 8)]));
        fixture.git(&["stash", "push", "-q"]);
        let stash = fixture.git(&["rev-parse", "stash@{0}"]);
        assert_eq!(folded(Review::Stash(stash.trim().to_owned())), (true, vec![(7, 8)]));
    }

    #[test]
    fn each_side_reads_the_point_it_compares() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "committed\n", "first");
        fixture.write("a.txt", "staged\n");
        fixture.git(&["add", "a.txt"]);
        fixture.write("a.txt", "worktree\n");
        let read = |review: &Review, side| review.file(&fixture.repo, side, "a.txt");
        assert_eq!(read(&Review::Changes, Side::Old).as_deref(), Some("committed\n"));
        assert_eq!(read(&Review::Changes, Side::New).as_deref(), Some("worktree\n"));
        assert_eq!(read(&Review::Unstaged, Side::Old).as_deref(), Some("staged\n"));
        assert_eq!(read(&Review::Commit("HEAD".into()), Side::New).as_deref(), Some("committed\n"));
        assert_eq!(read(&Review::Commit("HEAD".into()), Side::Old), None);
    }

    #[test]
    fn a_branch_reads_its_old_side_where_it_forked() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "fork\n", "base");
        fixture.git(&["checkout", "-q", "-b", "feature"]);
        fixture.commit("a.txt", "feature\n", "feature");
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit("a.txt", "moved on\n", "moved on");
        let review = Review::Branch { base: "main".into(), branch: "feature".into() };
        assert_eq!(review.file(&fixture.repo, Side::Old, "a.txt").as_deref(), Some("fork\n"));
        assert_eq!(review.file(&fixture.repo, Side::New, "a.txt").as_deref(), Some("feature\n"));
    }

    #[test]
    fn a_stash_reads_files_it_took_untracked() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.write("new.txt", "three\n");
        fixture.git(&["stash", "push", "-q", "--include-untracked"]);
        let review = Review::Stash("stash@{0}".into());
        assert_eq!(review.file(&fixture.repo, Side::New, "new.txt").as_deref(), Some("three\n"));
    }
}
