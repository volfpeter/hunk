//! The menus of list rows: what can be done to one row, without selecting it.

use rune::Tx;

use crate::action::Action;
use crate::app::{Kind, State};
use crate::overview::{Change, Tracking};

/// A row of one of the lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// A commit, branch or stash, in the list of that kind.
    Entry(Kind, String),
    /// A file or folder of the review, or the unstaged ones in Staging.
    File(String),
    /// A staged file or folder, in Staging.
    Staged(String),
}

/// The row whose menu is open, and the window point it opens at.
#[derive(Clone, Debug, PartialEq)]
pub struct RowMenu {
    pub row: Row,
    pub at: (f32, f32),
}

/// Something a row's menu offers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowAction {
    CheckOut,
    Update(Tracking),
    Pull,
    Push,
    /// Opens the merge dialog on the branch.
    Integrate,
    CopyName,
    DeleteBranch,
    Apply,
    Pop,
    Drop,
    CopyHash,
    UndoCommit,
    Stage,
    Unstage,
    Discard,
    Open,
    CopyPath,
}

impl RowAction {
    pub fn label(&self) -> String {
        match self {
            RowAction::CheckOut => "Check out".to_owned(),
            RowAction::Update(from) => format!("Update from {}", from.id()),
            RowAction::Pull => "Pull".to_owned(),
            RowAction::Push => "Push".to_owned(),
            RowAction::Integrate => "Merge or rebase…".to_owned(),
            RowAction::CopyName => "Copy name".to_owned(),
            RowAction::DeleteBranch => "Delete…".to_owned(),
            RowAction::Apply => "Apply".to_owned(),
            RowAction::Pop => "Pop".to_owned(),
            RowAction::Drop => "Drop…".to_owned(),
            RowAction::CopyHash => "Copy hash".to_owned(),
            RowAction::UndoCommit => "Undo commit".to_owned(),
            RowAction::Stage => "Stage".to_owned(),
            RowAction::Unstage => "Unstage".to_owned(),
            RowAction::Discard => "Discard…".to_owned(),
            RowAction::Open => "Open file".to_owned(),
            RowAction::CopyPath => "Copy path".to_owned(),
        }
    }

    /// Whether it throws something away, once confirmed.
    pub fn destroys(&self) -> bool {
        matches!(self, RowAction::DeleteBranch | RowAction::Drop | RowAction::Discard)
    }

    /// Whether it runs git, so it waits for the action running.
    pub fn runs_git(&self) -> bool {
        !matches!(self, RowAction::CopyName | RowAction::CopyHash | RowAction::Open | RowAction::CopyPath)
    }
}

impl State {
    /// Opens the menu of `row` at the window point `at`, unless it would be empty.
    pub fn open_row_menu(&mut self, row: Row, at: (f32, f32)) {
        if !self.row_actions(&row).is_empty() {
            self.row_menu = Some(RowMenu { row, at });
        }
    }

    pub fn close_row_menu(&mut self) {
        self.row_menu = None;
    }

