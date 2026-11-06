//! What a repository offers for review: its commits, branches and stashes.

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
        })
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
    let list = repo.run(&["stash", "list", "-z", "--format=%s%x1f%cr"])?;
    Ok(list
        .split_terminator('\0')
        .enumerate()
        .filter_map(|(index, record)| {
            let (subject, age) = record.split_once(FIELD)?;
            Some(Entry { id: format!("stash@{{{index}}}"), title: subject.to_owned(), detail: age.to_owned() })
        })
        .collect())
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
        assert_eq!(overview.stashes[0].id, "stash@{0}");
        assert!(overview.stashes[0].title.contains("parked"));
    }

    #[test]
    fn loads_a_repository_without_commits() {
        let overview = Overview::load(&Fixture::new().repo).unwrap();
        assert_eq!(overview.head, "main");
        assert!(overview.commits.is_empty());
    }
}
