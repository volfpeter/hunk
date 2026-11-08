//! What a repository offers for review: its commits, branches and stashes, and where its files
//! and branch stand.

use std::collections::HashMap;

use crate::git::{Repo, Result};

const COMMIT_LIMIT: &str = "200";
/// Separates fields in git's formatted output, since subjects can hold anything printable.
const FIELD: char = '\x1f';

/// One row of a list: something to review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// What git calls it.
    pub id: String,
    pub title: String,
    pub detail: String,
}

/// What is uncommitted about a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStatus {
    pub staged: bool,
    /// Changed since it was staged, or not tracked yet.
    pub unstaged: bool,
}

/// Lines a change adds and removes.
pub type LineCounts = (u32, u32);

/// How far the current branch and the branch it tracks have moved apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Upstream {
    pub ahead: u32,
    pub behind: u32,
}

/// A repository's state, as of one load.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Overview {
    /// The current branch, or the short commit when detached.
    pub head: String,
    /// The branch others are compared against.
    pub base: String,
    pub commits: Vec<Entry>,
    pub branches: Vec<Entry>,
    pub stashes: Vec<Entry>,
    /// Every file with something uncommitted about it, by path.
    pub files: HashMap<String, FileStatus>,
    /// The lines each staged file changes, by path.
    pub staged_lines: HashMap<String, LineCounts>,
    /// `None` when the current branch tracks nothing.
    pub upstream: Option<Upstream>,
}

impl Overview {
    pub fn load(repo: &Repo) -> Result<Overview> {
        let branches = branches(repo)?;
        Ok(Overview {
            head: head(repo)?,
            base: base_branch(repo, &branches)?,
            commits: commits(repo)?,
            branches,
            stashes: stashes(repo)?,
            files: file_statuses(repo)?,
            staged_lines: staged_lines(repo)?,
            upstream: upstream(repo)?,
        })
    }

    pub fn staged_files(&self) -> usize {
        self.files.values().filter(|file| file.staged).count()
    }
}

fn head(repo: &Repo) -> Result<String> {
    let branch = repo.run_accepting(&["symbolic-ref", "--short", "-q", "HEAD"], &[0, 1])?;
    if branch.trim().is_empty() {
        return Ok(repo.run(&["rev-parse", "--short", "HEAD"])?.trim().to_owned());
    }
    Ok(branch.trim().to_owned())
}

/// The remote's default branch, else `main` or `master`, else whatever is checked out.
fn base_branch(repo: &Repo, branches: &[Entry]) -> Result<String> {
    let remote = repo.run_accepting(&["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"], &[0, 1])?;
    if !remote.trim().is_empty() {
        return Ok(remote.trim().to_owned());
    }
    let local = ["main", "master"].into_iter().find(|name| branches.iter().any(|branch| branch.id == *name));
    Ok(local.unwrap_or("HEAD").to_owned())
}

fn commits(repo: &Repo) -> Result<Vec<Entry>> {
    if repo.head_commit()?.is_none() {
        return Ok(Vec::new());
    }
    let log = repo.run(&["log", "-n", COMMIT_LIMIT, "-z", "--format=%h%x1f%s%x1f%ar"])?;
    Ok(log
        .split_terminator('\0')
        .filter_map(|record| {
            let (hash, rest) = record.split_once(FIELD)?;
            let (subject, age) = rest.split_once(FIELD)?;
            Some(Entry { id: hash.to_owned(), title: subject.to_owned(), detail: format!("{hash} · {age}") })
        })
        .collect())
}

fn branches(repo: &Repo) -> Result<Vec<Entry>> {
    let refs = repo.run(&["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)%1f%(committerdate:relative)", "refs/heads"])?;
    Ok(refs
        .lines()
        .filter_map(|line| {
            let (name, age) = line.split_once(FIELD)?;
            Some(Entry { id: name.to_owned(), title: name.to_owned(), detail: age.to_owned() })
        })
        .collect())
}

fn stashes(repo: &Repo) -> Result<Vec<Entry>> {
    let list = repo.run(&["stash", "list", "-z", "--format=%h%x1f%s%x1f%cr"])?;
    Ok(list
        .split_terminator('\0')
        .filter_map(|record| {
            let (hash, rest) = record.split_once(FIELD)?;
            let (subject, age) = rest.split_once(FIELD)?;
            Some(Entry { id: hash.to_owned(), title: subject.to_owned(), detail: age.to_owned() })
        })
        .collect())
}

fn file_statuses(repo: &Repo) -> Result<HashMap<String, FileStatus>> {
    let status = repo.run(&["status", "--porcelain", "-z", "--untracked-files=all", "--no-renames"])?;
    Ok(status
        .split_terminator('\0')
        .filter_map(|record| {
            let (index, worktree, path) = (record.get(0..1)?, record.get(1..2)?, record.get(3..)?);
            Some((path.to_owned(), FileStatus { staged: !matches!(index, " " | "?"), unstaged: worktree != " " }))
        })
        .collect())
}

