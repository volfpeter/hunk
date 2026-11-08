//! What can be done to a repository.

use crate::git::{GitError, Repo, Result};
use crate::review::stash_commit;

/// A change to the repository. Stashes go by hash, since `stash@{n}` shifts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Stage(Vec<String>),
    Unstage(Vec<String>),
    /// Stages every change, new and deleted files too.
    StageAll,
    UnstageAll,
    /// Takes the files back to `HEAD`, deleting those it doesn't have.
    DiscardChanges(Vec<String>),
    /// Takes the files back to what is staged, deleting untracked ones.
    DiscardUnstaged(Vec<String>),
    Commit(String),
    /// Stages everything, then commits it.
    CommitAll(String),
    Checkout(String),
    CreateBranch(String),
    /// Puts changes aside, named `name` unless it is empty.
    Stash { name: String, takes: Stashed },
    ApplyStash(String),
    PopStash(String),
    DropStash(String),
    Pull,
    Push,
}

/// What a stash takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stashed {
    /// Changes to tracked files, staged or not.
    Tracked,
    /// Changes to tracked files and new files.
    WithNewFiles,
    /// Only what is staged.
    Staged,
}

impl Action {
    /// What it does, for people.
    pub fn label(&self) -> &'static str {
        match self {
            Action::Stage(_) => "Stage",
            Action::Unstage(_) => "Unstage",
            Action::StageAll => "Stage all",
            Action::UnstageAll => "Unstage all",
            Action::DiscardChanges(_) | Action::DiscardUnstaged(_) => "Discard",
            Action::Commit(_) | Action::CommitAll(_) => "Commit",
            Action::Checkout(_) => "Check out",
            Action::CreateBranch(_) => "New branch",
            Action::Stash { .. } => "Stash",
            Action::ApplyStash(_) => "Apply",
            Action::PopStash(_) => "Pop",
            Action::DropStash(_) => "Drop",
            Action::Pull => "Pull",
            Action::Push => "Push",
        }
    }

    /// What it did, for people, unless it only staged or unstaged files.
    pub fn outcome(&self) -> Option<String> {
        let said = match self {
            Action::Stage(_) | Action::Unstage(_) | Action::StageAll | Action::UnstageAll => return None,
            Action::DiscardChanges(_) | Action::DiscardUnstaged(_) => "Discarded changes",
            Action::Commit(_) | Action::CommitAll(_) => "Committed",
            Action::Checkout(branch) => return Some(format!("Switched to {branch}")),
            Action::CreateBranch(name) => return Some(format!("Created and switched to {name}")),
            Action::Stash { .. } => "Stashed",
            Action::ApplyStash(_) => "Applied the stash",
            Action::PopStash(_) => "Popped the stash",
            Action::DropStash(_) => "Dropped the stash",
            Action::Pull => "Pulled",
            Action::Push => "Pushed",
        };
        Some(said.to_owned())
    }

    pub fn run(&self, repo: &Repo) -> Result<()> {
        match self {
            Action::Stage(paths) => repo.run_on(&["add", "--all"], paths).map(drop),
            Action::Unstage(paths) => unstage(repo, paths),
            Action::StageAll => repo.run(&["add", "--all"]).map(drop),
            Action::UnstageAll => repo.run(&["reset", "-q"]).map(drop),
            Action::DiscardChanges(paths) => unstage(repo, paths).and_then(|()| discard_unstaged(repo, paths)),
            Action::DiscardUnstaged(paths) => discard_unstaged(repo, paths),
            Action::Commit(message) => repo.run(&["commit", "-m", message]).map(drop),
            Action::CommitAll(message) => Action::StageAll.run(repo).and_then(|()| Action::Commit(message.clone()).run(repo)),
            Action::Checkout(branch) => repo.run(&["switch", branch]).map(drop),
            Action::CreateBranch(name) => repo.run(&["switch", "-c", name]).map(drop),
            Action::Stash { name, takes } => stash(repo, name, *takes),
            Action::ApplyStash(stash) => repo.run(&["stash", "apply", &stash_commit(repo, stash)?]).map(drop),
            Action::PopStash(stash) => repo.run(&["stash", "pop", &stash_ref(repo, stash)?]).map(drop),
            Action::DropStash(stash) => repo.run(&["stash", "drop", &stash_ref(repo, stash)?]).map(drop),
            Action::Pull => repo.run(&["pull"]).map(drop),
            Action::Push => push(repo),
        }
    }
}

/// `reset` rather than `restore --staged`, which needs a commit to restore from.
fn unstage(repo: &Repo, paths: &[String]) -> Result<()> {
    repo.run_on(&["reset", "-q"], paths).map(drop)
}

