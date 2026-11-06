//! The app's state and what changes it.

use rune::{KeyEvent, Latest, Tx, Waker};
use rune_kit::diff::{DiffMode, DiffState, FileDiff, parse};
use rune_kit::{FileNode, FileTree, TreeModel};

use crate::git::{Repo, Result};
use crate::overview::{Entry, Overview};
use crate::review::Review;

/// The kinds of thing that can be reviewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Changes,
    Unstaged,
    Commits,
    Branches,
    Stashes,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Changes, Kind::Unstaged, Kind::Commits, Kind::Branches, Kind::Stashes];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Changes => "Changes",
            Kind::Unstaged => "Staged vs unstaged",
            Kind::Commits => "Commits",
            Kind::Branches => "Branches",
            Kind::Stashes => "Stashes",
        }
    }

    /// Whether reviewing this kind means picking one from a list.
    pub fn has_list(self) -> bool {
        matches!(self, Kind::Commits | Kind::Branches | Kind::Stashes)
    }
}

/// Changed lines of a file: added, removed.
pub type LineCounts = (u32, u32);

pub struct State {
    repo: Repo,
    pub kind: Kind,
    pub overview: Overview,
    /// The commits, branches or stashes on offer.
    pub list: TreeModel<Entry>,
    /// The files the shown review changes.
    pub files: TreeModel<FileNode<LineCounts>>,
    pub diff: DiffState,
    pub error: Option<String>,
    overview_load: Latest<Result<Overview>>,
    patch_load: Latest<Result<String>>,
}

impl State {
    pub fn new(repo: Repo) -> State {
        let mut diff = DiffState::default();
        diff.set_mode(DiffMode::Split);
        State {
            repo,
            kind: Kind::Changes,
            overview: Overview::default(),
            list: TreeModel::new().key("list"),
            files: TreeModel::new().key("files"),
            diff,
            error: None,
            overview_load: Latest::default(),
            patch_load: Latest::default(),
        }
    }

    pub fn setup(&mut self, waker: Waker) {
        self.overview_load.set_waker(waker.clone());
        self.patch_load.set_waker(waker);
        self.refresh();
    }

    /// Lands finished loads; returns whether any did.
    pub fn poll(&mut self, _clock: f64, _tx: &mut Tx) -> bool {
        let overview = self.overview_load.take();
        let patch = self.patch_load.take();
        let landed = overview.is_some() || patch.is_some();
        if let Some(overview) = overview {
            self.show_overview(overview);
        }
        if let Some(patch) = patch {
            self.show_patch(patch);
        }
        landed
    }

    pub fn on_key(&mut self, event: &KeyEvent, tx: &mut Tx) -> bool {
        self.diff.key(event, tx)
    }

    pub fn repo_name(&self) -> String {
        self.repo.name()
    }

    /// Reads the repository again, then the review on screen.
    pub fn refresh(&mut self) {
        self.overview_load.request(self.repo.clone(), |repo| Overview::load(&repo));
    }

    pub fn select_kind(&mut self, kind: Kind) {
        self.kind = kind;
        self.list.clear_selection();
        self.rebuild_list();
        self.load_review();
    }

    /// Shows the entry selected in the list.
    pub fn load_review(&mut self) {
        match self.review() {
            Some(review) => self.patch_load.request((self.repo.clone(), review), |(repo, review)| review.patch(&repo)),
            None => {
                self.patch_load.cancel();
                self.show_files(Vec::new());
            }
        }
    }

    /// What is selected for review, if anything is.
    pub fn review(&self) -> Option<Review> {
        let selected = || self.list.selected().next().map(str::to_owned);
        match self.kind {
            Kind::Changes => Some(Review::Changes),
            Kind::Unstaged => Some(Review::Unstaged),
            Kind::Commits => selected().map(Review::Commit),
            Kind::Branches => selected().map(|branch| Review::Branch { base: self.overview.base.clone(), branch }),
            Kind::Stashes => selected().map(Review::Stash),
        }
    }

    /// Jumps the diff to the file `path`.
    pub fn scroll_to_path(&mut self, path: &str, tx: &mut Tx) {
        if let Some(file) = self.diff.file_index(path) {
            self.diff.scroll_to_file(file, tx);
        }
    }

    /// Marks the file at the top of the diff in the file list.
    pub fn follow_visible_file(&mut self, file: usize) {
        if let Some(diff) = self.diff.files().get(file) {
            self.files.select(diff.path());
        }
    }

    fn show_overview(&mut self, overview: Result<Overview>) {
        match overview {
            Ok(overview) => {
                self.overview = overview;
                self.error = None;
                self.rebuild_list();
                self.load_review();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn show_patch(&mut self, patch: Result<String>) {
        match patch {
            Ok(patch) => {
                self.error = None;
                self.show_files(parse(&patch));
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn show_files(&mut self, files: Vec<FileDiff>) {
        let counts = |file: &FileDiff| (file.additions() as u32, file.deletions() as u32);
        self.files = FileTree::new().key("files").compact_folders(true).build(files.iter().map(|file| (file.path().to_owned(), counts(file))));
        self.files.expand_all();
        self.diff.set_files(files);
    }

    /// Fills the list for the current kind, keeping the selection while it still exists and
    /// otherwise selecting the first entry.
    fn rebuild_list(&mut self) {
        let kept = self.list.selected().next().map(str::to_owned);
        let entries = match self.kind {
            Kind::Changes | Kind::Unstaged => &[][..],
            Kind::Commits => &self.overview.commits,
            Kind::Branches => &self.overview.branches,
            Kind::Stashes => &self.overview.stashes,
        };
        self.list = TreeModel::new().key("list");
        for entry in entries {
            self.list.insert(None, entry.id.clone(), entry.clone());
        }
        let selected = kept.filter(|id| self.list.contains(id)).or_else(|| entries.first().map(|entry| entry.id.clone()));
        if let Some(id) = selected {
            self.list.select(&id);
        }
    }
}
