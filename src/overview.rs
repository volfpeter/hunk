//! What a repository offers for review: its commits, branches and stashes, and where its files
//! and branch stand.

use std::collections::HashMap;

use crate::forge::Hosted;
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

/// A branch that is only on a remote: no local branch has its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteBranch {
    pub remote: String,
    pub name: String,
    /// When it last changed, for people.
    pub age: String,
}

impl RemoteBranch {
    /// What git calls it, like `origin/main`.
    pub fn id(&self) -> String {
        format!("{}/{}", self.remote, self.name)
    }
}

/// A remote, by the URL it fetches from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Remote {
    pub name: String,
    pub url: String,
}

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
    /// The local branches, newest first.
    pub branches: Vec<Entry>,
    /// Newest first.
    pub remote_branches: Vec<RemoteBranch>,
    pub remotes: Vec<Remote>,
    pub stashes: Vec<Entry>,
    /// Every file with something uncommitted about it, by path.
    pub files: HashMap<String, FileStatus>,
    /// The lines each staged file changes, by path.
    pub staged_lines: HashMap<String, LineCounts>,
    /// `None` when the current branch tracks nothing.
    pub upstream: Option<Upstream>,
    /// Where the repository is on the web: at the first remote URL with a host, `origin`'s first.
    pub hosted: Option<Hosted>,
}