    /// What the menu of `row` offers, the destructive ones last.
    pub fn row_actions(&self, row: &Row) -> Vec<RowAction> {
        let offered = match row {
            Row::Entry(Kind::Branches, branch) if *branch == self.overview.head => {
                let upstream = self.overview.upstream;
                vec![(upstream.is_some(), RowAction::Pull), (upstream.is_none_or(|upstream| upstream.ahead > 0), RowAction::Push), (true, RowAction::CopyName)]
            }
            Row::Entry(Kind::Branches, branch) => {
                let integrate = self.overview.last_commit.is_some() && self.overview.operation.is_none();
                let mut offered = vec![(true, RowAction::CheckOut)];
                offered.extend(self.overview.tracking.get(branch).map(|from| (true, RowAction::Update(from.clone()))));
                offered.extend([(integrate, RowAction::Integrate), (true, RowAction::CopyName), (true, RowAction::DeleteBranch)]);
                offered
            }
            Row::Entry(Kind::Stashes, _) => vec![(true, RowAction::Apply), (true, RowAction::Pop), (true, RowAction::Drop)],
            Row::Entry(Kind::Commits, commit) => {
                let last = self.overview.commits.first().is_some_and(|first| first.id == *commit) && self.undoable().is_some();
                vec![(true, RowAction::CopyHash), (last, RowAction::UndoCommit)]
            }
            Row::Entry(..) => Vec::new(),
            Row::File(path) => {
                let worktree = !self.kind.has_list();
                vec![
                    (worktree && !self.stageable_in(path).is_empty(), RowAction::Stage),
                    (self.openable(path), RowAction::Open),
                    (true, RowAction::CopyPath),
                    (worktree && !self.discardable_in(path).is_empty(), RowAction::Discard),
                ]
            }
            Row::Staged(path) => vec![(true, RowAction::Unstage), (self.openable(path), RowAction::Open), (true, RowAction::CopyPath)],
        };
        offered.into_iter().filter_map(|(offer, action)| offer.then_some(action)).collect()
    }

    /// Does `action` to `row`, closing its menu.
    pub fn act_on_row(&mut self, row: &Row, action: &RowAction, tx: &mut Tx) {
        self.close_row_menu();
        let (Row::Entry(_, id) | Row::File(id) | Row::Staged(id)) = row;
        match action {
            RowAction::CheckOut => self.run(Action::Checkout(id.clone())),
            RowAction::Update(from) => self.run(Action::Update { branch: id.clone(), from: from.clone() }),
            RowAction::Pull => self.run(Action::Pull),
            RowAction::Push => self.run(Action::Push),
            RowAction::Integrate => {
                self.open_merger();
                self.merge_text = rune::TextEdit::new(id.clone());
            }
            RowAction::CopyName | RowAction::CopyHash | RowAction::CopyPath => tx.copy(id.clone()),
            RowAction::DeleteBranch => self.confirming = Some(Action::DeleteBranch(id.clone())),
            RowAction::Apply => self.run(Action::ApplyStash(id.clone())),
            RowAction::Pop => self.run(Action::PopStash(id.clone())),
            RowAction::Drop => self.confirming = Some(Action::DropStash(id.clone())),
            RowAction::UndoCommit => self.undo_commit(),
            RowAction::Stage => self.run(Action::Stage(self.stageable_in(id))),
            RowAction::Unstage => self.run(Action::Unstage(self.unstageable_in(id))),
            RowAction::Discard => self.confirming = self.discard_in(id),
            RowAction::Open => self.browse(id.clone(), None, tx),
        }
    }

    /// Opens the entry `id` of the list on screen, as a double-click does: checks out a branch,
    /// applies a stash.
    pub fn open_entry(&mut self, id: &str) {
        match self.kind {
            Kind::Branches if id != self.overview.head => self.run(Action::Checkout(id.to_owned())),
            Kind::Stashes => self.run(Action::ApplyStash(id.to_owned())),
            _ => {}
        }
    }

    /// The paths at or under `path` in the file list that can be discarded.
    fn discardable_in(&self, path: &str) -> Vec<String> {
        let files: Vec<usize> = self.files.subtree(path).filter(|node| !node.dir).filter_map(|node| self.diff.file_index(&node.path)).collect();
        files.into_iter().flat_map(|file| self.discardable(file)).collect()
    }

    /// Discarding what the review on screen shows at or under `path`: back to `HEAD` in
    /// Changes, to what is staged in Staging.
    fn discard_in(&self, path: &str) -> Option<Action> {
        let paths = self.discardable_in(path);
        match self.kind {
            _ if paths.is_empty() => None,
            Kind::Changes => Some(Action::DiscardChanges(paths)),
            Kind::Staging => Some(Action::DiscardUnstaged(paths)),
            _ => None,
        }
    }