fn discard_unstaged(repo: &Repo, paths: &[String]) -> Result<()> {
    let listed = repo.run_on(&["ls-files", "-z"], paths)?;
    let (tracked, untracked): (Vec<String>, Vec<String>) = paths.iter().cloned().partition(|path| listed.split_terminator('\0').any(|file| file == path));
    if !tracked.is_empty() {
        repo.run_on(&["restore", "--worktree"], &tracked)?;
    }
    if !untracked.is_empty() {
        repo.run_on(&["clean", "-f"], &untracked)?;
    }
    Ok(())
}

/// Where the stash with the hash `stash` sits right now: `pop` and `drop` take nothing else.
fn stash_ref(repo: &Repo, stash: &str) -> Result<String> {
    let commit = stash_commit(repo, stash)?;
    let list = repo.run(&["stash", "list", "-z", "--format=%H"])?;
    let index = list.split_terminator('\0').position(|hash| hash == commit);
    index.map(|index| format!("stash@{{{index}}}")).ok_or_else(|| GitError::new(format!("Stash {stash} is gone.")))
}

/// `--staged` needs git 2.35.
fn stash(repo: &Repo, name: &str, takes: Stashed) -> Result<()> {
    let mut args = vec!["stash", "push"];
    args.extend(match takes {
        Stashed::Tracked => None,
        Stashed::WithNewFiles => Some("--include-untracked"),
        Stashed::Staged => Some("--staged"),
    });
    if !name.is_empty() {
        args.extend(["-m", name]);
    }
    repo.run(&args).map(drop)
}