impl Overview {
    pub fn load(repo: &Repo) -> Result<Overview> {
        let branches = branches(repo)?;
        let remotes = remotes(repo)?;
        Ok(Overview {
            head: head(repo)?,
            base: base_branch(repo, &branches)?,
            commits: commits(repo)?,
            remote_branches: remote_branches(repo, &remotes, &branches)?,
            hosted: hosted(&remotes),
            remotes,
            branches,
            stashes: stashes(repo)?,
            files: file_statuses(repo)?,
            staged_lines: staged_lines(repo)?,
            upstream: upstream(repo)?,
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

    /// What a forge calls the branch `name`: a remote's branch, like `origin/main`, goes by its
    /// name there. `HEAD` is no branch.
    pub fn forge_branch<'a>(&self, name: &'a str) -> Option<&'a str> {
        if name == "HEAD" {
            return None;
        }
        if self.branches.iter().any(|branch| branch.id == name) {
            return Some(name);
        }
        let on_remote = self.remotes.iter().find_map(|remote| name.strip_prefix(remote.name.as_str())?.strip_prefix('/'));
        Some(on_remote.unwrap_or(name))
    }
}

/// The remotes in the order they were added, each with its first URL.
fn remotes(repo: &Repo) -> Result<Vec<Remote>> {
    let urls = repo.run_accepting(&["config", "-z", "--get-regexp", r"^remote\..*\.url$"], &[0, 1])?;
    let mut remotes: Vec<Remote> = Vec::new();
    for (key, url) in urls.split_terminator('\0').filter_map(|entry| entry.split_once('\n')) {
        let Some(name) = key.strip_prefix("remote.").and_then(|key| key.strip_suffix(".url")) else { continue };
        if !remotes.iter().any(|remote| remote.name == name) {
            remotes.push(Remote { name: name.to_owned(), url: url.to_owned() });
        }
    }
    Ok(remotes)
}

/// Where the first remote URL with a host says the repository is, trying `origin`'s first.
fn hosted(remotes: &[Remote]) -> Option<Hosted> {
    let origin = remotes.iter().filter(|remote| remote.name == "origin");
    origin.chain(remotes).find_map(|remote| Hosted::parse(&remote.url))
}

fn head(repo: &Repo) -> Result<String> {
    let branch = repo.run_accepting(&["symbolic-ref", "--short", "-q", "HEAD"], &[0, 1])?;
    if branch.trim().is_empty() {
        return Ok(repo.run(&["rev-parse", "--short", "HEAD"])?.trim().to_owned());
    }
    Ok(branch.trim().to_owned())
}

/// The local branch named after `origin`'s default branch, or that remote branch itself when
/// there is no such local one, else `main` or `master`, else whatever is checked out.
fn base_branch(repo: &Repo, branches: &[Entry]) -> Result<String> {
    let remote = repo.run_accepting(&["symbolic-ref", "--short", "-q", "refs/remotes/origin/HEAD"], &[0, 1])?;
    let remote = remote.trim();
    if !remote.is_empty() {
        let local = remote.strip_prefix("origin/").filter(|name| branches.iter().any(|branch| branch.id == *name));
        return Ok(local.unwrap_or(remote).to_owned());
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

/// The branches of `remotes` that no local branch is named after. A remote's name may hold `/`,
/// so the longest that starts the ref wins.
fn remote_branches(repo: &Repo, remotes: &[Remote], local: &[Entry]) -> Result<Vec<RemoteBranch>> {
    let refs = repo.run(&["for-each-ref", "--sort=-committerdate", "--format=%(refname:lstrip=2)%1f%(symref)%1f%(committerdate:relative)", "refs/remotes"])?;
    Ok(refs
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(FIELD);
            let (id, symref, age) = (fields.next()?, fields.next()?, fields.next()?);
            // `<remote>/HEAD` points at another branch.
            if !symref.is_empty() {
                return None;
            }
            let remote = remotes.iter().filter(|remote| id.strip_prefix(&remote.name).is_some_and(|rest| rest.starts_with('/'))).max_by_key(|remote| remote.name.len())?;
            let name = &id[remote.name.len() + 1..];
            let only_remote = !local.iter().any(|branch| branch.id == name);
            only_remote.then(|| RemoteBranch { remote: remote.name.clone(), name: name.to_owned(), age: age.to_owned() })
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
        let project = || Overview::load(&fixture.repo).unwrap().hosted.map(|hosted| hosted.name());
        assert_eq!(project(), None);

        fixture.git(&["remote", "add", "local", "/srv/git/thing.git"]);
        assert_eq!(project(), None);
        fixture.git(&["remote", "add", "fork", "git@github.com:someone/thing.git"]);
        assert_eq!(project().as_deref(), Some("someone/thing"));
        fixture.git(&["remote", "add", "origin", "https://codeberg.org/org/thing/"]);
        assert_eq!(project().as_deref(), Some("org/thing"));
    }

    #[test]
    fn a_forge_knows_a_remote_branch_by_its_name_there_and_head_as_no_branch() {
        let overview = Overview {
            branches: vec![Entry { id: "origin/odd".into(), title: String::new(), detail: String::new() }],
            remotes: vec![Remote { name: "origin".into(), url: String::new() }],
            ..Overview::default()
        };
        assert_eq!(overview.forge_branch("origin/main"), Some("main"));
        assert_eq!(overview.forge_branch("origin/odd"), Some("origin/odd"), "a local branch keeps its name");
        assert_eq!(overview.forge_branch("feature"), Some("feature"));
        assert_eq!(overview.forge_branch("HEAD"), None);
    }

    #[test]
    fn offers_the_remote_branches_that_no_local_branch_is_named_after() {
        let origin = Fixture::new();
        origin.commit("a.txt", "one\n", "first");
        origin.git(&["branch", "user/feature"]);
        let clone = Fixture::new();
        clone.git(&["remote", "add", "origin", origin.repo.root().to_str().unwrap()]);
        clone.git(&["fetch", "-q", "origin"]);
        clone.git(&["remote", "set-head", "origin", "main"]);
        clone.git(&["checkout", "-q", "-b", "main", "origin/main"]);

        let overview = Overview::load(&clone.repo).unwrap();

        assert_eq!(overview.base, "main", "the local branch, rather than origin's");
        assert_eq!(overview.remotes, [Remote { name: "origin".into(), url: origin.repo.root().to_str().unwrap().into() }]);
        let ids: Vec<String> = overview.remote_branches.iter().map(RemoteBranch::id).collect();
        assert_eq!(ids, ["origin/user/feature"]);
        assert_eq!(overview.remote_branches[0].name, "user/feature");
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
