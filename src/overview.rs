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
    pub staged: Option<Change>,
    /// Since it was staged. A file not tracked yet is new.
    pub unstaged: Option<Change>,
}

/// What a change does to a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    New,
    Changed,
    Deleted,
}

impl Change {
    /// From a letter of `git status --porcelain`.
    fn from_status(letter: &str) -> Option<Change> {
        match letter {
            " " => None,
            "A" | "?" => Some(Change::New),
            "D" => Some(Change::Deleted),
            _ => Some(Change::Changed),
        }
    }
}

/// What a commit takes: what is staged, or everything when nothing is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommitPlan {
    /// Nothing is staged, so the commit stages everything first.
    pub takes_all: bool,
    pub new: usize,
    pub changed: usize,
    pub deleted: usize,
    /// Files the commit takes only part of.
    pub partly: usize,
    /// Files the commit leaves alone.
    pub left_out: usize,
}

impl CommitPlan {
    /// The files the commit takes, partly or wholly.
    pub fn files(&self) -> usize {
        self.new + self.changed + self.deleted
    }
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
    /// `<user-or-org>/<project>`, from the remotes' URLs: `origin`'s first.
    pub project: Option<String>,
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
            project: project(repo)?,
        })
    }

    pub fn staged_files(&self) -> usize {
        self.files.values().filter(|file| file.staged.is_some()).count()
    }

    pub fn commit_plan(&self) -> CommitPlan {
        let mut plan = CommitPlan { takes_all: self.staged_files() == 0, ..CommitPlan::default() };
        for file in self.files.values() {
            let taken = if plan.takes_all { file.unstaged } else { file.staged };
            match taken {
                Some(Change::New) => plan.new += 1,
                Some(Change::Changed) => plan.changed += 1,
                Some(Change::Deleted) => plan.deleted += 1,
                None => plan.left_out += 1,
            }
            if !plan.takes_all && file.staged.is_some() && file.unstaged.is_some() {
                plan.partly += 1;
            }
        }
        plan
    }
}

/// The name the first remote URL that has one gives the project, trying `origin`'s first.
fn project(repo: &Repo) -> Result<Option<String>> {
    let urls = repo.run_accepting(&["config", "-z", "--get-regexp", r"^remote\..*\.url$"], &[0, 1])?;
    let mut remotes: Vec<(&str, &str)> = urls.split_terminator('\0').filter_map(|entry| entry.split_once('\n')).collect();
    remotes.sort_by_key(|(key, _)| *key != "remote.origin.url");
    Ok(remotes.into_iter().find_map(|(_, url)| project_in(url)))
}

/// `<user-or-org>/<project>` from the last two parts of a remote's URL, unless it has no host,
/// as a path on this machine doesn't.
fn project_in(url: &str) -> Option<String> {
    let path = match url.split_once("://") {
        Some((_, rest)) => rest.split_once('/').filter(|(host, _)| !host.is_empty())?.1,
        // `[user@]host:path`, as ssh takes it.
        None => url.split_once(':').filter(|(host, _)| !host.is_empty() && !host.contains('/'))?.1,
    };
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut parts = path.rsplit('/').filter(|part| !part.is_empty());
    let (project, owner) = (parts.next()?, parts.next()?);
    Some(format!("{owner}/{project}"))
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
            let staged = if index == "?" { None } else { Change::from_status(index) };
            Some((path.to_owned(), FileStatus { staged, unstaged: Change::from_status(worktree) }))
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
        assert_eq!(status("both.txt"), Some((Some(Change::Changed), Some(Change::Changed))));
        assert_eq!(status("old.txt"), Some((Some(Change::Deleted), None)));
        assert_eq!(status("moved.txt"), Some((Some(Change::New), None)));
        assert_eq!(status("untracked.txt"), Some((None, Some(Change::New))));
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
    fn names_the_project_after_a_remote_on_a_host_origin_first() {
        let fixture = Fixture::new();
        let project = || Overview::load(&fixture.repo).unwrap().project;
        assert_eq!(project(), None);

        fixture.git(&["remote", "add", "local", "/srv/git/thing.git"]);
        assert_eq!(project(), None);
        fixture.git(&["remote", "add", "fork", "git@github.com:someone/thing.git"]);
        assert_eq!(project().as_deref(), Some("someone/thing"));
        fixture.git(&["remote", "add", "origin", "https://codeberg.org/org/thing/"]);
        assert_eq!(project().as_deref(), Some("org/thing"));
    }

    #[test]
    fn a_project_is_the_last_two_parts_of_a_url_with_a_host() {
        assert_eq!(project_in("ssh://git@host.example:2222/group/sub/repo.git").as_deref(), Some("sub/repo"));
        assert_eq!(project_in("github.com:owner/repo").as_deref(), Some("owner/repo"));
        assert_eq!(project_in("file:///srv/git/repo.git"), None);
        assert_eq!(project_in("../repo"), None);
        assert_eq!(project_in("https://host.example/repo"), None);
    }

    #[test]
    fn loads_a_repository_without_commits() {
        let overview = Overview::load(&Fixture::new().repo).unwrap();
        assert_eq!(overview.head, "main");
        assert!(overview.commits.is_empty());
    }

    #[test]
    fn a_commit_takes_what_is_staged_or_everything_when_nothing_is() {
        use Change::*;
        let overview = |files: &[(&str, Option<Change>, Option<Change>)]| Overview {
            files: files.iter().map(|&(path, staged, unstaged)| (path.to_owned(), FileStatus { staged, unstaged })).collect(),
            ..Overview::default()
        };

        let staged = overview(&[("new", Some(New), None), ("partly", Some(Changed), Some(Changed)), ("gone", None, Some(Deleted))]);
        let plan = CommitPlan { new: 1, changed: 1, partly: 1, left_out: 1, ..CommitPlan::default() };
        assert_eq!(staged.commit_plan(), plan);

        let nothing_staged = overview(&[("untracked", None, Some(New)), ("edited", None, Some(Changed)), ("gone", None, Some(Deleted))]);
        let plan = CommitPlan { takes_all: true, new: 1, changed: 1, deleted: 1, ..CommitPlan::default() };
        assert_eq!(nothing_staged.commit_plan(), plan);
    }
}