    /// Whether `path` is a file the file overlay can show: one the working tree has. A review
    /// of history goes by the diff, as its header's button does.
    fn openable(&self, path: &str) -> bool {
        let file = self.files.get(path).or_else(|| self.staged.get(path));
        if file.is_none_or(|node| node.dir) {
            return false;
        }
        if self.kind.has_list() {
            return self.diff.file_index(path).and_then(|file| self.worktree_path(file)).is_some();
        }
        self.overview.files.get(path).is_none_or(|status| status.unstaged != Some(Change::Deleted) && !(status.staged == Some(Change::Deleted) && status.unstaged.is_none()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::tests::{state, status};
    use crate::overview::{Entry, Upstream};

    fn entry(id: &str) -> Entry {
        Entry { id: id.to_owned(), title: id.to_owned(), detail: String::new() }
    }

    #[test]
    fn a_branch_offers_what_fits_it() {
        let (_fixture, mut state) = state();
        state.overview.head = "main".into();
        state.overview.upstream = Some(Upstream { ahead: 0, behind: 2 });
        let tracked = Tracking { remote: "origin".into(), branch: "feature".into() };
        state.overview.tracking.insert("feature".into(), tracked.clone());
        let actions = |state: &State, branch: &str| state.row_actions(&Row::Entry(Kind::Branches, branch.into()));

        assert_eq!(actions(&state, "main"), [RowAction::Pull, RowAction::CopyName], "nothing to push");
        assert_eq!(actions(&state, "feature"), [RowAction::CheckOut, RowAction::Update(tracked), RowAction::CopyName, RowAction::DeleteBranch], "no commit to merge into");
        assert_eq!(actions(&state, "local")[1], RowAction::CopyName);
    }

    #[test]
    fn only_the_last_commit_can_be_undone_from_its_row() {
        let (_fixture, mut state) = state();
        state.overview.commits = vec![entry("b"), entry("a")];
        state.overview.last_commit = Some(crate::overview::LastCommit { hash: "b".into(), parents: 1, message: String::new() });

        assert_eq!(state.row_actions(&Row::Entry(Kind::Commits, "b".into())), [RowAction::CopyHash, RowAction::UndoCommit]);
        assert_eq!(state.row_actions(&Row::Entry(Kind::Commits, "a".into())), [RowAction::CopyHash]);
    }

    #[test]
    fn destructive_actions_ask_first_and_merging_opens_its_dialog_on_the_branch() {
        let (_fixture, mut state) = state();
        let mut tx = Tx::default();
        state.open_row_menu(Row::Entry(Kind::Stashes, "abc".into()), (0.0, 0.0));

        state.act_on_row(&Row::Entry(Kind::Stashes, "abc".into()), &RowAction::Drop, &mut tx);
        assert_eq!((state.confirming.take(), &state.row_menu), (Some(Action::DropStash("abc".into())), &None));

        state.act_on_row(&Row::Entry(Kind::Branches, "feature".into()), &RowAction::DeleteBranch, &mut tx);
        assert_eq!(state.confirming.take(), Some(Action::DeleteBranch("feature".into())));

        state.act_on_row(&Row::Entry(Kind::Branches, "feature".into()), &RowAction::Integrate, &mut tx);
        assert_eq!((state.merger, state.merge_text.text.as_str()), (Some(0), "feature"));
    }

    #[test]
    fn a_deleted_file_has_nothing_to_open() {
        let (_fixture, mut state) = state();
        state.overview.files.insert("gone.txt".into(), status(false, true));
        state.overview.files.get_mut("gone.txt").unwrap().unstaged = Some(Change::Deleted);
        state.overview.files.insert("here.txt".into(), status(false, true));
        state.files = rune_kit::FileTree::new().key("files").build(["gone.txt", "here.txt"].map(|path| (path.to_owned(), Default::default())));

        assert!(!state.row_actions(&Row::File("gone.txt".into())).contains(&RowAction::Open));
        assert!(state.row_actions(&Row::File("here.txt".into())).contains(&RowAction::Open));
    }
}
