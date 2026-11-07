//! The app's state and what changes it.

use std::time::Duration;

use rune::{KeyEvent, Latest, TextEdit, Tx, Waker};
use rune_kit::diff::{Annotation, DiffMode, DiffState, FileDiff, LineRange, parse};
use rune_kit::{FileNode, FileTree, TreeModel};

use crate::action::Action;
use crate::comments::{Anchor, Comment, Comments, prompt};
use crate::git::{Repo, Result};
use crate::overview::{Entry, FileStatus, Overview};
use crate::review::{Loaded, Review};
use crate::watch::RepoWatch;

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

/// What the diff on screen shows.
struct Shown {
    review: Review,
    /// The commit under review.
    commit: Option<String>,
    patch: String,
}

/// An action failed, and how.
pub struct ActionFailure {
    pub action: &'static str,
    pub message: String,
}

/// What a palette command does.
type Run = Box<dyn Fn(&mut State, &mut Tx)>;

/// Something the command palette offers.
pub struct Command {
    /// What it concerns, such as "Git".
    topic: &'static str,
    pub label: String,
    /// The key that does the same outside the palette.
    pub shortcut: Option<&'static str>,
    run: Run,
}

impl Command {
    fn new(topic: &'static str, label: impl Into<String>, run: impl Fn(&mut State, &mut Tx) + 'static) -> Command {
        Command { topic, label: label.into(), shortcut: None, run: Box::new(run) }
    }

    fn shortcut(self, key: &'static str) -> Command {
        Command { shortcut: Some(key), ..self }
    }

