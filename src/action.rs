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
    /// Steps back from the commit, keeping its changes staged. `message` is what it said.
    UndoCommit { commit: String, message: String },
    Checkout(String),
    /// Checks out `branch` of `remote` as a local branch tracking it.
    Track { remote: String, branch: String },
    CreateBranch(String),
    /// Deletes the local branch, whether another branch holds its commits or not.
    DeleteBranch(String),
    /// Brings in another branch, its name as git knows it.
    Integrate(Integration, String),
    /// Gives up the stopped merge or rebase, going back to where it started.
    Abort(Integration),
    /// Goes on with the stopped merge or rebase, taking what is staged.
    Continue(Integration),
    /// Puts changes aside, named `name` unless it is empty.
    Stash { name: String, takes: Stashed },
    ApplyStash(String),
    PopStash(String),
    DropStash(String),
    /// Fetches every remote, dropping what each no longer has.
    Fetch,
    Pull,
    Push,
    /// Replaces the upstream with the branch, unless it holds commits not fetched yet.
    ForcePush,
    AddRemote { name: String, url: String },
    RemoveRemote(String),
}

/// How another branch comes into the current one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Integration {
    /// Its commits join the current branch's in a merge commit, unless it can fast-forward.
    Merge,
    /// The current branch's own commits are made again on top of it.
    Rebase,
}

