//! The app's state and what changes it.

use std::time::Duration;

use rune::{KeyEvent, Latest, TextEdit, Tx, Waker};
use rune_kit::diff::{Annotation, DiffMode, DiffState, FileDiff, LineRange, parse};
use rune_kit::{FileNode, FileTree, TreeModel};

use crate::comments::{Anchor, Comment, Comments, prompt};
use crate::git::{Repo, Result};
use crate::overview::{Entry, Overview};
use crate::review::{Loaded, Review};

/// The id of the annotation holding the comment being written.
pub const DRAFT_NOTE: &str = "draft";
/// How long the copy button says it copied.
const COPIED_FOR: Duration = Duration::from_secs(2);

/// Whether the copy button says it copied.
enum CopiedNotice {
    Hidden,
    /// Copied just now; the next poll knows the time.
    Started,
    /// Showing until this clock time.
    Until(f64),
}

/// The kinds of thing that can be reviewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Changes,
    Unstaged,
    Commits,
    Branches,
    Stashes,
    Comments,
}

impl Kind {
    pub const ALL: [Kind; 6] = [Kind::Changes, Kind::Unstaged, Kind::Commits, Kind::Branches, Kind::Stashes, Kind::Comments];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Changes => "Changes",
            Kind::Unstaged => "Staged vs unstaged",
            Kind::Commits => "Commits",
            Kind::Branches => "Branches",
            Kind::Stashes => "Stashes",
            Kind::Comments => "Comments",
        }
    }

    /// Whether reviewing this kind means picking one from a list.
    pub fn has_list(self) -> bool {
        matches!(self, Kind::Commits | Kind::Branches | Kind::Stashes | Kind::Comments)
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
    pub comments: Comments,
    /// What the comment being written is about.
    pub draft: Option<Anchor>,
    pub draft_text: TextEdit,
    /// The comment being reworded.
    pub editing: Option<u64>,
    pub edit_text: TextEdit,
    /// The comment waiting for its deletion to be confirmed.
    pub deleting: Option<u64>,
    /// The review the diff shows and the commit under review.
    shown: Option<(Review, Option<String>)>,
    /// The comment to scroll to once its review is on screen.
    reveal: Option<u64>,
    copied: CopiedNotice,
    overview_load: Latest<Result<Overview>>,
    patch_load: Latest<Result<Loaded>>,
}

