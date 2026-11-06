//! What can be reviewed: a comparison between two points in a repository.

use rune_kit::diff::Side;
use serde::{Deserialize, Serialize};

use crate::git::{Repo, Result};

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
    /// The two points being compared, for people.
    pub fn title(&self) -> String {
        match self {
            Review::Changes => "HEAD → working tree".to_owned(),
            Review::Unstaged => "Staged → working tree".to_owned(),
            Review::Commit(commit) => format!("{commit}^ → {commit}"),
            Review::Branch { base, branch } => format!("{base} → {branch}"),
            Review::Stash(stash) => stash.clone(),
        }
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

    /// The comparison as `git diff` output.
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
            Review::Commit(commit) => repo.run(&["show", "--no-ext-diff", "--format=", "--diff-merges=first-parent", commit]),
            Review::Branch { base, branch } => diff(repo, &[&format!("{base}...{branch}")]),
            Review::Stash(stash) => repo.run(&["stash", "show", "--no-ext-diff", "--patch", "--include-untracked", stash]),
        }
    }
}

fn diff(repo: &Repo, revisions: &[&str]) -> Result<String> {
    let args: Vec<&str> = ["diff", "--no-ext-diff"].iter().chain(revisions).copied().collect();
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