impl Integration {
    pub fn label(self) -> &'static str {
        match self {
            Integration::Merge => "Merge",
            Integration::Rebase => "Rebase",
        }
    }

    fn command(self) -> &'static str {
        match self {
            Integration::Merge => "merge",
            Integration::Rebase => "rebase",
        }
    }
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
            Action::UndoCommit { .. } => "Undo commit",
            Action::Checkout(_) | Action::Track { .. } => "Check out",
            Action::CreateBranch(_) => "New branch",
            Action::DeleteBranch(_) => "Delete branch",
            Action::Integrate(way, _) => way.label(),
            Action::Abort(Integration::Merge) => "Abort merge",
            Action::Abort(Integration::Rebase) => "Abort rebase",
            Action::Continue(Integration::Merge) => "Continue merge",
            Action::Continue(Integration::Rebase) => "Continue rebase",
            Action::Stash { .. } => "Stash",
            Action::ApplyStash(_) => "Apply",
            Action::PopStash(_) => "Pop",
            Action::DropStash(_) => "Drop",
            Action::Fetch => "Fetch",
            Action::Pull => "Pull",
            Action::Push => "Push",
            Action::ForcePush => "Force push",
            Action::AddRemote { .. } => "Add remote",
            Action::RemoveRemote(_) => "Remove remote",
        }
    }

    /// What it did, for people, unless it only staged or unstaged files.
    pub fn outcome(&self) -> Option<String> {
        let said = match self {
            Action::Stage(_) | Action::Unstage(_) | Action::StageAll | Action::UnstageAll => return None,
            Action::DiscardChanges(_) | Action::DiscardUnstaged(_) => "Discarded changes",
            Action::Commit(_) | Action::CommitAll(_) => "Committed",
            Action::UndoCommit { .. } => "Undid the commit, its changes are staged",
            Action::Checkout(branch) | Action::Track { branch, .. } => return Some(format!("Switched to {branch}")),
            Action::CreateBranch(name) => return Some(format!("Created and switched to {name}")),
            Action::DeleteBranch(name) => return Some(format!("Deleted {name}")),
            Action::Integrate(Integration::Merge, branch) => return Some(format!("Merged {branch}")),
            Action::Integrate(Integration::Rebase, branch) => return Some(format!("Rebased onto {branch}")),
            Action::Abort(Integration::Merge) => "Aborted the merge",
            Action::Abort(Integration::Rebase) => "Aborted the rebase",
            // Git stops again at the next conflict, which fails.
            Action::Continue(Integration::Merge) => "Merged",
            Action::Continue(Integration::Rebase) => "Rebased",
            Action::Stash { .. } => "Stashed",
            Action::ApplyStash(_) => "Applied the stash",
            Action::PopStash(_) => "Popped the stash",
            Action::DropStash(_) => "Dropped the stash",
            Action::Fetch => "Fetched",
            Action::Pull => "Pulled",
            Action::Push => "Pushed",
            Action::ForcePush => "Force pushed",
            Action::AddRemote { name, .. } => return Some(format!("Added remote {name}")),
            Action::RemoveRemote(name) => return Some(format!("Removed remote {name}")),
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
            Action::UndoCommit { commit, .. } => undo_commit(repo, commit),
            Action::Checkout(branch) => repo.run(&["switch", branch]).map(drop),
            // The full ref, as a local branch may be called `<remote>/<branch>` too.
            Action::Track { remote, branch } => repo.run(&["switch", "-c", branch, "--track", &format!("refs/remotes/{remote}/{branch}")]).map(drop),
            Action::CreateBranch(name) => repo.run(&["switch", "-c", name]).map(drop),
            Action::DeleteBranch(name) => repo.run(&["branch", "-D", "--", name]).map(drop),
            Action::Integrate(Integration::Merge, branch) => repo.run(&["merge", "--no-edit", branch]).map(drop),
            Action::Integrate(Integration::Rebase, branch) => repo.run(&["rebase", branch]).map(drop),
            Action::Abort(way) => repo.run(&[way.command(), "--abort"]).map(drop),
            Action::Continue(way) => repo.run(&[way.command(), "--continue"]).map(drop),
            Action::Stash { name, takes } => stash(repo, name, *takes),
            Action::ApplyStash(stash) => repo.run(&["stash", "apply", &stash_commit(repo, stash)?]).map(drop),
            Action::PopStash(stash) => repo.run(&["stash", "pop", &stash_ref(repo, stash)?]).map(drop),
            Action::DropStash(stash) => repo.run(&["stash", "drop", &stash_ref(repo, stash)?]).map(drop),
            Action::Fetch => repo.run(&["fetch", "--all", "--prune"]).map(drop),
            Action::Pull => repo.run(&["pull"]).map(drop),
            Action::Push => push(repo),
            Action::ForcePush => repo.run(&["push", "--force-with-lease"]).map(drop),
            Action::AddRemote { name, url } => repo.run(&["remote", "add", "--", name, url]).map(drop),
            Action::RemoveRemote(name) => repo.run(&["remote", "remove", name]).map(drop),
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

/// Undoes `commit` only while it is the last one, which a refresh may not have shown yet.
fn undo_commit(repo: &Repo, commit: &str) -> Result<()> {
    if repo.head_commit()?.as_deref() != Some(commit) {
        return Err(GitError::new("The last commit changed meanwhile, so nothing was undone."));
    }
    repo.run(&["reset", "-q", "--soft", &format!("{commit}~1")]).map(drop)
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

    #[test]
    fn fetching_drops_what_a_remote_no_longer_has_and_tracking_checks_it_out() {
        let origin = Fixture::new();
        origin.commit("a.txt", "one\n", "first");
        origin.git(&["branch", "user/feature"]);
        origin.git(&["branch", "gone"]);
        let clone = Fixture::new();
        let url = origin.repo.root().to_str().unwrap().to_owned();
        Action::AddRemote { name: "origin".into(), url }.run(&clone.repo).unwrap();
        Action::Fetch.run(&clone.repo).unwrap();
        origin.git(&["branch", "-q", "-D", "gone"]);

        Action::Fetch.run(&clone.repo).unwrap();

        assert_eq!(clone.git(&["for-each-ref", "--format=%(refname:lstrip=2)", "refs/remotes"]), "origin/HEAD\norigin/main\norigin/user/feature\n");
        Action::Track { remote: "origin".into(), branch: "user/feature".into() }.run(&clone.repo).unwrap();
        assert_eq!(clone.git(&["rev-parse", "--abbrev-ref", "HEAD", "@{upstream}"]), "user/feature\norigin/user/feature\n");

        Action::RemoveRemote("origin".into()).run(&clone.repo).unwrap();
        assert_eq!(clone.git(&["remote"]), "");
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

    /// `main` and `feature` changing the same line of `a.txt` apart, on `main`.
    fn with_clashing_branches() -> Fixture {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.git(&["checkout", "-q", "-b", "feature"]);
        fixture.commit("a.txt", "feature\n", "feature");
        fixture.git(&["checkout", "-q", "main"]);
        fixture.commit("a.txt", "main\n", "main");
        fixture
    }

    fn head(fixture: &Fixture) -> String {
        fixture.git(&["rev-parse", "HEAD"]).trim().to_owned()
    }

    #[test]
    fn a_conflicted_merge_stops_and_aborts_back_to_where_it_started() {
        let fixture = with_clashing_branches();
        let before = head(&fixture);

        let error = Action::Integrate(Integration::Merge, "feature".into()).run(&fixture.repo).unwrap_err();
        assert!(error.to_string().contains("CONFLICT"), "{error}");

        Action::Abort(Integration::Merge).run(&fixture.repo).unwrap();
        assert_eq!((head(&fixture), status(&fixture)), (before, String::new()));
    }

    #[test]
    fn a_settled_merge_continues_with_gits_message_and_no_editor() {
        let fixture = with_clashing_branches();
        assert!(Action::Integrate(Integration::Merge, "feature".into()).run(&fixture.repo).is_err());
        fixture.write("a.txt", "both\n");
        fixture.git(&["add", "a.txt"]);

        Action::Continue(Integration::Merge).run(&fixture.repo).unwrap();

        assert_eq!(fixture.git(&["log", "-1", "--format=%s"]).trim(), "Merge branch 'feature'");
        assert_eq!(fixture.git(&["rev-list", "--count", "--merges", "HEAD"]).trim(), "1");
    }

    #[test]
    fn a_rebase_stops_at_a_conflict_and_continues_or_aborts() {
        let fixture = with_clashing_branches();
        fixture.git(&["checkout", "-q", "feature"]);
        let before = head(&fixture);
        let rebase = Action::Integrate(Integration::Rebase, "main".into());

        assert!(rebase.run(&fixture.repo).is_err());
        Action::Abort(Integration::Rebase).run(&fixture.repo).unwrap();
        assert_eq!(head(&fixture), before);

        assert!(rebase.run(&fixture.repo).is_err());
        fixture.write("a.txt", "both\n");
        fixture.git(&["add", "a.txt"]);
        Action::Continue(Integration::Rebase).run(&fixture.repo).unwrap();
        assert_eq!(fixture.git(&["log", "--format=%s"]), "feature\nmain\nfirst\n");
        assert_eq!(fixture.git(&["symbolic-ref", "--short", "HEAD"]).trim(), "feature");
    }

    #[test]
    fn undoing_a_commit_keeps_its_changes_staged_unless_another_commit_came_meanwhile() {
        let fixture = Fixture::new();
        fixture.commit("a.txt", "one\n", "first");
        fixture.commit("a.txt", "two\n", "second");
        let second = head(&fixture);
        let undo = |commit: &str| Action::UndoCommit { commit: commit.to_owned(), message: String::new() }.run(&fixture.repo);

        undo(&second).unwrap();
        assert_eq!((fixture.git(&["log", "--format=%s"]), status(&fixture)), ("first\n".to_owned(), "M  a.txt\n".to_owned()));

        fixture.git(&["commit", "-q", "-m", "again"]);
        assert!(undo(&second).is_err());
        assert_eq!(fixture.git(&["log", "-1", "--format=%s"]).trim(), "again");
    }

    #[test]
    fn deletes_a_branch_even_with_commits_no_other_branch_has() {
        let fixture = with_clashing_branches();
        Action::DeleteBranch("feature".into()).run(&fixture.repo).unwrap();
        assert_eq!(fixture.git(&["branch", "--format=%(refname:short)"]), "main\n");
    }

    #[test]
    fn force_pushing_replaces_the_upstream_unless_it_moved_since_the_last_fetch() {
        let origin = Fixture::new();
        origin.commit("a.txt", "one\n", "first");
        origin.git(&["checkout", "-q", "--detach"]);
        let clone = Fixture::new();
        clone.git(&["remote", "add", "origin", origin.repo.root().to_str().unwrap()]);
        clone.git(&["fetch", "-q", "origin"]);
        clone.git(&["checkout", "-q", "-b", "main", "--track", "origin/main"]);
        clone.git(&["commit", "-q", "--amend", "-m", "rewritten"]);

        Action::ForcePush.run(&clone.repo).unwrap();
        assert_eq!(origin.git(&["log", "--format=%s", "main"]), "rewritten\n");

        origin.git(&["checkout", "-q", "main"]);
        origin.commit("b.txt", "two\n", "someone else's");
        origin.git(&["checkout", "-q", "--detach"]);
        clone.git(&["commit", "-q", "--amend", "-m", "rewritten again"]);
        assert!(Action::ForcePush.run(&clone.repo).is_err());
        assert_eq!(origin.git(&["log", "-1", "--format=%s", "main"]).trim(), "someone else's");
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