impl State {
    pub fn new(repo: Repo, comments: Comments) -> State {
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
            comments,
            draft: None,
            draft_text: TextEdit::default(),
            editing: None,
            edit_text: TextEdit::default(),
            deleting: None,
            shown: None,
            reveal: None,
            copied: CopiedNotice::Hidden,
            overview_load: Latest::default(),
            patch_load: Latest::default(),
        }
    }

    pub fn setup(&mut self, waker: Waker) {
        self.overview_load.set_waker(waker.clone());
        self.patch_load.set_waker(waker.clone());
        self.comments.set_waker(waker);
        self.refresh();
    }

    /// Lands finished loads; returns whether any did.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let overview = self.overview_load.take();
        let patch = self.patch_load.take();
        let comments = self.comments.poll();
        let copied_ended = self.step_copied_notice(clock, tx);
        let landed = overview.is_some() || patch.is_some() || comments || copied_ended;
        if let Some(overview) = overview {
            self.show_overview(overview);
        }
        if comments {
            self.comments_changed();
        }
        if let Some(patch) = patch {
            self.show_patch(patch, tx);
        }
        landed
    }

    pub fn on_key(&mut self, event: &KeyEvent, tx: &mut Tx) -> bool {
        self.diff.key(event, tx)
    }

    pub fn repo_name(&self) -> String {
        self.repo.name()
    }

    /// Reads the repository again, then the review on screen, and finds comments' code again.
    pub fn refresh(&mut self) {
        self.overview_load.request(self.repo.clone(), |repo| Overview::load(&repo));
        self.comments.follow_code();
    }

    pub fn select_kind(&mut self, kind: Kind) {
        self.kind = kind;
        self.list.clear_selection();
        self.rebuild_list();
        self.show_selected();
    }

    /// Shows the entry just selected in the list, and the comment if it is one.
    pub fn show_selected(&mut self) {
        self.reveal = self.selected_comment().map(|comment| comment.id);
        self.load_review();
    }

    /// Shows the entry selected in the list.
    pub fn load_review(&mut self) {
        match self.review() {
            Some(review) => self.patch_load.request((self.repo.clone(), review), |(repo, review)| review.load(&repo)),
            None => {
                self.patch_load.cancel();
                self.shown = None;
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
            Kind::Comments => self.selected_comment().map(|comment| comment.review.clone()),
        }
    }

    /// The comment selected in the list, when the list holds comments.
    pub fn selected_comment(&self) -> Option<&Comment> {
        let id = self.list.selected().next().filter(|_| self.kind == Kind::Comments)?;
        self.comments.get(id.parse().ok()?)
    }

    /// The selected comment, when the diff on screen has nowhere to show it.
    pub fn unplaced_comment(&self) -> Option<&Comment> {
        self.selected_comment().filter(|comment| comment.anchor.position(self.diff.files()).is_none())
    }

    /// Starts a comment on `lines`, keeping any text already written.
    pub fn compose_on_lines(&mut self, lines: LineRange) {
        if let Some(anchor) = self.diff.files().get(lines.file).and_then(|file| Anchor::on_lines(file, lines)) {
            self.open_draft(anchor);
        }
    }

    /// Starts a comment on all of the file at index `file`, keeping any text already written.
    pub fn compose_on_file(&mut self, file: usize, tx: &mut Tx) {
        let Some(anchor) = self.diff.files().get(file).map(Anchor::on_file) else { return };
        self.scroll_to_anchor(&anchor, tx);
        self.open_draft(anchor);
    }

    /// Keeps the comment being written.
    pub fn submit_draft(&mut self) {
        let body = self.draft_text.text.trim().to_owned();
        if body.is_empty() {
            return;
        }
        if let (Some(anchor), Some((review, commit))) = (self.draft.clone(), self.shown.clone()) {
            self.comments.add(anchor, review, commit, body);
            self.diff.set_selection(None);
            self.cancel_draft();
        }
    }

    pub fn cancel_draft(&mut self) {
        self.draft = None;
        self.draft_text = TextEdit::default();
        self.comments_changed();
    }

    pub fn resolve_comment(&mut self, id: u64) {
        self.comments.set_resolved(id, true);
        self.comments_changed();
    }

    /// Makes the comment open again, and looks for its code, which nothing followed meanwhile.
    pub fn reopen_comment(&mut self, id: u64) {
        self.comments.set_resolved(id, false);
        self.comments.follow_code();
        self.comments_changed();
    }

    /// Opens the comment's wording for editing, leaving any comment being written as it is.
    pub fn start_edit(&mut self, id: u64) {
        let Some(comment) = self.comments.get(id) else { return };
        self.edit_text = TextEdit::new(comment.body.clone());
        self.edit_text.focused = true;
        self.draft_text.focused = false;
        self.editing = Some(id);
    }

    /// Opens the selected comment's wording for editing and brings it into view.
    pub fn edit_selected(&mut self, tx: &mut Tx) {
        let Some((id, anchor)) = self.selected_comment().map(|comment| (comment.id, comment.anchor.clone())) else { return };
        self.start_edit(id);
        self.scroll_to_anchor(&anchor, tx);
    }

    /// Keeps the new wording.
    pub fn submit_edit(&mut self) {
        let body = self.edit_text.text.trim().to_owned();
        if body.is_empty() {
            return;
        }
        if let Some(id) = self.editing {
            self.comments.edit(id, body);
            self.cancel_edit();
        }
    }

    pub fn cancel_edit(&mut self) {
        self.editing = None;
        self.edit_text = TextEdit::default();
        self.comments_changed();
    }

    pub fn ask_to_delete(&mut self, id: u64) {
        self.deleting = Some(id);
    }

    pub fn confirm_delete(&mut self) {
        if let Some(id) = self.deleting.take() {
            self.comments.delete(id);
            self.comments_changed();
        }
    }

    pub fn cancel_delete(&mut self) {
        self.deleting = None;
    }

    /// Copies every open comment as a prompt for an agent.
    pub fn copy_prompt(&mut self, tx: &mut Tx) {
        tx.copy(prompt(&self.comments.open()));
        self.copied = CopiedNotice::Started;
    }

    pub fn just_copied(&self) -> bool {
        !matches!(self.copied, CopiedNotice::Hidden)
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

    /// Times the copied notice; returns whether it just ended.
    fn step_copied_notice(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let until = match self.copied {
            CopiedNotice::Hidden => return false,
            CopiedNotice::Started => clock + COPIED_FOR.as_secs_f64(),
            CopiedNotice::Until(until) => until,
        };
        if clock >= until {
            self.copied = CopiedNotice::Hidden;
            return true;
        }
        self.copied = CopiedNotice::Until(until);
        tx.wake_after(Duration::from_secs_f64(until - clock));
        false
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

    fn show_patch(&mut self, patch: Result<Loaded>, tx: &mut Tx) {
        match patch {
            Ok(loaded) => {
                self.error = None;
                self.shown = Some((loaded.review, loaded.commit));
                self.show_files(parse(&loaded.patch));
                if let Some(anchor) = self.reveal.take().and_then(|id| self.comments.get(id)).map(|comment| comment.anchor.clone()) {
                    self.scroll_to_anchor(&anchor, tx);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn open_draft(&mut self, anchor: Anchor) {
        self.draft = Some(anchor);
        self.draft_text.focused = true;
        self.edit_text.focused = false;
        self.place_annotations();
    }

    fn scroll_to_anchor(&mut self, anchor: &Anchor, tx: &mut Tx) {
        if let Some((file, at)) = anchor.position(self.diff.files()) {
            self.diff.scroll_to_line(file, at, tx);
        }
    }

    /// Brings what shows comments up to date with them.
    fn comments_changed(&mut self) {
        self.place_annotations();
        if self.kind == Kind::Comments {
            let selected = self.list.selected().next().map(str::to_owned);
            self.rebuild_list();
            if self.list.selected().next().map(str::to_owned) != selected {
                self.show_selected();
            }
        }
    }

    /// Puts each open comment, the selected comment and the draft under their lines in the diff
    /// on screen.
    fn place_annotations(&mut self) {
        let selected_resolved = self.selected_comment().filter(|comment| comment.resolved);
        let files = self.diff.files();
        let note = |anchor: &Anchor, id: String| anchor.position(files).map(|(file, at)| Annotation::new(file, at, id));
        let comments = self.comments.open().into_iter().chain(selected_resolved).filter_map(|comment| note(&comment.anchor, comment.id.to_string()));
        let draft = self.draft.as_ref().and_then(|anchor| note(anchor, DRAFT_NOTE.to_owned()));
        let notes = comments.chain(draft).collect();
        self.diff.set_annotations(notes);
    }

    fn show_files(&mut self, files: Vec<FileDiff>) {
        let counts = |file: &FileDiff| (file.additions() as u32, file.deletions() as u32);
        self.files = FileTree::new().key("files").compact_folders(true).build(files.iter().map(|file| (file.path().to_owned(), counts(file))));
        self.files.expand_all();
        self.diff.set_files(files);
        self.place_annotations();
    }

    /// Fills the list for the current kind, keeping the selection while it still exists and
    /// otherwise selecting the first entry.
    fn rebuild_list(&mut self) {
        let kept = self.list.selected().next().map(str::to_owned);
        let comments: Vec<Entry>;
        let entries = match self.kind {
            Kind::Changes | Kind::Unstaged => &[][..],
            Kind::Commits => &self.overview.commits,
            Kind::Branches => &self.overview.branches,
            Kind::Stashes => &self.overview.stashes,
            Kind::Comments => {
                comments = self.comments.listed().into_iter().map(comment_entry).collect();
                &comments
            }
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

fn comment_entry(comment: &Comment) -> Entry {
    let location = comment.anchor.location();
    Entry {
        id: comment.id.to_string(),
        title: comment.body.lines().next().unwrap_or_default().to_owned(),
        detail: comment.status().map_or_else(|| location.clone(), |status| format!("{location} · {}", status.to_lowercase())),
    }
}