/// A branch that has never been pushed goes to `origin`, and tracks it from then on.
fn push(repo: &Repo) -> Result<()> {
    let args: &[&str] = if repo.commit("@{upstream}")?.is_some() { &["push"] } else { &["push", "--set-upstream", "origin", "HEAD"] };
    repo.run(args).map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::Fixture;

    fn paths(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_owned()).collect()
    }

    fn status(fixture: &Fixture) -> String {
        fixture.git(&["status", "--porcelain", "--untracked-files=all"])
    }

    /// A commit of `a.txt`, an edit to it, and an untracked `new.txt`.
    fn with_changes() -> Fixture {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.write("a.txt", "two\n");
        fixture.write("new.txt", "new\n");
        fixture
    }

    #[test]
    fn stages_and_unstages_edited_new_and_deleted_files() {
        let fixture = with_changes();
        fixture.commit("gone.txt", "gone\n", "second");
        fixture.git(&["rm", "-q", "--cached", "gone.txt"]);
        std::fs::remove_file(fixture.repo.root().join("gone.txt")).unwrap();
        fixture.git(&["reset", "-q"]);
        let all = paths(&["a.txt", "gone.txt", "new.txt"]);

        Action::Stage(all.clone()).run(&fixture.repo).unwrap();
        assert_eq!(status(&fixture), "M  a.txt\nD  gone.txt\nA  new.txt\n");

        Action::Unstage(all).run(&fixture.repo).unwrap();
        assert_eq!(status(&fixture), " M a.txt\n D gone.txt\n?? new.txt\n");
    }

    #[test]
    fn stages_and_unstages_everything_even_before_the_first_commit() {
        let fixture = Fixture::new();
        fixture.write("a.txt", "one\n");
        fixture.write("b.txt", "two\n");

        Action::StageAll.run(&fixture.repo).unwrap();
        assert_eq!(status(&fixture), "A  a.txt\nA  b.txt\n");

        Action::UnstageAll.run(&fixture.repo).unwrap();
        assert_eq!(status(&fixture), "?? a.txt\n?? b.txt\n");
    }

    #[test]
    fn no_paths_means_no_files_rather_than_all_of_them() {
        let fixture = with_changes();
        fixture.git(&["add", "a.txt"]);

        for action in [Action::Stage(Vec::new()), Action::Unstage(Vec::new()), Action::DiscardChanges(Vec::new()), Action::DiscardUnstaged(Vec::new())] {
            action.run(&fixture.repo).unwrap();
        }

        assert_eq!(status(&fixture), "M  a.txt\n?? new.txt\n");
    }

    #[test]
    fn takes_paths_literally() {
        let fixture = Fixture::new();
        fixture.commit("base.txt", "base\n", "first");
        fixture.write("a[1].txt", "new\n");
        fixture.write("a1.txt", "other\n");

        Action::Stage(paths(&["a[1].txt"])).run(&fixture.repo).unwrap();

        assert_eq!(status(&fixture), "A  a[1].txt\n?? a1.txt\n");
    }

    #[test]
    fn unstages_before_the_first_commit() {
        let fixture = Fixture::new();
        fixture.write("a.txt", "one\n");
        fixture.git(&["add", "a.txt"]);

        Action::Unstage(paths(&["a.txt"])).run(&fixture.repo).unwrap();

        assert_eq!(status(&fixture), "?? a.txt\n");
    }

    #[test]
    fn commits_what_is_staged_and_passes_on_git_saying_no() {
        let fixture = with_changes();
        assert!(Action::Commit("nothing staged".into()).run(&fixture.repo).unwrap_err().to_string().contains("no changes added"));

        fixture.git(&["add", "a.txt"]);
        Action::Commit("second\n\nWith a body.".into()).run(&fixture.repo).unwrap();

        assert_eq!(fixture.git(&["log", "-1", "--format=%B"]).trim(), "second\n\nWith a body.");
        assert_eq!(status(&fixture), "?? new.txt\n");
    }

    #[test]
    fn committing_all_takes_new_files_too() {
        let fixture = with_changes();
        Action::CommitAll("second".into()).run(&fixture.repo).unwrap();
        assert_eq!(status(&fixture), "");
        assert_eq!(fixture.git(&["show", "--format=", "--name-only", "HEAD"]), "a.txt\nnew.txt\n");
    }

    #[test]
    fn discarding_changes_goes_back_to_head() {
        let fixture = with_changes();
        fixture.git(&["add", "a.txt"]);
        fixture.write("a.txt", "three\n");
        fixture.write("staged.txt", "new\n");
        fixture.git(&["add", "staged.txt"]);

        Action::DiscardChanges(paths(&["a.txt", "new.txt", "staged.txt"])).run(&fixture.repo).unwrap();

        assert_eq!(fixture.read("a.txt").as_deref(), Some("one\n"));
        assert_eq!((fixture.read("new.txt"), fixture.read("staged.txt")), (None, None));
        assert_eq!(status(&fixture), "");
    }

    #[test]
    fn discarding_unstaged_changes_keeps_what_is_staged() {
        let fixture = with_changes();
        fixture.git(&["add", "a.txt"]);
        fixture.write("a.txt", "three\n");

        Action::DiscardUnstaged(paths(&["a.txt", "new.txt"])).run(&fixture.repo).unwrap();

        assert_eq!(fixture.read("a.txt").as_deref(), Some("two\n"));
        assert_eq!(fixture.read("new.txt"), None);
        assert_eq!(status(&fixture), "M  a.txt\n");
    }

    #[test]
    fn discarding_a_staged_rename_restores_the_old_file() {
        let fixture = Fixture::new();
        fixture.commit("old.txt", "one\n", "first");
        fixture.git(&["mv", "old.txt", "new.txt"]);

        Action::DiscardChanges(paths(&["old.txt", "new.txt"])).run(&fixture.repo).unwrap();

        assert_eq!((fixture.read("old.txt").as_deref(), fixture.read("new.txt")), (Some("one\n"), None));
    }

    #[test]
    fn discarding_before_the_first_commit_deletes_the_file() {
        let fixture = Fixture::new();
        fixture.write("a.txt", "one\n");
        fixture.git(&["add", "a.txt"]);

        Action::DiscardChanges(paths(&["a.txt"])).run(&fixture.repo).unwrap();

        assert_eq!(fixture.read("a.txt"), None);
    }

    #[test]
    fn checks_out_and_creates_branches() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        let head = || fixture.git(&["symbolic-ref", "--short", "HEAD"]).trim().to_owned();

        Action::CreateBranch("feature".into()).run(&fixture.repo).unwrap();
        assert_eq!(head(), "feature");

        Action::Checkout("main".into()).run(&fixture.repo).unwrap();
        assert_eq!(head(), "main");
    }

    fn stash(fixture: &Fixture, takes: Stashed) {
        Action::Stash { name: String::new(), takes }.run(&fixture.repo).unwrap();
    }

    #[test]
    fn stashes_tracked_changes_with_or_without_new_files() {
        let fixture = with_changes();
        stash(&fixture, Stashed::Tracked);
        assert_eq!(status(&fixture), "?? new.txt\n");

        fixture.write("a.txt", "two\n");
        stash(&fixture, Stashed::WithNewFiles);
        assert_eq!(status(&fixture), "");
    }

    #[test]
    fn stashes_only_what_is_staged() {
        let fixture = with_changes();
        fixture.commit("b.txt", "one\n", "second");
        fixture.write("b.txt", "two\n");
        fixture.git(&["add", "b.txt"]);

        stash(&fixture, Stashed::Staged);

        assert_eq!(status(&fixture), " M a.txt\n?? new.txt\n");
    }

    #[test]
    fn a_named_stash_keeps_its_name() {
        let fixture = with_changes();
        Action::Stash { name: "parked idea".into(), takes: Stashed::Tracked }.run(&fixture.repo).unwrap();
        assert!(fixture.git(&["stash", "list", "--format=%s"]).contains("parked idea"));
    }

    /// Two stashes and the hash of each, the newest last.
    fn with_two_stashes() -> (Fixture, String, String) {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        let stash = |text: &str| {
            fixture.write("a.txt", text);
            fixture.git(&["stash", "push", "-q"]);
            fixture.git(&["rev-parse", "--short", "stash@{0}"]).trim().to_owned()
        };
        let (older, newer) = (stash("older\n"), stash("newer\n"));
        (fixture, older, newer)
    }

    #[test]
    fn applying_a_stash_keeps_it_and_popping_removes_only_it() {
        let (fixture, older, newer) = with_two_stashes();

        Action::ApplyStash(older.clone()).run(&fixture.repo).unwrap();
        assert_eq!(fixture.read("a.txt").as_deref(), Some("older\n"));
        assert_eq!(fixture.git(&["stash", "list"]).lines().count(), 2);
        fixture.git(&["checkout", "--", "a.txt"]);

        Action::PopStash(older).run(&fixture.repo).unwrap();
        assert_eq!(fixture.read("a.txt").as_deref(), Some("older\n"));
        assert_eq!(fixture.git(&["rev-parse", "--short", "stash@{0}"]).trim(), newer);
        assert_eq!(fixture.git(&["stash", "list"]).lines().count(), 1);
    }

    #[test]
    fn applies_a_stash_whose_short_hash_is_only_digits() {
        let (fixture, _, _) = with_two_stashes();
        let stash = fixture.stash_with_digits_only_hash();

        Action::ApplyStash(stash).run(&fixture.repo).unwrap();

        assert_eq!(fixture.read("a.txt").as_deref(), Some("newer\n"));
    }

    #[test]
    fn dropping_goes_by_hash_however_the_stashes_shifted() {
        let (fixture, older, newer) = with_two_stashes();
        fixture.write("a.txt", "newest\n");
        fixture.git(&["stash", "push", "-q"]);

        Action::DropStash(older.clone()).run(&fixture.repo).unwrap();

        let left = fixture.git(&["stash", "list", "--format=%h"]);
        assert!(left.contains(&newer) && !left.contains(&older), "{left}");
        assert!(Action::DropStash(older).run(&fixture.repo).is_err());
    }

    #[test]
    fn a_conflict_reaches_the_user_though_git_says_it_on_stdout() {
        let (fixture, older, _) = with_two_stashes();
        fixture.commit("a.txt", "moved on\n", "second");

        let error = Action::ApplyStash(older).run(&fixture.repo).unwrap_err();

        assert!(error.to_string().contains("CONFLICT"), "{error}");
    }

    #[test]
    fn pushes_a_new_branch_to_origin_then_pulls_what_others_pushed() {
        let origin = Fixture::new();
        origin.commit("a.txt", "one\n", "first");
        origin.git(&["checkout", "-q", "--detach"]);
        let clone = Fixture::new();
        clone.git(&["remote", "add", "origin", origin.repo.root().to_str().unwrap()]);
        clone.git(&["fetch", "-q", "origin"]);
        clone.git(&["checkout", "-q", "-b", "main", "origin/main"]);
        clone.git(&["checkout", "-q", "-b", "feature"]);
        clone.commit("b.txt", "two\n", "second");

        Action::Push.run(&clone.repo).unwrap();

        assert_eq!(origin.git(&["log", "-1", "--format=%s", "feature"]).trim(), "second");
        assert_eq!(clone.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(), "origin/feature");

        clone.commit("c.txt", "three\n", "third");
        Action::Push.run(&clone.repo).unwrap();
        assert_eq!(origin.git(&["log", "-1", "--format=%s", "feature"]).trim(), "third");

        origin.git(&["checkout", "-q", "feature"]);
        origin.commit("d.txt", "four\n", "fourth");
        origin.git(&["checkout", "-q", "--detach"]);
        Action::Pull.run(&clone.repo).unwrap();
        assert_eq!(clone.read("d.txt").as_deref(), Some("four\n"));
    }
}