    /// Its topic and label, as one line.
    pub fn title(&self) -> String {
        format!("{}: {}", self.topic, self.label)
    }
}

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
    pub commit_text: TextEdit,
    pub branch_text: TextEdit,
    /// Whether the new branch dialog is open.
    pub naming_branch: bool,
    /// The action waiting to be confirmed.
    pub confirming: Option<Action>,
    /// What the action in the background is called.
    pub running: Option<&'static str>,
    pub action_failure: Option<ActionFailure>,
    /// Why the repository can't be watched for changes.
    pub watch_error: Option<String>,
    pub palette_text: TextEdit,
    /// The highlighted row of the command palette, while it is open.
    pub palette: Option<usize>,
    shown: Option<Shown>,
    /// The comment to scroll to once its review is on screen.
    reveal: Option<u64>,
    copied: CopiedNotice,
    overview_load: Latest<Result<Overview>>,
    patch_load: Latest<Result<Loaded>>,
    action_load: Latest<(Action, Result<()>)>,
    watch: Option<RepoWatch>,
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
            commit_text: TextEdit::default(),
            branch_text: TextEdit::default(),
            naming_branch: false,
            confirming: None,
            running: None,
            action_failure: None,
            watch_error: None,
            palette_text: TextEdit::default(),
            palette: None,
            shown: None,
            reveal: None,
            copied: CopiedNotice::Hidden,
            overview_load: Latest::default(),
            patch_load: Latest::default(),
            action_load: Latest::default(),
            watch: None,
        }
    }

    pub fn setup(&mut self, waker: Waker) {
        self.overview_load.set_waker(waker.clone());
        self.patch_load.set_waker(waker.clone());
        self.action_load.set_waker(waker.clone());
        self.comments.set_waker(waker.clone());
        self.watch = Some(RepoWatch::start(self.repo.clone(), waker));
        self.refresh();
    }

    /// Lands finished loads; returns whether any did.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let overview = self.overview_load.take();
        let patch = self.patch_load.take();
        let comments = self.comments.poll();
        let action = self.action_load.take();
        let (repo_changed, watch_failure) = self.watch.as_ref().map_or((false, None), RepoWatch::take);
        let copied_ended = self.step_copied_notice(clock, tx);
        let landed = overview.is_some() || patch.is_some() || comments || action.is_some() || watch_failure.is_some() || copied_ended;
        if watch_failure.is_some() {
            self.watch_error = watch_failure;
        }
        if repo_changed {
            self.refresh();
        }
        if let Some((action, result)) = action {
            self.finish_action(&action, result);
        }
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
        if event.is_command('k') || (event.is_command('p') && event.mods.shift) {
            if !self.dialog_open() {
                self.open_palette();
            }
            return true;
        }
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

    /// The selected comment or the one being reworded, when the diff on screen has nowhere to
    /// show it.
    pub fn unplaced_comment(&self) -> Option<&Comment> {
        let editing = self.editing.and_then(|id| self.comments.get(id));
        self.selected_comment().or(editing).filter(|comment| comment.anchor.position(self.diff.files()).is_none())
    }

    /// Whether the comment being written has lost its place in the diff on screen.
    pub fn draft_unplaced(&self) -> bool {
        self.draft.as_ref().is_some_and(|anchor| anchor.position(self.diff.files()).is_none())
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
        if let (Some(anchor), Some(shown)) = (self.draft.clone(), &self.shown) {
            self.comments.add(anchor, shown.review.clone(), shown.commit.clone(), body);
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
        self.focus(|s| &mut s.edit_text);
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

    /// Runs `action` in the background, unless one is running already.
    pub fn run(&mut self, action: Action) {
        if self.running.is_none() {
            self.running = Some(action.label());
            self.action_load.request((self.repo.clone(), action), |(repo, action)| {
                let result = action.run(&repo);
                (action, result)
            });
        }
    }

    pub fn dismiss_failure(&mut self) {
        self.action_failure = None;
    }

    /// Whether the review on screen compares against the worktree, so its files can be changed.
    pub fn reviews_worktree(&self) -> bool {
        self.shown.as_ref().is_some_and(|shown| matches!(shown.review, Review::Changes | Review::Unstaged))
    }

    /// The paths of the file at index `file` that can be staged.
    pub fn stageable(&self, file: usize) -> Vec<String> {
        self.paths_where(file, |status| status.unstaged)
    }

    /// The paths of the file at index `file` that can be unstaged.
    pub fn unstageable(&self, file: usize) -> Vec<String> {
        self.paths_where(file, |status| status.staged)
    }

    /// The paths of the file at index `file` that can be discarded.
    pub fn discardable(&self, file: usize) -> Vec<String> {
        self.paths_where(file, |_| true)
    }

    pub fn stage_file(&mut self, file: usize) {
        self.run(Action::Stage(self.stageable(file)));
    }

    pub fn unstage_file(&mut self, file: usize) {
        self.run(Action::Unstage(self.unstageable(file)));
    }

    /// What the review on screen shows of the file at index `file` goes, once confirmed.
    pub fn ask_to_discard(&mut self, file: usize) {
        let Some(shown) = &self.shown else { return };
        let paths = self.discardable(file);
        self.confirming = match shown.review {
            Review::Changes if !paths.is_empty() => Some(Action::DiscardChanges(paths)),
            Review::Unstaged if !paths.is_empty() => Some(Action::DiscardUnstaged(paths)),
            _ => None,
        };
    }

    pub fn ask_to_drop_stash(&mut self) {
        self.confirming = self.selected_of(Kind::Stashes).map(Action::DropStash);
    }

    pub fn confirm_action(&mut self) {
        if let Some(action) = self.confirming.take() {
            self.run(action);
        }
    }

    pub fn cancel_action(&mut self) {
        self.confirming = None;
    }

    pub fn can_commit(&self) -> bool {
        self.running.is_none() && self.overview.staged_files() > 0 && !self.commit_text.text.trim().is_empty()
    }

    /// Commits what is staged. The message stays until that worked.
    pub fn commit(&mut self) {
        if self.can_commit() {
            self.run(Action::Commit(self.commit_text.text.trim().to_owned()));
        }
    }

    /// Takes the reader to the commit message.
    pub fn write_commit_message(&mut self) {
        if !matches!(self.kind, Kind::Changes | Kind::Unstaged) {
            self.select_kind(Kind::Changes);
        }
        self.focus(|s| &mut s.commit_text);
    }

    pub fn can_stash(&self) -> bool {
        self.running.is_none() && !self.overview.files.is_empty()
    }

    /// The branch selected in the list, unless it is the current one.
    pub fn branch_to_check_out(&self) -> Option<String> {
        self.selected_of(Kind::Branches).filter(|branch| *branch != self.overview.head)
    }

    pub fn check_out_selected(&mut self) {
        if let Some(branch) = self.branch_to_check_out() {
            self.run(Action::Checkout(branch));
        }
    }

    pub fn start_new_branch(&mut self) {
        self.branch_text = TextEdit::default();
        self.naming_branch = true;
        self.focus(|s| &mut s.branch_text);
    }

    pub fn cancel_new_branch(&mut self) {
        self.naming_branch = false;
    }

    pub fn create_branch(&mut self) {
        let name = self.branch_text.text.trim().to_owned();
        if !name.is_empty() {
            self.naming_branch = false;
            self.run(Action::CreateBranch(name));
        }
    }

    /// The stash selected in the list.
    pub fn selected_stash(&self) -> Option<String> {
        self.selected_of(Kind::Stashes)
    }

    pub fn apply_selected_stash(&mut self) {
        if let Some(stash) = self.selected_stash() {
            self.run(Action::ApplyStash(stash));
        }
    }

    pub fn pop_selected_stash(&mut self) {
        if let Some(stash) = self.selected_stash() {
            self.run(Action::PopStash(stash));
        }
    }

    /// Whether pushing can do anything: a branch without an upstream can always be published.
    pub fn can_push(&self) -> bool {
        self.running.is_none() && self.overview.upstream.is_none_or(|upstream| upstream.ahead > 0)
    }

    pub fn open_palette(&mut self) {
        self.palette_text = TextEdit::default();
        self.palette = Some(0);
        self.focus(|s| &mut s.palette_text);
    }

    pub fn close_palette(&mut self) {
        self.palette = None;
    }

    /// What the palette offers for what has been typed into it.
    pub fn palette_commands(&self) -> Vec<Command> {
        let typed = self.palette_text.text.to_lowercase();
        let mut commands = self.commands();
        commands.retain(|command| {
            let title = command.title().to_lowercase();
            typed.split_whitespace().all(|word| title.contains(word))
        });
        commands
    }

    /// Moves the palette's highlight by `rows`, stopping at the ends.
    pub fn move_palette(&mut self, rows: isize) {
        let last = self.palette_commands().len().saturating_sub(1);
        self.palette = self.palette.map(|active| active.saturating_add_signed(rows).min(last));
    }

    /// Closes the palette and does what row `row` offers.
    pub fn run_command(&mut self, row: usize, tx: &mut Tx) {
        if let Some(command) = self.palette_commands().get(row) {
            self.close_palette();
            (command.run)(self, tx);
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

    fn finish_action(&mut self, action: &Action, result: Result<()>) {
        self.running = None;
        match result {
            Ok(()) if matches!(action, Action::Commit(_)) => self.commit_text = TextEdit::default(),
            Ok(()) => {}
            Err(error) => self.action_failure = Some(ActionFailure { action: action.label(), message: error.to_string() }),
        }
        self.refresh();
    }

    /// Everything available right now, by topic. Actions on a file act on the one at the top of
    /// the diff.
    fn commands(&self) -> Vec<Command> {
        let mut commands = Vec::new();
        let mut offer = |available: bool, command: Command| {
            if available {
                commands.push(command);
            }
        };
        let idle = self.running.is_none();
        let file = self.diff.visible_file();
        let files = self.diff.files().len();
        let name = self.diff.files().get(file).and_then(|diff| diff.path().rsplit('/').next()).unwrap_or_default();

        offer(idle && !self.stageable(file).is_empty(), Command::new("File", format!("Stage {name}"), |s, _| s.stage_file(s.diff.visible_file())));
        offer(idle && !self.unstageable(file).is_empty(), Command::new("File", format!("Unstage {name}"), |s, _| s.unstage_file(s.diff.visible_file())));
        offer(idle && !self.discardable(file).is_empty(), Command::new("File", format!("Discard {name}"), |s, _| s.ask_to_discard(s.diff.visible_file())));

        type Step = fn(&mut DiffState, bool, &mut Tx) -> bool;
        let moves: [(bool, &str, &'static str, Step, bool); 4] = [
            (file + 1 < files, "Next file", "]", DiffState::next_file, true),
            (file > 0, "Previous file", "[", DiffState::next_file, false),
            (files > 0, "Next hunk", "j", DiffState::next_hunk, true),
            (files > 0, "Previous hunk", "k", DiffState::next_hunk, false),
        ];
        for (available, label, key, step, forward) in moves {
            offer(available, Command::new("Go to", label, move |s, tx| _ = step(&mut s.diff, forward, tx)).shortcut(key));
        }

        for kind in Kind::ALL {
            offer(kind != self.kind, Command::new("Review", kind.label(), move |s, _| s.select_kind(kind)));
        }

        let any = |test: fn(&FileStatus) -> bool| self.overview.files.values().any(test);
        offer(idle && any(|status| status.unstaged), Command::new("Git", "Stage all", |s, _| s.run(Action::StageAll)));
        offer(idle && any(|status| status.staged), Command::new("Git", "Unstage all", |s, _| s.run(Action::UnstageAll)));
        offer(idle, Command::new("Git", "Commit", |s, _| s.write_commit_message()));
        offer(self.can_stash(), Command::new("Git", "Stash", |s, _| s.run(Action::Stash)));
        if let Some(stash) = self.selected_stash() {
            offer(idle, Command::new("Git", format!("Apply stash {stash}"), |s, _| s.apply_selected_stash()));
            offer(idle, Command::new("Git", format!("Pop stash {stash}"), |s, _| s.pop_selected_stash()));
            offer(idle, Command::new("Git", format!("Drop stash {stash}"), |s, _| s.ask_to_drop_stash()));
        }
        if let Some(branch) = self.branch_to_check_out() {
            offer(idle, Command::new("Git", format!("Check out {branch}"), |s, _| s.check_out_selected()));
        }
        offer(idle, Command::new("Git", "New branch", |s, _| s.start_new_branch()));
        offer(idle, Command::new("Git", "Pull", |s, _| s.run(Action::Pull)));
        offer(self.can_push(), Command::new("Git", "Push", |s, _| s.run(Action::Push)));
        offer(true, Command::new("Git", "Refresh", |s, _| s.refresh()));

        offer(!self.comments.open().is_empty(), Command::new("Comments", "Copy prompt", State::copy_prompt));
        commands
    }

    /// The paths of the file at index `file` whose status passes `test`, in a review of the
    /// worktree. A rename has two.
    fn paths_where(&self, file: usize, test: fn(FileStatus) -> bool) -> Vec<String> {
        let Some(diff) = self.diff.files().get(file).filter(|_| self.reviews_worktree()) else { return Vec::new() };
        let mut paths: Vec<String> = diff.old_path.iter().chain(&diff.new_path).filter(|path| self.overview.files.get(*path).copied().is_some_and(test)).cloned().collect();
        paths.dedup();
        paths
    }

    /// What is selected in the list, when it lists `kind`.
    fn selected_of(&self, kind: Kind) -> Option<String> {
        self.list.selected().next().filter(|_| self.kind == kind).map(str::to_owned)
    }

    fn dialog_open(&self) -> bool {
        self.deleting.is_some() || self.confirming.is_some() || self.naming_branch || self.action_failure.is_some()
    }

    /// Gives the keyboard to one text field, which takes it from every other.
    fn focus(&mut self, field: fn(&mut State) -> &mut TextEdit) {
        for text in [&mut self.draft_text, &mut self.edit_text, &mut self.commit_text, &mut self.branch_text, &mut self.palette_text] {
            text.focused = false;
        }
        field(self).focused = true;
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
            Ok(Loaded { review, patch, commit }) => {
                self.error = None;
                let same_review = self.shown.as_ref().filter(|shown| shown.review == review);
                match same_review.map(|shown| shown.patch == patch) {
                    Some(true) => {}
                    Some(false) => self.update_files(parse(&patch), tx),
                    None => self.show_files(parse(&patch)),
                }
                self.shown = Some(Shown { review, commit, patch });
                if let Some(anchor) = self.reveal.take().and_then(|id| self.comments.get(id)).map(|comment| comment.anchor.clone()) {
                    self.scroll_to_anchor(&anchor, tx);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn open_draft(&mut self, anchor: Anchor) {
        self.draft = Some(anchor);
        self.focus(|s| &mut s.draft_text);
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

    /// Swaps in `files`, a newer state of the review on screen, leaving the reader where they
    /// are. The diff's rows go by a file's position, so when that moves they get the start of
    /// their file.
    fn update_files(&mut self, files: Vec<FileDiff>, tx: &mut Tx) {
        let old = self.diff.files();
        let top = self.diff.visible_file();
        let top_path = old.get(top).map(|file| file.path().to_owned());
        let collapsed: Vec<String> = old.iter().enumerate().filter(|(index, _)| self.diff.collapsed(*index)).map(|(_, file)| file.path().to_owned()).collect();
        let selection = self.diff.selection().and_then(|lines| Some((old.get(lines.file)?.path().to_owned(), lines)));
        let marked = self.files.selected().next().map(str::to_owned);
        let rows = (0..self.files.visible_len()).filter_map(|row| self.files.visible_id(row));
        let closed: Vec<String> = rows.filter(|id| self.files.has_children(id) && !self.files.is_expanded(id)).map(str::to_owned).collect();

        self.show_files(files);

        for folder in &closed {
            self.files.set_expanded(folder, false);
        }
        if let Some(path) = marked {
            self.files.select(&path);
        }
        for path in &collapsed {
            if let Some(file) = self.diff.file_index(path) {
                self.diff.set_collapsed(file, true);
            }
        }
        if let Some((path, lines)) = selection
            && let Some(file) = self.diff.file_index(&path)
            && Anchor::on_lines(&self.diff.files()[file], lines).is_some()
        {
            self.diff.set_selection(Some(LineRange { file, ..lines }));
        }
        if let Some(file) = top_path.and_then(|path| self.diff.file_index(&path)).filter(|file| *file != top) {
            self.diff.scroll_to_file(file, tx);
        }
    }

    /// Shows `files` in the file list, and in the diff in the same order.
    fn show_files(&mut self, mut files: Vec<FileDiff>) {
        let counts = |file: &FileDiff| (file.additions() as u32, file.deletions() as u32);
        self.files = FileTree::new().key("files").compact_folders(true).build(files.iter().map(|file| (file.path().to_owned(), counts(file))));
        self.files.expand_all();
        files.sort_by_key(|file| self.files.visible_row(file.path()));
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rune::Key;

    use super::*;
    use crate::git::testing::Fixture;
    use crate::overview::Upstream;

    const TWO_FILES: &str = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-a\n+A\ndiff --git a/c.txt b/c.txt\n--- a/c.txt\n+++ b/c.txt\n@@ -1 +1 @@\n-c\n+C\n";
    const THREE_FILES: &str = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-a\n+A\ndiff --git a/b.txt b/b.txt\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-b\n+B\ndiff --git a/c.txt b/c.txt\n--- a/c.txt\n+++ b/c.txt\n@@ -1 +1,2 @@\n-c\n+C\n+D\n";

    fn state() -> (Fixture, State) {
        let fixture = Fixture::new();
        let state = State::new(fixture.repo.clone(), Comments::load(fixture.repo.clone()));
        (fixture, state)
    }

    fn land(state: &mut State, review: Review, patch: &str) {
        state.show_patch(Ok(Loaded { review, patch: patch.to_owned(), commit: None }), &mut Tx::default());
    }

    fn file_paths(state: &State) -> Vec<&str> {
        state.diff.files().iter().map(|file| file.path()).collect()
    }

    fn status(staged: bool, unstaged: bool) -> FileStatus {
        FileStatus { staged, unstaged }
    }

    #[test]
    fn the_diff_uses_the_order_of_the_file_list() {
        let (_fixture, mut state) = state();
        let added = |path: &str| format!("diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+x\n");
        let patch: String = ["z.txt", "src/b.txt", "a.txt", "src/a.txt"].map(added).concat();

        land(&mut state, Review::Changes, &patch);

        assert_eq!(file_paths(&state), ["src/a.txt", "src/b.txt", "a.txt", "z.txt"]);
    }

    #[test]
    fn a_file_appearing_in_a_review_takes_its_place_in_the_order() {
        let (_fixture, mut state) = state();
        land(&mut state, Review::Changes, TWO_FILES);

        land(&mut state, Review::Changes, THREE_FILES);

        assert_eq!(file_paths(&state), ["a.txt", "b.txt", "c.txt"]);
    }

    #[test]
    fn an_unchanged_patch_leaves_the_diff_alone() {
        let (_fixture, mut state) = state();
        land(&mut state, Review::Changes, TWO_FILES);
        state.diff.set_selection(Some(LineRange::line(1, rune_kit::diff::LineRef::new(rune_kit::diff::Side::New, 1))));

        land(&mut state, Review::Changes, TWO_FILES);

        assert_eq!(state.diff.selection().map(|lines| lines.file), Some(1));
    }

    #[test]
    fn changes_to_a_review_keep_collapsed_files_and_the_selection_on_their_file() {
        let (_fixture, mut state) = state();
        land(&mut state, Review::Changes, THREE_FILES);
        state.diff.set_collapsed(2, true);
        state.diff.set_selection(Some(LineRange::line(2, rune_kit::diff::LineRef::new(rune_kit::diff::Side::New, 1))));

        land(&mut state, Review::Changes, TWO_FILES);

        assert_eq!(file_paths(&state), ["a.txt", "c.txt"]);
        assert!(state.diff.collapsed(1));
        assert_eq!(state.diff.selection().map(|lines| lines.file), Some(1));
    }

    #[test]
    fn a_draft_whose_lines_went_is_still_reachable() {
        let (_fixture, mut state) = state();
        land(&mut state, Review::Changes, TWO_FILES);
        let line = rune_kit::diff::LineRef::new(rune_kit::diff::Side::New, 1);
        state.compose_on_lines(LineRange::line(1, line));
        assert!(!state.draft_unplaced());

        land(&mut state, Review::Changes, &TWO_FILES[..TWO_FILES.find("diff --git a/c.txt").unwrap()]);

        assert!(state.draft_unplaced());
    }

    #[test]
    fn what_a_file_offers_goes_by_its_status_and_only_in_worktree_reviews() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, false)), ("c.txt".to_owned(), status(true, true))]);
        land(&mut state, Review::Changes, TWO_FILES);

        assert_eq!((state.stageable(0).len(), state.unstageable(0).len()), (0, 1));
        assert_eq!((state.stageable(1).len(), state.unstageable(1).len()), (1, 1));

        land(&mut state, Review::Commit("abc".into()), TWO_FILES);
        assert!(state.stageable(1).is_empty() && state.unstageable(1).is_empty());
    }

    #[test]
    fn a_staged_rename_unstages_both_its_paths() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("old.txt".to_owned(), status(true, false)), ("new.txt".to_owned(), status(true, false))]);
        land(&mut state, Review::Changes, "diff --git a/old.txt b/new.txt\nsimilarity index 90%\nrename from old.txt\nrename to new.txt\n--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-a\n+b\n");

        assert_eq!(state.unstageable(0), ["old.txt", "new.txt"]);
    }

    #[test]
    fn discarding_means_what_the_review_shows() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, true))]);
        let paths = vec!["a.txt".to_owned()];

        land(&mut state, Review::Changes, TWO_FILES);
        state.ask_to_discard(0);
        assert_eq!(state.confirming, Some(Action::DiscardChanges(paths.clone())));

        land(&mut state, Review::Unstaged, TWO_FILES);
        state.ask_to_discard(0);
        assert_eq!(state.confirming, Some(Action::DiscardUnstaged(paths)));
    }

    #[test]
    fn a_failed_commit_keeps_its_message_and_a_good_one_clears_it() {
        let (_fixture, mut state) = state();
        state.commit_text = TextEdit::new("Fix it".to_owned());
        let action = Action::Commit("Fix it".into());

        state.finish_action(&action, Err(crate::git::GitError::new("hook said no")));
        assert_eq!(state.commit_text.text, "Fix it");
        assert_eq!(state.action_failure.as_ref().map(|failure| failure.message.as_str()), Some("hook said no"));

        state.finish_action(&action, Ok(()));
        assert_eq!(state.commit_text.text, "");
    }

    #[test]
    fn nothing_new_to_push_means_no_push() {
        let (_fixture, mut state) = state();
        assert!(state.can_push());

        state.overview.upstream = Some(Upstream { ahead: 0, behind: 3 });
        assert!(!state.can_push());

        state.overview.upstream = Some(Upstream { ahead: 1, behind: 0 });
        assert!(state.can_push());
    }

    #[test]
    fn the_palette_filters_by_every_word_typed_and_stays_on_its_rows() {
        let (_fixture, mut state) = state();
        state.open_palette();
        state.palette_text = TextEdit::new("BRANCH new".to_owned());
        assert_eq!(state.palette_commands().iter().map(|command| command.label.as_str()).collect::<Vec<_>>(), ["New branch"]);

        state.move_palette(5);
        assert_eq!(state.palette, Some(0));
        state.move_palette(-1);
        assert_eq!(state.palette, Some(0));

        state.run_command(0, &mut Tx::default());
        assert_eq!((state.palette, state.naming_branch), (None, true));
    }

    #[test]
    fn the_palette_stays_shut_behind_another_dialog() {
        let (_fixture, mut state) = state();
        let ctrl_k = KeyEvent { key: Key::Command('k'), mods: rune::Mods { ctrl: true, cmd: true, ..rune::Mods::default() } };
        state.confirming = Some(Action::DropStash("abc".into()));

        state.on_key(&ctrl_k, &mut Tx::default());
        assert_eq!(state.palette, None);

        state.cancel_action();
        state.on_key(&ctrl_k, &mut Tx::default());
        assert_eq!(state.palette, Some(0));
    }

    #[test]
    fn ctrl_shift_p_opens_the_palette_but_ctrl_p_does_not() {
        let (_fixture, mut state) = state();
        let ctrl_p = |shift| KeyEvent { key: Key::Command('p'), mods: rune::Mods { ctrl: true, cmd: true, shift, ..rune::Mods::default() } };

        state.on_key(&ctrl_p(false), &mut Tx::default());
        assert_eq!(state.palette, None);

        state.on_key(&ctrl_p(true), &mut Tx::default());
        assert_eq!(state.palette, Some(0));
    }

    #[test]
    fn the_palette_offers_only_the_moves_the_diff_can_make() {
        let (_fixture, mut state) = state();
        let labels = |state: &State| state.commands().into_iter().map(|command| command.label).collect::<Vec<_>>();
        assert!(!labels(&state).iter().any(|label| label.starts_with("Next") || label.starts_with("Previous")));

        land(&mut state, Review::Changes, TWO_FILES);
        let first = labels(&state);
        assert!(first.contains(&"Next file".to_owned()) && !first.contains(&"Previous file".to_owned()), "{first:?}");

        state.diff.scroll_to_file(1, &mut Tx::default());
        let last = labels(&state);
        assert!(!last.contains(&"Next file".to_owned()) && last.contains(&"Previous file".to_owned()), "{last:?}");
    }

    #[test]
    fn the_palette_switches_to_any_other_review_kind() {
        let (_fixture, mut state) = state();
        state.open_palette();
        state.palette_text = TextEdit::new("review".to_owned());
        let labels: Vec<String> = state.palette_commands().into_iter().map(|command| command.label).collect();
        assert_eq!(labels, ["Staged vs unstaged", "Commits", "Branches", "Stashes", "Comments"]);

        state.run_command(1, &mut Tx::default());
        assert_eq!(state.kind, Kind::Commits);
    }

    #[test]
    fn the_palette_acts_on_the_file_at_the_top_of_the_diff() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(false, true))]);
        land(&mut state, Review::Changes, TWO_FILES);

        let labels: Vec<String> = state.commands().into_iter().map(|command| command.label).collect();

        assert!(labels.contains(&"Stage a.txt".to_owned()) && labels.contains(&"Discard a.txt".to_owned()), "{labels:?}");
        assert!(!labels.iter().any(|label| label.starts_with("Unstage")), "{labels:?}");
    }

    #[test]
    fn the_branch_picked_can_be_checked_out_unless_it_is_the_current_one() {
        let (_fixture, mut state) = state();
        state.overview.head = "main".to_owned();
        state.overview.branches = ["main", "feature"].map(|name| Entry { id: name.to_owned(), title: name.to_owned(), detail: String::new() }).to_vec();
        state.select_kind(Kind::Branches);
        assert_eq!(state.branch_to_check_out(), None);

        state.list.select("feature");
        assert_eq!(state.branch_to_check_out().as_deref(), Some("feature"));
    }
}