/// Binary files count no lines.
fn staged_lines(repo: &Repo) -> Result<HashMap<String, LineCounts>> {
    let stat = repo.run(&["diff", "--cached", "--no-ext-diff", "--numstat", "-z", "--no-renames"])?;
    Ok(stat
        .split_terminator('\0')
        .filter_map(|record| {
            let mut fields = record.splitn(3, '\t');
            let (added, removed, path) = (fields.next()?, fields.next()?, fields.next()?);
            Some((path.to_owned(), (added.parse().unwrap_or(0), removed.parse().unwrap_or(0))))
        })
        .collect())
}

fn upstream(repo: &Repo) -> Result<Option<Upstream>> {
    if repo.head_commit()?.is_none() || repo.commit("@{upstream}")?.is_none() {
        return Ok(None);
    }
    let counts = repo.run(&["rev-list", "--left-right", "--count", "@{upstream}...HEAD"])?;
    let mut counts = counts.split_whitespace().filter_map(|count| count.parse().ok());
    Ok(counts.next().zip(counts.next()).map(|(behind, ahead)| Upstream { ahead, behind }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::Fixture;

    #[test]
    fn loads_commits_branches_and_stashes() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.commit("a.txt", "two\n", "second");
        fixture.git(&["branch", "feature"]);
        fixture.write("a.txt", "three\n");
        fixture.git(&["stash", "push", "-q", "-m", "parked"]);

        let overview = Overview::load(&fixture.repo).unwrap();

        assert_eq!(overview.head, "main");
        assert_eq!(overview.base, "main");
        assert_eq!(overview.commits.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["second", "first"]);
        let mut branches: Vec<&str> = overview.branches.iter().map(|b| b.id.as_str()).collect();
        branches.sort_unstable();
        assert_eq!(branches, ["feature", "main"]);
        assert_eq!(overview.stashes[0].id, fixture.git(&["rev-parse", "--short", "stash@{0}"]).trim());
        assert!(overview.stashes[0].title.contains("parked"));
    }

    #[test]
    fn says_what_is_staged_and_what_is_not() {
        let fixture = Fixture::new();
        fixture.commit("both.txt", "one\n", "first");
        fixture.commit("old.txt", "one\n", "second");
        fixture.write("both.txt", "two\n");
        fixture.git(&["add", "both.txt"]);
        fixture.write("both.txt", "three\n");
        fixture.git(&["mv", "old.txt", "moved.txt"]);
        fixture.write("untracked.txt", "new\n");

        let files = Overview::load(&fixture.repo).unwrap().files;

        let status = |path: &str| files.get(path).map(|file| (file.staged, file.unstaged));
        assert_eq!(status("both.txt"), Some((true, true)));
        assert_eq!(status("old.txt"), Some((true, false)));
        assert_eq!(status("moved.txt"), Some((true, false)));
        assert_eq!(status("untracked.txt"), Some((false, true)));
    }

    #[test]
    fn counts_the_lines_each_staged_file_changes_even_before_the_first_commit() {
        let fixture = Fixture::new();
        fixture.write("a.txt", "one\ntwo\n");
        fixture.git(&["add", "a.txt"]);
        assert_eq!(Overview::load(&fixture.repo).unwrap().staged_lines, HashMap::from([("a.txt".to_owned(), (2, 0))]));

        fixture.git(&["commit", "-q", "-m", "first"]);
        fixture.write("a.txt", "one\n");
        fixture.git(&["add", "a.txt"]);
        fixture.write("b.bin", "\0binary");
        fixture.git(&["add", "b.bin"]);
        fixture.write("a.txt", "unstaged\n");
        let lines = Overview::load(&fixture.repo).unwrap().staged_lines;
        assert_eq!(lines, HashMap::from([("a.txt".to_owned(), (0, 1)), ("b.bin".to_owned(), (0, 0))]));
    }

    #[test]
    fn a_detached_head_or_a_missing_upstream_tracks_nothing() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.git(&["config", "branch.main.remote", "origin"]);
        fixture.git(&["config", "branch.main.merge", "refs/heads/main"]);
        assert_eq!(Overview::load(&fixture.repo).unwrap().upstream, None);

        fixture.git(&["checkout", "-q", "--detach"]);
        assert_eq!(Overview::load(&fixture.repo).unwrap().upstream, None);
    }

    #[test]
    fn counts_commits_ahead_of_and_behind_the_upstream() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        assert_eq!(Overview::load(&fixture.repo).unwrap().upstream, None);

        fixture.git(&["branch", "base"]);
        fixture.git(&["branch", "--set-upstream-to=base"]);
        fixture.commit("a.txt", "two\n", "second");
        fixture.commit("a.txt", "three\n", "third");
        fixture.git(&["checkout", "-q", "base"]);
        fixture.commit("b.txt", "other\n", "elsewhere");
        fixture.git(&["checkout", "-q", "main"]);

        assert_eq!(Overview::load(&fixture.repo).unwrap().upstream, Some(Upstream { ahead: 2, behind: 1 }));
    }

    #[test]
    fn loads_a_repository_without_commits() {
        let overview = Overview::load(&Fixture::new().repo).unwrap();
        assert_eq!(overview.head, "main");
        assert!(overview.commits.is_empty());
    }
}
