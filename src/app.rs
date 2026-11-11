//! The app's state and what changes it.

use std::collections::HashMap;
use std::cell::RefCell;
use std::mem;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use rune::{Key, KeyEvent, Latest, TextEdit, Tx, Waker};
use rune_kit::diff::{Annotation, DiffMode, DiffState, FileDiff, LineRange, LineRef};
use rune_kit::{FileNode, FileTree, TreeModel};
use serde::{Deserialize, Serialize};

use crate::action::{Action, Integration, Stashed};
use crate::comments::{Anchor, Comment, Comments, Group, prompt};
use crate::forge::Hosted;
use crate::gh::{self, PullRequest};
use crate::git::{Repo, Result};
use crate::open::{Choice, Offer, Opened, resolve};
use crate::overview::{Change, Entry, FileStatus, LastCommit, LineCounts, Operation, Overview};
use crate::review::{Loaded, Review, files, parent_branch};
use crate::snippet::Snippet;
use crate::switcher::{BranchChoice, BranchOffer};
use crate::watch::RepoWatch;
use crate::windows::{self, Shared};

/// The id of the annotation holding the comment being written.
pub const DRAFT_NOTE: &str = "draft";
/// How long the copy button says it copied.
const COPIED_FOR: Duration = Duration::from_secs(2);
/// How long what an action did shows.
const OUTCOME_FOR: Duration = Duration::from_secs(3);

/// Something shown for a while. Its time runs from the first poll that sees it.
struct Notice<T> {
    lasts: Duration,
    shown: Option<T>,
    /// The clock time it hides at, once a poll has seen it.
    until: Option<f64>,
}

impl<T> Notice<T> {
    fn new(lasts: Duration) -> Notice<T> {
        Notice { lasts, shown: None, until: None }
    }

    /// Shows `value`, for the full time again if something was showing.
    fn show(&mut self, value: T) {
        self.shown = Some(value);
        self.until = None;
    }

    fn hide(&mut self) {
        self.shown = None;
        self.until = None;
    }

    /// Times it; returns whether it just ended.
    fn step(&mut self, clock: f64, tx: &mut Tx) -> bool {
        if self.shown.is_none() {
            return false;
        }
        let until = *self.until.get_or_insert(clock + self.lasts.as_secs_f64());
        if clock >= until {
            self.hide();
            return true;
        }
        tx.wake_after(Duration::from_secs_f64(until - clock));
        false
    }
}

/// The kinds of thing that can be reviewed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Staging,
    Changes,
    Branches,
    Commits,
    Stashes,
}

impl Kind {
    /// By reach: staging is part of the changes, which are part of the branch. Commits split
    /// the branch up, and stashes sit beside it.
    pub const ALL: [Kind; 5] = [Kind::Staging, Kind::Changes, Kind::Branches, Kind::Commits, Kind::Stashes];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Staging => "Staging",
            Kind::Changes => "Changes",
            Kind::Branches => "Branches",
            Kind::Commits => "Commits",
            Kind::Stashes => "Stashes",
        }
    }

    /// Whether reviewing this kind means picking one from a list.
    pub fn has_list(self) -> bool {
        matches!(self, Kind::Commits | Kind::Branches | Kind::Stashes)
    }
}

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

/// A branch on a forge: the repository, and the branch's name there.
type ForgeBranch = (Hosted, String);

/// A branch's pull request, if it has one, or why looking for it failed.
type Found = std::result::Result<Option<PullRequest>, String>;

/// What the branch on screen has on its forge.
pub enum PullRequestOffer {
    Existing(PullRequest),
    /// The forge's form for a new one, and why looking for one failed, if it did.
    New { url: String, failure: Option<String> },
}

/// What a palette command does.
type Run = Box<dyn Fn(&mut State, &mut Tx)>;

/// Something the command palette offers.
pub struct Command {
    /// What it concerns, such as "Git".
    pub topic: &'static str,
    pub label: String,
    /// The key that does the same outside the palette.
    pub shortcut: Option<&'static str>,
    /// Another key that does the same.
    pub alias: Option<&'static str>,
    /// Whether it can run right now.
    available: bool,
    run: Run,
}

impl Command {
    fn new(topic: &'static str, label: impl Into<String>, run: impl Fn(&mut State, &mut Tx) + 'static) -> Command {
        Command { topic, label: label.into(), shortcut: None, alias: None, available: true, run: Box::new(run) }
    }

    fn shortcut(self, key: &'static str) -> Command {
        Command { shortcut: Some(key), ..self }
    }

    fn alias(self, key: &'static str) -> Command {
        Command { alias: Some(key), ..self }
    }

    /// Its topic and label, as one line.
    pub fn title(&self) -> String {
        format!("{}: {}", self.topic, self.label)
    }
}

/// The topic of commands about the app rather than the repository.
pub const APPLICATION: &str = "Application";
pub const PALETTE: &str = "Ctrl+K";
pub const KEYS: &str = "Ctrl+H";
pub const COMMIT: &str = "Ctrl+Enter";
const STASH: &str = "Ctrl+Shift+S";
pub const PULL: &str = "Ctrl+Shift+↓";
pub const PUSH: &str = "Ctrl+Shift+↑";
const FETCH: &str = "Ctrl+Shift+F";
pub const COPY: &str = "Ctrl+Shift+C";
pub const MERGE: &str = "Ctrl+Shift+M";
pub const PULL_REQUEST: &str = "Ctrl+Shift+R";
/// The key of each view of the repository, in the order of `Kind::ALL`.
const VIEWS: [&str; 5] = ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+5"];
pub const COMMENTS: &str = "Ctrl+0";
const LAYOUT: &str = "Ctrl+\\";

/// The shortcut of the command `event` runs, of those that run only with no dialog open.
fn command_shortcut(event: &KeyEvent) -> Option<&'static str> {
    let ctrl_shift = event.mods.cmd && event.mods.shift;
    match &event.key {
        Key::Enter if event.mods.cmd => Some(COMMIT),
        Key::Down if ctrl_shift => Some(PULL),
        Key::Up if ctrl_shift => Some(PUSH),
        Key::Command(digit @ '1'..='5') => VIEWS.get(*digit as usize - '1' as usize).copied(),
        _ if ctrl_shift && event.is_command('c') => Some(COPY),
        _ if ctrl_shift && event.is_command('s') => Some(STASH),
        _ if ctrl_shift && event.is_command('f') => Some(FETCH),
        _ if ctrl_shift && event.is_command('m') => Some(MERGE),
        _ if ctrl_shift && event.is_command('r') => Some(PULL_REQUEST),
        _ if event.is_command('0') => Some(COMMENTS),
        _ if event.is_command('\\') => Some(LAYOUT),
        _ => None,
    }
}

/// Where the open dialog opens a repository.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    ThisWindow,
    NewWindow,
}

/// What outlives the repository on screen.
#[derive(Default)]
pub struct Shell {
    waker: Option<Waker>,
    /// What every window shares.
    pub shared: Rc<RefCell<Shared>>,
    /// The window's id among them.
    window: u64,
    pub open_text: TextEdit,
    /// The highlighted row of the open dialog, while it is open.
    pub opener: Option<usize>,
    /// What the open dialog offers for the path typed.
    pub offer: Offer,
    offer_load: Latest<Offer>,
    open_load: Latest<std::result::Result<(Opened, Target), String>>,
}

impl Shell {
    pub fn new(shared: Rc<RefCell<Shared>>, window: u64) -> Shell {
        Shell { shared, window, ..Shell::default() }
    }
}

/// The window's state. All of it is about the repository on screen, but for `shell`.
pub struct State {
    /// `None` leaves the rest empty.
    repo: Option<Repo>,
    /// A folder opened outside any repository, which can be made one.
    pub not_repository: Option<PathBuf>,
    /// The review on screen, or to go back to from the comments page.
    pub kind: Kind,
    /// Whether every comment is on screen, in place of the review.
    pub showing_comments: bool,
    pub overview: Overview,
    /// The commits, branches or stashes on offer.
    pub list: TreeModel<Entry>,
    /// The files the shown review changes, but for those wholly staged in a review of the
    /// worktree.
    pub files: TreeModel<FileNode<LineCounts>>,
    /// The staged files, in a review of the worktree.
    pub staged: TreeModel<FileNode<LineCounts>>,
    pub diff: DiffState,
    pub error: Option<String>,
    pub comments: Comments,
    /// The comments page: an entry for every comment with something to show, each with the
    /// snippet it was written on.
    pub comment_sheet: DiffState,
    /// The comment of each entry of the comments page, and whether it was resolved.
    sheet: Vec<(u64, bool)>,
    /// Every comment and its status, as the comments page was last filled.
    sheet_listing: Vec<(u64, Option<&'static str>)>,
    /// The comment opened from the comments page, shown in its review even when resolved.
    shown_comment: Option<u64>,
    /// What the comment being written is about.
    pub draft: Option<Anchor>,
    pub draft_text: TextEdit,
    /// The comment being reworded.
    pub editing: Option<u64>,
    pub edit_text: TextEdit,
    /// The comment waiting for its deletion to be confirmed.
    pub deleting: Option<u64>,
    /// The comments asked to be deleted together.
    pub deleting_group: Option<Group>,
    /// Whether the commit dialog is open.
    pub committing: bool,
    pub commit_text: TextEdit,
    /// What the stash being set up takes, while its dialog is open.
    pub stashing: Option<Stashed>,
    pub stash_text: TextEdit,
    pub branch_text: TextEdit,
    /// The highlighted row of the branch switcher, while it is open.
    pub switcher: Option<usize>,
    /// Whether the remotes dialog is open.
    pub managing_remotes: bool,
    pub remote_name_text: TextEdit,
    pub remote_url_text: TextEdit,
    /// The highlighted row of the merge dialog, while it is open.
    pub merger: Option<usize>,
    pub merge_text: TextEdit,
    /// How the merge dialog brings in the branch picked.
    pub integration: Integration,
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
    /// Whether the list of keys is open.
    pub showing_keys: bool,
    shown: Option<Shown>,
    /// The comment to scroll to once its review is on screen.
    reveal: Option<u64>,
    /// What was copied: one comment, or `None` for every open one.
    copied: Notice<Option<u64>>,
    outcome: Notice<String>,
    overview_load: Latest<Result<Overview>>,
    patch_load: Latest<Result<Loaded>>,
    action_load: Latest<(Action, Result<()>)>,
    /// What GitHub said about each branch's pull request, when last asked.
    pull_requests: HashMap<ForgeBranch, Found>,
    pull_request_load: Latest<Vec<(ForgeBranch, Found)>>,
    /// Whether the forge's form for the current branch's pull request opens once it is pushed.
    creating_pull_request: bool,
    /// The branch the current one grew from, the base of its new pull request.
    head_parent_load: Latest<(ForgeBranch, Result<String>)>,
    watch: Option<RepoWatch>,
    pub shell: Shell,
}

impl State {
    /// A window on what `opened` found, if anything.
    pub fn new(opened: Option<Opened>, shell: Shell) -> State {
        let (layout, kind) = {
            let settings = &shell.shared.borrow().settings;
            (settings.layout(), settings.kind())
        };
        let (mut diff, mut comment_sheet) = (DiffState::default(), DiffState::default());
        diff.set_mode(layout);
        comment_sheet.set_mode(layout);
        let (repo, comments, not_repository) = match opened {
            Some(Opened::Repository(repo, comments)) => (Some(repo), comments, None),
            Some(Opened::Folder(folder)) => (None, Comments::default(), Some(folder)),
            None => (None, Comments::default(), None),
        };
        let mut state = State {
            repo,
            not_repository,
            kind,
            showing_comments: false,
            overview: Overview::default(),
            list: TreeModel::new().key("list"),
            files: TreeModel::new().key("files"),
            staged: TreeModel::new().key("staged"),
            diff,
            error: None,
            comments,
            comment_sheet,
            sheet: Vec::new(),
            sheet_listing: Vec::new(),
            shown_comment: None,
            draft: None,
            draft_text: TextEdit::default(),
            editing: None,
            edit_text: TextEdit::default(),
            deleting: None,
            deleting_group: None,
            committing: false,
            commit_text: TextEdit::default(),
            stashing: None,
            stash_text: TextEdit::default(),
            branch_text: TextEdit::default(),
            switcher: None,
            managing_remotes: false,
            remote_name_text: TextEdit::default(),
            remote_url_text: TextEdit::default(),
            merger: None,
            merge_text: TextEdit::default(),
            integration: Integration::Merge,
            confirming: None,
            running: None,
            action_failure: None,
            watch_error: None,
            palette_text: TextEdit::default(),
            palette: None,
            showing_keys: false,
            shown: None,
            reveal: None,
            copied: Notice::new(COPIED_FOR),
            outcome: Notice::new(OUTCOME_FOR),
            overview_load: Latest::default(),
            patch_load: Latest::default(),
            action_load: Latest::default(),
            pull_requests: HashMap::new(),
            pull_request_load: Latest::default(),
            creating_pull_request: false,
            head_parent_load: Latest::default(),
            watch: None,
            shell,
        };
        state.rebuild_sheet();
        state
    }

    pub fn setup(&mut self, waker: Waker) {
        self.shell.offer_load.set_waker(waker.clone());
        self.shell.open_load.set_waker(waker.clone());
        self.shell.waker = Some(waker);
        self.start();
    }

    /// Lands finished loads; returns whether any did.
    pub fn poll(&mut self, clock: f64, tx: &mut Tx) -> bool {
        let opened = self.shell.open_load.take();
        let opened_landed = opened.is_some();
        match opened {
            Some(Ok((opened, target))) => {
                let shown = opened.root().is_some_and(|root| self.shell.shared.borrow().focus(root));
                match target {
                    _ if shown => {}
                    Target::NewWindow => windows::open(opened, &self.shell.shared),
                    Target::ThisWindow => {
                        // What else landed was about the repository it replaces.
                        self.show_opened(opened);
                        tx.set_title(self.title());
                        return true;
                    }
                }
            }
            Some(Err(message)) => self.action_failure = Some(ActionFailure { action: "Open", message }),
            None => {}
        }
        let offer = self.shell.offer_load.take();
        let overview = self.overview_load.take();
        let patch = self.patch_load.take();
        let comments = self.comments.poll();
        let action = self.action_load.take();
        let pull_requests = self.pull_request_load.take();
        let head_parent = self.head_parent_load.take();
        let (repo_changed, watch_failure) = self.watch.as_ref().map_or((false, None), RepoWatch::take);
        let landed = opened_landed || offer.is_some() || overview.is_some() || patch.is_some() || comments || action.is_some() || pull_requests.is_some() || head_parent.is_some() || watch_failure.is_some();
        self.pull_requests.extend(pull_requests.into_iter().flatten());
        if let Some(offer) = offer {
            self.show_offer(offer);
        }
        if watch_failure.is_some() {
            self.watch_error = watch_failure;
        }
        if repo_changed {
            self.refresh();
        }
        if let Some((action, result)) = action {
            self.finish_action(&action, result);
        }
        if let Some(((hosted, branch), parent)) = head_parent {
            match parent {
                Ok(parent) => self.open_in_browser(&hosted.compare_url(self.overview.forge_branch(&parent), &branch), tx),
                Err(error) => self.action_failure = Some(ActionFailure { action: "Create pull request", message: error.to_string() }),
            }
        }
        // After the action, so what it did starts its time now.
        let copied_ended = self.copied.step(clock, tx);
        let outcome_ended = self.outcome.step(clock, tx);
        if let Some(overview) = overview {
            self.show_overview(overview);
        }
        if comments {
            self.comments_changed();
        }
        if let Some(patch) = patch {
            self.show_patch(patch, tx);
        }
        landed || copied_ended || outcome_ended
    }

    pub fn on_key(&mut self, event: &KeyEvent, tx: &mut Tx) -> bool {
        if self.confirm_by_key(event) {
            return true;
        }
        if let Some(shortcut) = command_shortcut(event) {
            if !self.dialog_open() && self.palette.is_none() {
                self.run_shortcut(shortcut, tx);
            }
            return true;
        }
        if event.is_command('k') {
            if !self.dialog_open() {
                self.open_palette();
            }
            return true;
        }
        if event.is_command('h') {
            if self.showing_keys {
                self.close_keys();
            } else if !self.dialog_open() {
                self.open_keys();
            }
            return true;
        }
        if event.is_command('o') {
            if !self.dialog_open() {
                self.open_opener();
            }
            return true;
        }
        // With Shift too, as an alias.
        if event.is_command('b') {
            if !self.dialog_open() {
                self.open_switcher();
            }
            return true;
        }
        self.active_diff().key(event, tx)
    }

    /// The diff on screen: the review's, or the comments page.
    fn active_diff(&mut self) -> &mut DiffState {
        if self.showing_comments { &mut self.comment_sheet } else { &mut self.diff }
    }

    /// Runs the command whose key is `shortcut`, if it is on offer.
    fn run_shortcut(&mut self, shortcut: &str, tx: &mut Tx) {
        if let Some(command) = self.commands().into_iter().find(|command| command.shortcut == Some(shortcut)) {
            (command.run)(self, tx);
        }
    }

    pub fn has_repository(&self) -> bool {
        self.repo.is_some()
    }

    /// `<user-or-org>/<project>` when a remote says, else the name of the worktree's folder.
    pub fn repo_name(&self) -> String {
        self.overview.hosted.as_ref().map(Hosted::name).or_else(|| self.repo.as_ref().map(Repo::name)).unwrap_or_default()
    }

    /// The window's title: the name of the folder on screen, else the app's.
    pub fn title(&self) -> String {
        let folder = self.not_repository.as_deref().and_then(Path::file_name).map(|name| name.to_string_lossy().into_owned());
        self.repo.as_ref().map(Repo::name).or(folder).unwrap_or_else(|| crate::NAME.to_owned())
    }

    /// Whether another repository can be opened: not while an action runs, whose outcome would
    /// go unseen.
    pub fn can_open(&self) -> bool {
        self.running.is_none()
    }

    /// Opens the open dialog, offering the recent repositories.
    pub fn open_opener(&mut self) {
        if !self.can_open() {
            return;
        }
        self.palette = None;
        self.shell.open_text = TextEdit::default();
        self.shell.offer = Offer::default();
        self.shell.opener = Some(0);
        self.focus(|s| &mut s.shell.open_text);
        self.offer_folders();
    }

    pub fn close_opener(&mut self) {
        self.shell.opener = None;
    }

    /// Asks what to offer for the path typed into the open dialog. The repository on screen is
    /// no use among the recent ones.
    pub fn offer_folders(&mut self) {
        let current = self.repo.as_ref().map(Repo::root);
        let recent: Vec<PathBuf> = self.shell.shared.borrow().recents.roots().iter().filter(|root| Some(root.as_path()) != current).cloned().collect();
        self.shell.offer_load.request((self.shell.open_text.text.clone(), recent), |(typed, recent)| Offer::new(&typed, &recent));
    }

    /// Whether the open dialog waits for what to offer.
    pub fn offer_pending(&self) -> bool {
        self.shell.offer_load.is_pending()
    }

    /// Moves the open dialog's highlight by `rows`, stopping at the ends.
    pub fn move_opener(&mut self, rows: isize) {
        let last = self.shell.offer.choices.len().saturating_sub(1);
        self.shell.opener = self.shell.opener.map(|active| active.saturating_add_signed(rows).min(last));
    }

    /// Completes the path typed with the highlighted folder.
    pub fn complete_path(&mut self) {
        let Some(choice) = self.highlighted_choice() else { return };
        self.shell.open_text = TextEdit::new(choice.completion.clone());
        self.focus(|s| &mut s.shell.open_text);
        self.offer_folders();
    }

    /// Opens the folder on row `row` of the open dialog, or the path typed when nothing is on
    /// offer, in `target`. A repository another window shows comes to the front there instead.
    pub fn open_choice(&mut self, row: usize, target: Target) {
        let typed = self.shell.open_text.text.trim();
        let folder = match self.shell.offer.choices.get(row) {
            Some(choice) => choice.path.clone(),
            None if !typed.is_empty() => resolve(typed),
            None => return,
        };
        self.close_opener();
        self.shell.open_load.request(folder, move |folder| Opened::find(&folder).map(|opened| (opened, target)));
    }

    /// Makes the folder on screen a repository, and opens it.
    pub fn init_folder(&mut self) {
        if let Some(folder) = self.not_repository.clone() {
            self.shell.open_load.request(folder, |folder| Opened::init(&folder).map(|opened| (opened, Target::ThisWindow)));
        }
    }

    /// [`State::refresh`], asking the forge again about pull requests too.
    pub fn refresh_all(&mut self) {
        self.refresh();
        self.look_for_pull_requests(true);
    }

    /// Lets the other windows know this one is gone.
    pub fn on_close(&mut self, _: &mut Tx) {
        self.shell.shared.borrow_mut().forget(self.shell.window);
    }

    /// Asks the forge again about pull requests when the window comes back, as one may have been
    /// opened or merged in the browser meanwhile.
    pub fn on_focus(&mut self, focused: bool, _: &mut Tx) {
        if focused {
            self.look_for_pull_requests(true);
        }
    }

    /// Reads the repository again, then the review on screen, and finds comments' code again.
    pub fn refresh(&mut self) {
        let Some(repo) = &self.repo else { return };
        self.overview_load.request(repo.clone(), |repo| Overview::load(&repo));
        self.comments.follow_code(repo);
    }

    pub fn select_kind(&mut self, kind: Kind) {
        self.showing_comments = false;
        self.kind = kind;
        self.shell.shared.borrow_mut().settings.set_kind(kind);
        self.list.clear_selection();
        self.rebuild_list();
        self.show_selected();
    }

    /// Shows the entry just selected in the list.
    pub fn show_selected(&mut self) {
        self.shown_comment = None;
        self.place_annotations();
        self.load_review();
    }

    /// Shows every comment in place of the review, or the review again.
    pub fn toggle_comments(&mut self) {
        self.showing_comments = !self.showing_comments;
    }

    /// Where the review a comment was written in shows: its kind, and its entry in the list
    /// when the kind has one. `None` when the review is no longer on offer.
    pub fn review_place(&self, review: &Review) -> Option<(Kind, Option<String>)> {
        let listed = |entries: &[Entry], id: &str| entries.iter().any(|entry| entry.id == id).then(|| Some(id.to_owned()));
        match review {
            Review::Changes => Some((Kind::Changes, None)),
            Review::Unstaged => Some((Kind::Staging, None)),
            Review::Commit(commit) => Some((Kind::Commits, listed(&self.overview.commits, commit)?)),
            Review::Branch { branch, .. } => Some((Kind::Branches, listed(&self.overview.branches, branch)?)),
            Review::Stash(stash) => Some((Kind::Stashes, listed(&self.overview.stashes, stash)?)),
        }
    }

    /// Opens the review the comment `id` was written in, at the comment.
    pub fn show_in_diff(&mut self, id: u64) {
        let Some((kind, entry)) = self.comments.get(id).and_then(|comment| self.review_place(&comment.review)) else { return };
        self.select_kind(kind);
        if let Some(entry) = entry {
            self.list.select(&entry);
        }
        self.shown_comment = Some(id);
        self.reveal = Some(id);
        self.place_annotations();
        self.load_review();
    }

    /// Lays out every diff as `layout`, from now on.
    pub fn set_layout(&mut self, layout: DiffMode) {
        self.diff.set_mode(layout);
        self.comment_sheet.set_mode(layout);
        self.shell.shared.borrow_mut().settings.set_layout(layout);
    }

    /// The comment of the entry `file` of the comments page.
    pub fn sheet_comment(&self, file: usize) -> Option<&Comment> {
        self.sheet.get(file).and_then(|(id, _)| self.comments.get(*id))
    }

    /// The comments the comments page has no entry for, having nothing to show of their code.
    pub fn comments_off_sheet(&self) -> Vec<&Comment> {
        self.comments.listed().into_iter().filter(|comment| !self.sheet.iter().any(|(id, _)| *id == comment.id)).collect()
    }

    /// Shows the entry selected in the list. A branch picked from the list is compared with the
    /// branch it grew from, which takes git to find.
    pub fn load_review(&mut self) {
        let find_base = self.kind == Kind::Branches;
        match self.repo.clone().zip(self.review()) {
            Some(request) => self.patch_load.request(request, move |(repo, review)| match review {
                Review::Branch { base, branch } if find_base => Review::of_branch(&repo, &branch, &base)?.load(&repo),
                review => review.load(&repo),
            }),
            None => {
                self.patch_load.cancel();
                self.shown = None;
                self.files.clear();
                self.show_files(Vec::new());
            }
        }
    }

    /// What is selected for review, if anything is. A branch from the list has the default
    /// branch as its base, for [`Review::of_branch`] to fall back on.
    fn review(&self) -> Option<Review> {
        let selected = || self.list.selected().next().map(str::to_owned);
        match self.kind {
            Kind::Changes => Some(Review::Changes),
            Kind::Staging => Some(Review::Unstaged),
            Kind::Commits => selected().map(Review::Commit),
            Kind::Branches => selected().map(|branch| Review::Branch { base: self.overview.base.clone(), branch }),
            Kind::Stashes => selected().map(Review::Stash),
        }
    }

    /// The comment opened from the comments page or the one being reworded, when the diff on
    /// screen has nowhere to show it.
    pub fn unplaced_comment(&self) -> Option<&Comment> {
        let shown = self.shown_comment.and_then(|id| self.comments.get(id));
        let editing = self.editing.and_then(|id| self.comments.get(id));
        shown.or(editing).filter(|comment| comment.anchor.position(self.diff.files()).is_none())
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
            let files = self.diff.files();
            let snippet = anchor.position(files).and_then(|(file, _)| Snippet::new(&files[file], &anchor));
            self.comments.add(anchor, shown.review.clone(), shown.commit.clone(), snippet, body);
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
        if let Some(repo) = &self.repo {
            self.comments.follow_code(repo);
        }
        self.comments_changed();
    }

    /// Opens the comment's wording for editing, leaving any comment being written as it is.
    pub fn start_edit(&mut self, id: u64) {
        let Some(comment) = self.comments.get(id) else { return };
        self.edit_text = TextEdit::new(comment.body.clone());
        self.focus(|s| &mut s.edit_text);
        self.editing = Some(id);
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

    /// Goes ahead with the open confirm dialog on Ctrl+Enter; returns whether it did.
    pub fn confirm_by_key(&mut self, event: &KeyEvent) -> bool {
        if event.key != Key::Enter || !event.mods.cmd {
            return false;
        }
        if self.deleting.is_some() {
            self.confirm_delete();
        } else if self.deleting_group.is_some() {
            self.confirm_delete_group();
        } else if self.confirming.is_some() {
            self.confirm_action();
        } else {
            return false;
        }
        true
    }

    pub fn resolve_all_comments(&mut self) {
        self.comments.resolve_all();
        self.comments_changed();
    }

    pub fn ask_to_delete_group(&mut self, group: Group) {
        self.deleting_group = Some(group);
    }

    pub fn confirm_delete_group(&mut self) {
        if let Some(group) = self.deleting_group.take() {
            self.comments.delete_group(group);
            self.comments_changed();
        }
    }

    pub fn cancel_delete_group(&mut self) {
        self.deleting_group = None;
    }

    /// The open comments whose code is still there, which copying them all takes.
    pub fn comments_to_copy(&self) -> Vec<&Comment> {
        self.comments.open().into_iter().filter(|comment| !comment.outdated).collect()
    }

    /// Copies every open comment whose code is still there as a prompt for an agent.
    pub fn copy_open_comments(&mut self, tx: &mut Tx) {
        tx.copy(prompt(&self.comments_to_copy()));
        self.copied.show(None);
    }

    /// Copies the comment `id` as a prompt for an agent, whatever its status.
    pub fn copy_comment(&mut self, id: u64, tx: &mut Tx) {
        let Some(comment) = self.comments.get(id) else { return };
        tx.copy(prompt(&[comment]));
        self.copied.show(Some(id));
    }

    /// Whether the comment `id`, or every open one for `None`, was just copied.
    pub fn just_copied(&self, which: Option<u64>) -> bool {
        self.copied.shown == Some(which)
    }

    /// What the last action did, for a while after it finished.
    pub fn outcome(&self) -> Option<&str> {
        self.outcome.shown.as_deref()
    }

    pub fn dismiss_outcome(&mut self) {
        self.outcome.hide();
    }

    /// Runs `action` in the background, unless one is running already.
    pub fn run(&mut self, action: Action) {
        if let Some(repo) = self.repo.clone().filter(|_| self.running.is_none()) {
            self.running = Some(action.label());
            self.action_load.request((repo, action), |(repo, action)| {
                let result = action.run(&repo);
                (action, result)
            });
        }
    }

    pub fn dismiss_failure(&mut self) {
        self.action_failure = None;
    }

    /// What the diff on screen compares.
    pub fn shown_review(&self) -> Option<&Review> {
        self.shown.as_ref().map(|shown| &shown.review)
    }

    /// Whether the review on screen compares against the worktree, so its files can be changed.
    pub fn reviews_worktree(&self) -> bool {
        self.shown.as_ref().is_some_and(|shown| matches!(shown.review, Review::Changes | Review::Unstaged))
    }

    /// The paths of the file at index `file` that can be staged.
    pub fn stageable(&self, file: usize) -> Vec<String> {
        self.paths_where(file, |status| status.unstaged.is_some())
    }

    /// The paths of the file at index `file` that can be unstaged.
    pub fn unstageable(&self, file: usize) -> Vec<String> {
        self.paths_where(file, |status| status.staged.is_some())
    }

    /// The paths of the file at index `file` that can be discarded.
    pub fn discardable(&self, file: usize) -> Vec<String> {
        self.paths_where(file, |_| true)
    }

    /// The paths at or under `path` in the file list that can be staged.
    pub fn stageable_in(&self, path: &str) -> Vec<String> {
        let files: Vec<usize> = self.files.subtree(path).filter(|node| !node.dir).filter_map(|node| self.diff.file_index(&node.path)).collect();
        files.into_iter().flat_map(|file| self.stageable(file)).collect()
    }

    /// The paths at or under `path` in the staged list.
    pub fn unstageable_in(&self, path: &str) -> Vec<String> {
        self.staged.subtree(path).filter(|node| !node.dir).map(|node| node.path.clone()).collect()
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

    /// What discarding everything means in the view on screen: back to `HEAD` in Changes, back
    /// to what is staged in Staging.
    fn discard_all(&self) -> Option<Action> {
        let paths = |test: fn(FileStatus) -> bool| {
            let mut paths: Vec<String> = self.overview.files.iter().filter(|(_, status)| test(**status)).map(|(path, _)| path.clone()).collect();
            paths.sort_unstable();
            paths
        };
        let action = match self.kind {
            _ if self.showing_comments => return None,
            Kind::Changes => Action::DiscardChanges(paths(|_| true)),
            Kind::Staging => Action::DiscardUnstaged(paths(|status| status.unstaged.is_some())),
            _ => return None,
        };
        match &action {
            Action::DiscardChanges(paths) | Action::DiscardUnstaged(paths) if paths.is_empty() => None,
            _ => Some(action),
        }
    }

    /// Everything the view on screen shows goes, once confirmed.
    pub fn ask_to_discard_all(&mut self) {
        self.confirming = self.discard_all();
    }

    /// How many of the files `action` discards it deletes, as nothing else holds them.
    pub fn deleted_by(&self, action: &Action) -> usize {
        let (paths, back_to_head) = match action {
            Action::DiscardChanges(paths) => (paths, true),
            Action::DiscardUnstaged(paths) => (paths, false),
            _ => return 0,
        };
        let deleted = |status: &FileStatus| match (status.staged, status.unstaged) {
            (Some(Change::New), _) => back_to_head,
            (None, Some(Change::New)) => true,
            _ => false,
        };
        paths.iter().filter_map(|path| self.overview.files.get(path)).filter(|status| deleted(status)).count()
    }

    /// The last commit, while undoing it can leave its changes staged: it has one parent, and
    /// no merge or rebase is under way.
    fn undoable(&self) -> Option<&LastCommit> {
        self.overview.last_commit.as_ref().filter(|commit| commit.parents == 1 && self.overview.operation.is_none())
    }

    /// Whether the upstream holds the last commit already.
    pub fn head_pushed(&self) -> bool {
        self.overview.upstream.is_some_and(|upstream| upstream.ahead == 0)
    }

    /// Undoes the last commit, keeping its changes staged, once confirmed if it was pushed.
    pub fn undo_commit(&mut self) {
        let Some(last) = self.undoable() else { return };
        let action = Action::UndoCommit { commit: last.hash.clone(), message: last.message.clone() };
        if self.head_pushed() {
            self.confirming = Some(action);
        } else {
            self.run(action);
        }
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

    pub fn can_stage_all(&self) -> bool {
        self.running.is_none() && self.overview.files.values().any(|status| status.unstaged.is_some())
    }

    pub fn can_unstage_all(&self) -> bool {
        self.running.is_none() && self.overview.files.values().any(|status| status.staged.is_some())
    }

    /// Whether there is anything to commit, and no merge or rebase to finish first. A conflict
    /// left by applying a stash stops it too, as committing everything would stage it.
    pub fn can_start_commit(&self) -> bool {
        self.running.is_none() && !self.overview.files.is_empty() && self.overview.operation.is_none() && self.overview.conflicted().is_empty()
    }

    pub fn can_commit(&self) -> bool {
        self.can_start_commit() && !self.commit_text.text.trim().is_empty()
    }

    /// Opens the commit dialog, with any message written before.
    pub fn start_commit(&mut self) {
        self.committing = true;
        self.focus(|s| &mut s.commit_text);
    }

    /// Closes the commit dialog, keeping the message.
    pub fn cancel_commit(&mut self) {
        self.committing = false;
    }

    /// Commits what is staged, or everything when nothing is. The message stays until that
    /// worked.
    pub fn commit(&mut self) {
        if self.can_commit() {
            self.committing = false;
            let message = self.commit_text.text.trim().to_owned();
            self.run(if self.overview.commit_plan().takes_all { Action::CommitAll(message) } else { Action::Commit(message) });
        }
    }

    /// Whether there is anything to stash.
    pub fn can_stash(&self) -> bool {
        self.running.is_none() && !self.overview.files.is_empty()
    }

    /// Opens the stash dialog, taking new files too unless told otherwise.
    pub fn start_stash(&mut self) {
        self.stash_text = TextEdit::default();
        self.stashing = Some(Stashed::WithNewFiles);
        self.focus(|s| &mut s.stash_text);
    }

    pub fn cancel_stash(&mut self) {
        self.stashing = None;
    }

    /// Whether the stash being set up would take anything.
    pub fn stash_ready(&self) -> bool {
        self.stashing.is_some_and(|takes| self.can_stash() && (takes != Stashed::Staged || self.overview.staged_files() > 0))
    }

    pub fn stash(&mut self) {
        let Some(takes) = self.stashing.filter(|_| self.stash_ready()) else { return };
        self.stashing = None;
        self.run(Action::Stash { name: self.stash_text.text.trim().to_owned(), takes });
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

    /// Opens the branch switcher, offering every branch.
    pub fn open_switcher(&mut self) {
        self.palette = None;
        self.branch_text = TextEdit::default();
        self.switcher = Some(0);
        self.focus(|s| &mut s.branch_text);
    }

    pub fn close_switcher(&mut self) {
        self.switcher = None;
    }

    /// What the branch switcher offers for the name typed.
    pub fn branch_offer(&self) -> BranchOffer {
        BranchOffer::new(&self.branch_text.text, &self.overview.branches, &self.overview.remote_branches)
    }

    /// Moves the branch switcher's highlight by `rows`, stopping at the ends.
    pub fn move_switcher(&mut self, rows: isize) {
        let last = self.branch_offer().choices.len().saturating_sub(1);
        self.switcher = self.switcher.map(|active| active.saturating_add_signed(rows).min(last));
    }

    /// Switches to what row `row` of the branch switcher offers, creating it first if it is new,
    /// and closes the switcher. It stays open while an action runs, which would refuse the switch.
    pub fn pick_branch(&mut self, row: usize) {
        if self.running.is_some() {
            return;
        }
        let Some(choice) = self.branch_offer().choices.into_iter().nth(row) else { return };
        self.close_switcher();
        match choice {
            BranchChoice::Local(branch) if branch.id == self.overview.head => {}
            BranchChoice::Local(branch) => self.run(Action::Checkout(branch.id)),
            BranchChoice::Remote(branch) => self.run(Action::Track { remote: branch.remote, branch: branch.name }),
            BranchChoice::Create(name) => self.run(Action::CreateBranch(name)),
        }
    }

    /// The selected branch is deleted, once confirmed. Not the current one.
    pub fn ask_to_delete_branch(&mut self) {
        self.confirming = self.branch_to_check_out().map(Action::DeleteBranch);
    }

    /// Opens the merge dialog: the branches to bring in, or the merge or rebase that stopped.
    pub fn open_merger(&mut self) {
        self.palette = None;
        self.merge_text = TextEdit::default();
        self.integration = Integration::Merge;
        self.merger = Some(0);
        self.focus(|s| &mut s.merge_text);
    }

    pub fn close_merger(&mut self) {
        self.merger = None;
    }

    /// What the merge dialog offers for the name typed: every branch but the current one.
    pub fn merge_offer(&self) -> BranchOffer {
        let others: Vec<Entry> = self.overview.branches.iter().filter(|branch| branch.id != self.overview.head).cloned().collect();
        BranchOffer::existing(&self.merge_text.text, &others, &self.overview.remote_branches)
    }

    /// Moves the merge dialog's highlight by `rows`, stopping at the ends.
    pub fn move_merger(&mut self, rows: isize) {
        let last = self.merge_offer().choices.len().saturating_sub(1);
        self.merger = self.merger.map(|active| active.saturating_add_signed(rows).min(last));
    }

    /// The branch on row `row` of the merge dialog, as git knows it.
    pub fn merge_choice(&self, row: usize) -> Option<String> {
        match self.merge_offer().choices.into_iter().nth(row)? {
            BranchChoice::Local(branch) => Some(branch.id),
            BranchChoice::Remote(branch) => Some(branch.id()),
            BranchChoice::Create(_) => None,
        }
    }

    /// Whether a branch can be brought in: there is a commit to bring it into, and nothing else
    /// under way.
    pub fn can_integrate(&self) -> bool {
        self.running.is_none() && self.overview.operation.is_none() && self.overview.last_commit.is_some()
    }

    /// Brings in the branch on row `row` of the merge dialog the way picked, and closes it.
    pub fn integrate(&mut self, row: usize) {
        let Some(branch) = self.merge_choice(row).filter(|_| self.can_integrate()) else { return };
        self.close_merger();
        self.run(Action::Integrate(self.integration, branch));
    }

    /// How the merge or rebase that stopped brings its branch in.
    fn stopped(&self) -> Option<Integration> {
        self.overview.operation.as_ref().map(|operation| match operation {
            Operation::Merge { .. } => Integration::Merge,
            Operation::Rebase { .. } => Integration::Rebase,
        })
    }

    /// Whether the stopped merge or rebase can go on: no file is left conflicted.
    pub fn can_continue(&self) -> bool {
        self.running.is_none() && self.stopped().is_some() && self.overview.conflicted().is_empty()
    }

    pub fn continue_operation(&mut self) {
        if let Some(way) = self.stopped().filter(|_| self.can_continue()) {
            self.close_merger();
            self.run(Action::Continue(way));
        }
    }

    /// Gives up the stopped merge or rebase, once confirmed.
    pub fn ask_to_abort(&mut self) {
        if let Some(way) = self.stopped().filter(|_| self.running.is_none()) {
            self.close_merger();
            self.confirming = Some(Action::Abort(way));
        }
    }

    pub fn can_fetch(&self) -> bool {
        self.running.is_none() && !self.overview.remotes.is_empty()
    }

    /// Opens the remotes dialog, with nothing typed.
    pub fn open_remotes(&mut self) {
        self.close_switcher();
        self.palette = None;
        self.remote_name_text = TextEdit::default();
        self.remote_url_text = TextEdit::default();
        self.managing_remotes = true;
        self.focus(|s| &mut s.remote_url_text);
    }

    pub fn close_remotes(&mut self) {
        self.managing_remotes = false;
    }

    /// The name a remote gets when none is typed: `origin`, unless a remote has it.
    pub fn default_remote_name(&self) -> Option<&'static str> {
        (!self.overview.remotes.iter().any(|remote| remote.name == "origin")).then_some("origin")
    }

    /// The remote to add, once it has a name and a URL.
    fn remote_to_add(&self) -> Option<Action> {
        let typed = self.remote_name_text.text.trim();
        let name = if typed.is_empty() { self.default_remote_name()? } else { typed };
        let url = self.remote_url_text.text.trim();
        (!url.is_empty()).then(|| Action::AddRemote { name: name.to_owned(), url: url.to_owned() })
    }

    pub fn can_add_remote(&self) -> bool {
        self.running.is_none() && self.remote_to_add().is_some()
    }

    /// Adds the remote typed, keeping the dialog open to show it.
    pub fn add_remote(&mut self) {
        if let Some(action) = self.remote_to_add().filter(|_| self.can_add_remote()) {
            self.run(action);
        }
    }

    /// Removes the remote `name`, once confirmed.
    pub fn ask_to_remove_remote(&mut self, name: &str) {
        self.confirming = Some(Action::RemoveRemote(name.to_owned()));
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

    /// The branch the Branches review shows, on the forge, with its base there: none when the
    /// base is no branch. `None` when the repository is on no forge, or the branch is compared
    /// with itself.
    fn branch_on_forge(&self) -> Option<(ForgeBranch, Option<&str>)> {
        let Some(Review::Branch { base, branch }) = self.shown_review().filter(|_| self.kind == Kind::Branches) else { return None };
        let hosted = self.overview.hosted.clone().filter(|_| base != branch)?;
        Some(((hosted, branch.clone()), self.overview.forge_branch(base)))
    }

    /// The current branch on its forge, unless it is the default branch.
    fn head_on_forge(&self) -> Option<ForgeBranch> {
        let hosted = self.overview.hosted.clone()?;
        let head = &self.overview.head;
        let feature = *head != self.overview.base && self.overview.branches.iter().any(|branch| branch.id == *head);
        feature.then(|| (hosted, head.clone()))
    }

    /// The current branch on GitHub, unless it is the default branch.
    fn head_on_github(&self) -> Option<ForgeBranch> {
        self.head_on_forge().filter(|(hosted, _)| hosted.is_github())
    }

    /// Asks GitHub for the pull requests of the current branch and of the branch on screen: those
    /// it hasn't asked about, or all of them `again`.
    fn look_for_pull_requests(&mut self, again: bool) {
        let shown = self.branch_on_forge().map(|(branch, _)| branch).filter(|(hosted, _)| hosted.is_github());
        let mut asked: Vec<ForgeBranch> = self.head_on_github().into_iter().chain(shown).filter(|branch| again || !self.pull_requests.contains_key(branch)).collect();
        asked.dedup();
        if asked.is_empty() {
            return;
        }
        let look = |asked: Vec<ForgeBranch>| {
            asked.into_iter().map(|branch| {
                let found = gh::pull_request(&branch.0, &branch.1);
                (branch, found)
            })
            .collect()
        };
        if again {
            self.pull_request_load.request(asked, look);
        } else {
            self.pull_request_load.request_if_changed(&asked, look);
        }
    }

    /// The current branch's pull request, once GitHub said.
    pub fn head_pull_request(&self) -> Option<&PullRequest> {
        self.pull_requests.get(&self.head_on_github()?)?.as_ref().ok()?.as_ref()
    }

    /// What the branch on screen has on its forge, once known: on GitHub, that takes asking.
    pub fn pull_request_offer(&self) -> Option<PullRequestOffer> {
        let ((hosted, branch), base) = self.branch_on_forge()?;
        let new = |failure| PullRequestOffer::New { url: hosted.compare_url(base, &branch), failure };
        if !hosted.is_github() {
            return Some(new(None));
        }
        Some(match self.pull_requests.get(&(hosted.clone(), branch.clone()))? {
            Ok(Some(existing)) => PullRequestOffer::Existing(existing.clone()),
            Ok(None) => new(None),
            Err(failure) => new(Some(failure.clone())),
        })
    }

    /// Opens the pull request of the branch on screen in the browser, or the forge's form for one.
    pub fn open_pull_request(&mut self, tx: &mut Tx) {
        let url = match self.pull_request_offer() {
            Some(PullRequestOffer::Existing(existing)) => existing.url,
            Some(PullRequestOffer::New { url, .. }) => url,
            None => return,
        };
        self.open_in_browser(&url, tx);
    }

    /// Opens the current branch's pull request in the browser, or else the forge's form for one,
    /// pushing first what the forge doesn't have yet.
    pub fn open_head_pull_request(&mut self, tx: &mut Tx) {
        if let Some(url) = self.head_pull_request().map(|found| found.url.clone()) {
            self.open_in_browser(&url, tx);
            return;
        }
        if self.head_on_forge().is_none() || self.running.is_some() {
            return;
        }
        if self.overview.upstream.is_none_or(|upstream| upstream.ahead > 0) {
            self.creating_pull_request = true;
            self.run(Action::Push);
        } else {
            self.find_head_parent();
        }
    }

    /// Looks for the branch the current one grew from, in the background, to open the forge's
    /// form for a pull request into it.
    fn find_head_parent(&mut self) {
        let Some((repo, branch)) = self.repo.clone().zip(self.head_on_forge()) else { return };
        let fallback = self.overview.base.clone();
        self.head_parent_load.request((repo, branch, fallback), |(repo, branch, fallback)| {
            let parent = parent_branch(&repo, &branch.1, &fallback);
            (branch, parent)
        });
    }

    fn open_in_browser(&mut self, url: &str, tx: &mut Tx) {
        if let Err(error) = tx.open_url(url) {
            self.action_failure = Some(ActionFailure { action: "Open pull request", message: format!("{url}: {error}") });
        }
    }

    /// Whether pushing can do anything: a branch without an upstream can always be published.
    pub fn can_push(&self) -> bool {
        self.running.is_none() && self.overview.upstream.is_none_or(|upstream| upstream.ahead > 0)
    }

    /// Whether the branch and its upstream went apart, which only a force push can send.
    pub fn can_force_push(&self) -> bool {
        self.running.is_none() && self.overview.upstream.is_some_and(|upstream| upstream.ahead > 0 && upstream.behind > 0)
    }

    pub fn ask_to_force_push(&mut self) {
        if self.can_force_push() {
            self.confirming = Some(Action::ForcePush);
        }
    }

    pub fn open_palette(&mut self) {
        self.palette_text = TextEdit::default();
        self.palette = Some(0);
        self.focus(|s| &mut s.palette_text);
    }

    pub fn close_palette(&mut self) {
        self.palette = None;
    }

    pub fn open_keys(&mut self) {
        self.palette = None;
        self.showing_keys = true;
    }

    pub fn close_keys(&mut self) {
        self.showing_keys = false;
    }

    /// Every command with a key, whether it can run now or not, in the palette's order.
    pub fn keyed_commands(&self) -> Vec<Command> {
        let mut commands = self.every_command();
        commands.retain(|command| command.shortcut.is_some());
        commands
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

    /// Jumps the diff to the staged file `path` when the diff shows what is staged, and
    /// otherwise leaves nothing marked, as nothing is.
    pub fn scroll_to_staged(&mut self, path: &str, tx: &mut Tx) {
        if self.shows_staged() {
            self.scroll_to_path(path, tx);
        } else {
            self.staged.clear_selection();
        }
    }

    /// Marks the file at the top of the diff in each file list that holds it.
    pub fn follow_visible_file(&mut self, file: usize) {
        let Some(path) = self.diff.files().get(file).map(|diff| diff.path().to_owned()) else { return };
        mark(&mut self.files, &path);
        if self.shows_staged() {
            mark(&mut self.staged, &path);
        }
    }

    /// The files of the shown review that the file list holds.
    pub fn listed_files(&self) -> impl Iterator<Item = &FileDiff> {
        let worktree = self.reviews_worktree();
        let wholly_staged = |path: &str| self.overview.files.get(path).is_some_and(|status| status.staged.is_some() && status.unstaged.is_none());
        self.diff.files().iter().filter(move |file| !(worktree && wholly_staged(file.path())))
    }

    fn finish_action(&mut self, action: &Action, result: Result<()>) {
        self.running = None;
        let creating_pull_request = mem::take(&mut self.creating_pull_request);
        match result {
            Ok(()) => {
                match action {
                    Action::Push if creating_pull_request => self.find_head_parent(),
                    Action::Commit(_) | Action::CommitAll(_) => self.commit_text = TextEdit::default(),
                    // A message written since stays.
                    Action::UndoCommit { message, .. } if self.commit_text.text.trim().is_empty() => self.commit_text = TextEdit::new(message.clone()),
                    Action::Fetch => self.look_for_pull_requests(true),
                    Action::AddRemote { .. } => {
                        self.remote_name_text = TextEdit::default();
                        self.remote_url_text = TextEdit::default();
                    }
                    _ => {}
                }
                if let Some(outcome) = action.outcome() {
                    self.outcome.show(outcome);
                }
            }
            Err(error) => self.action_failure = Some(ActionFailure { action: action.label(), message: error.to_string() }),
        }
        self.refresh();
    }

    /// Everything available right now, by topic.
    fn commands(&self) -> Vec<Command> {
        let mut commands = self.every_command();
        commands.retain(|command| command.available);
        commands
    }

    /// Every command for the repository on screen, by topic, each saying whether it can run
    /// right now. Actions on a file act on the one at the top of the diff.
    fn every_command(&self) -> Vec<Command> {
        let open = Command::new(APPLICATION, "Open repository", |s, _| s.open_opener()).shortcut("Ctrl+O");
        let shortcuts = Command::new(APPLICATION, "Keyboard shortcuts", |s, _| s.open_keys()).shortcut(KEYS);
        if self.repo.is_none() {
            return vec![open, shortcuts];
        }
        let mut commands = Vec::new();
        let mut offer = |available: bool, command: Command| commands.push(Command { available, ..command });
        let idle = self.running.is_none();
        let diff = if self.showing_comments { &self.comment_sheet } else { &self.diff };
        let file = diff.visible_file();
        let files = diff.files().len();

        offer(self.can_open(), open);
        offer(true, shortcuts);
        let to_copy = self.comments_to_copy().len();
        offer(to_copy > 0, Command::new(APPLICATION, copy_label(to_copy), State::copy_open_comments).shortcut(COPY));
        offer(!self.comments.open().is_empty(), Command::new(APPLICATION, "Resolve all comments", |s, _| s.resolve_all_comments()));
        for (group, label) in [(Group::Outdated, "Delete outdated comments"), (Group::Resolved, "Delete resolved comments"), (Group::All, "Delete all comments")] {
            offer(self.comments.count(group) > 0, Command::new(APPLICATION, label, move |s, _| s.ask_to_delete_group(group)));
        }

        offer(self.can_stage_all(), Command::new("Git", "Stage all", |s, _| s.run(Action::StageAll)));
        offer(self.can_unstage_all(), Command::new("Git", "Unstage all", |s, _| s.run(Action::UnstageAll)));
        if let Some(discard) = self.discard_all() {
            let label = if matches!(discard, Action::DiscardChanges(_)) { "Discard all changes" } else { "Discard all unstaged changes" };
            offer(idle, Command::new("Git", label, |s, _| s.ask_to_discard_all()));
        }
        offer(self.can_start_commit(), Command::new("Git", "Commit", |s, _| s.start_commit()).shortcut(COMMIT));
        offer(idle && self.undoable().is_some(), Command::new("Git", "Undo last commit", |s, _| s.undo_commit()));
        offer(self.can_stash(), Command::new("Git", "Stash", |s, _| s.start_stash()).shortcut(STASH));
        if let Some(stash) = self.selected_stash() {
            offer(idle, Command::new("Git", format!("Apply stash {stash}"), |s, _| s.apply_selected_stash()));
            offer(idle, Command::new("Git", format!("Pop stash {stash}"), |s, _| s.pop_selected_stash()));
            offer(idle, Command::new("Git", format!("Drop stash {stash}"), |s, _| s.ask_to_drop_stash()));
        }
        if let Some(branch) = self.branch_to_check_out() {
            offer(idle, Command::new("Git", format!("Check out {branch}"), |s, _| s.check_out_selected()));
            offer(idle, Command::new("Git", format!("Delete branch {branch}"), |s, _| s.ask_to_delete_branch()));
        }
        let merge = if self.overview.operation.is_some() { "Merge or rebase in progress" } else { "Merge or rebase" };
        offer(self.overview.last_commit.is_some(), Command::new("Git", merge, |s, _| s.open_merger()).shortcut(MERGE));
        let shown = self.branch_on_forge().zip(self.pull_request_offer()).map(|(((_, branch), _), offer)| match offer {
            PullRequestOffer::Existing(found) => format!("Open pull request #{}", found.number),
            PullRequestOffer::New { .. } => format!("Create pull request for {branch}"),
        });
        let head = self.head_on_forge().map(|(_, branch)| match self.head_pull_request() {
            Some(found) => format!("Open pull request #{}", found.number),
            None => format!("Create pull request for {branch}"),
        });
        // The current branch's command pushes first, so it stands for both.
        if let Some(label) = shown.filter(|label| head.as_ref() != Some(label)) {
            offer(true, Command::new("Git", label, State::open_pull_request));
        }
        let head_available = head.is_some() && (idle || self.head_pull_request().is_some());
        let head = head.unwrap_or_else(|| "Open or create pull request".to_owned());
        offer(head_available, Command::new("Git", head, State::open_head_pull_request).shortcut(PULL_REQUEST));
        offer(true, Command::new("Git", "Switch branch", |s, _| s.open_switcher()).shortcut("Ctrl+B").alias("Ctrl+Shift+B"));
        offer(self.can_fetch(), Command::new("Git", "Fetch", |s, _| s.run(Action::Fetch)).shortcut(FETCH));
        offer(idle, Command::new("Git", "Pull", |s, _| s.run(Action::Pull)).shortcut(PULL));
        offer(self.can_push(), Command::new("Git", "Push", |s, _| s.run(Action::Push)).shortcut(PUSH));
        offer(self.can_force_push(), Command::new("Git", "Force push", |s, _| s.ask_to_force_push()));
        offer(true, Command::new("Git", "Remotes", |s, _| s.open_remotes()));
        offer(true, Command::new("Git", "Refresh", |s, _| s.refresh_all()));

        for (kind, key) in Kind::ALL.into_iter().zip(VIEWS) {
            offer(self.showing_comments || kind != self.kind, Command::new("View", kind.label(), move |s, _| s.select_kind(kind)).shortcut(key));
        }
        offer(!self.showing_comments, Command::new("View", "Comments", |s, _| s.toggle_comments()).shortcut(COMMENTS));
        let (layout, other) = match self.diff.mode() {
            DiffMode::Split => ("Show unified", DiffMode::Unified),
            _ => ("Show split", DiffMode::Split),
        };
        offer(true, Command::new("View", layout, move |s, _| s.set_layout(other)).shortcut(LAYOUT));

        if !self.showing_comments {
            let name = self.diff.files().get(file).and_then(|diff| diff.path().rsplit('/').next()).unwrap_or_default();
            offer(idle && !self.stageable(file).is_empty(), Command::new("File", format!("Stage {name}"), |s, _| s.stage_file(s.diff.visible_file())));
            offer(idle && !self.unstageable(file).is_empty(), Command::new("File", format!("Unstage {name}"), |s, _| s.unstage_file(s.diff.visible_file())));
            offer(idle && !self.discardable(file).is_empty(), Command::new("File", format!("Discard {name}"), |s, _| s.ask_to_discard(s.diff.visible_file())));
        }

        type Step = fn(&mut DiffState, bool, &mut Tx) -> bool;
        let moves: [(bool, &str, &'static str, Step, bool); 4] = [
            (file + 1 < files, "Next file", "]", DiffState::next_file, true),
            (file > 0, "Previous file", "[", DiffState::next_file, false),
            (files > 0, "Next hunk", "j", DiffState::next_hunk, true),
            (files > 0, "Previous hunk", "k", DiffState::next_hunk, false),
        ];
        for (available, label, key, step, forward) in moves {
            offer(available, Command::new("Navigation", label, move |s, tx| _ = step(s.active_diff(), forward, tx)).shortcut(key));
        }
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
        let pickers = self.switcher.is_some() || self.merger.is_some() || self.shell.opener.is_some();
        let dialogs = self.deleting.is_some() || self.deleting_group.is_some() || self.confirming.is_some() || self.committing || self.stashing.is_some() || self.managing_remotes || self.action_failure.is_some() || self.showing_keys;
        pickers || dialogs
    }

    /// The folder highlighted in the open dialog.
    fn highlighted_choice(&self) -> Option<&Choice> {
        self.shell.opener.and_then(|row| self.shell.offer.choices.get(row))
    }

    /// Starts on the repository on screen, once there is a waker: remembers it, watches it and
    /// loads it.
    fn start(&mut self) {
        let (Some(repo), Some(waker)) = (self.repo.clone(), self.shell.waker.clone()) else { return };
        self.shell.shared.borrow_mut().recents.remember(repo.root());
        self.overview_load.set_waker(waker.clone());
        self.patch_load.set_waker(waker.clone());
        self.action_load.set_waker(waker.clone());
        self.pull_request_load.set_waker(waker.clone());
        self.head_parent_load.set_waker(waker.clone());
        self.comments.set_waker(waker.clone());
        self.watch = Some(RepoWatch::start(repo, waker));
        self.refresh();
    }

    /// Puts what `opened` found in place of the repository on screen, starting afresh.
    fn show_opened(&mut self, opened: Opened) {
        self.shell.shared.borrow_mut().set_root(self.shell.window, opened.root());
        *self = State::new(Some(opened), mem::take(&mut self.shell));
        self.start();
    }

    /// Takes in what the open dialog offers, forgetting the recent repositories that are gone.
    fn show_offer(&mut self, offer: Offer) {
        self.shell.shared.borrow_mut().recents.forget(&offer.gone);
        self.shell.offer = offer;
        self.shell.opener = self.shell.opener.map(|_| 0);
    }

    /// Whether the diff on screen holds every staged file.
    fn shows_staged(&self) -> bool {
        self.shown.as_ref().is_some_and(|shown| shown.review == Review::Changes)
    }

    /// Gives the keyboard to one text field, which takes it from every other.
    fn focus(&mut self, field: fn(&mut State) -> &mut TextEdit) {
        let fields = [&mut self.draft_text, &mut self.edit_text, &mut self.commit_text, &mut self.stash_text, &mut self.branch_text, &mut self.remote_name_text, &mut self.remote_url_text, &mut self.merge_text, &mut self.palette_text, &mut self.shell.open_text];
        for text in fields {
            text.focused = false;
        }
        field(self).focused = true;
    }

    fn show_overview(&mut self, overview: Result<Overview>) {
        match overview {
            Ok(overview) => {
                let switched = overview.head != self.overview.head;
                self.overview = overview;
                self.error = None;
                self.rebuild_list();
                // The branch switched to takes over the review, as long as it is a branch.
                if switched && self.kind == Kind::Branches && self.list.contains(&self.overview.head) {
                    self.list.select(&self.overview.head);
                }
                self.build_file_lists();
                self.load_review();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn show_patch(&mut self, patch: Result<Loaded>, tx: &mut Tx) {
        match patch {
            Ok(Loaded { review, patch, commit }) => {
                self.error = None;
                let unchanged = self.shown.as_ref().filter(|shown| shown.review == review).map(|shown| shown.patch == patch);
                let files = files(&patch);
                // The file lists go by the review on screen.
                self.shown = Some(Shown { review, commit, patch });
                self.look_for_pull_requests(false);
                match unchanged {
                    Some(true) => {}
                    Some(false) => self.update_files(files, tx),
                    None => {
                        self.files.clear();
                        self.show_files(files);
                    }
                }
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
        self.rebuild_sheet();
    }

    /// Fills the comments page with an entry for every comment with a snippet, in listing order,
    /// once the listing changed. An entry keeps what the reader opened or closed while the
    /// comment stays resolved or not; a resolved one starts closed.
    fn rebuild_sheet(&mut self) {
        let listed = self.comments.listed();
        let listing: Vec<(u64, Option<&'static str>)> = listed.iter().map(|comment| (comment.id, comment.status())).collect();
        if listing == self.sheet_listing {
            return;
        }
        let collapsed: HashMap<(u64, bool), bool> = self.sheet.iter().enumerate().map(|(file, entry)| (*entry, self.comment_sheet.collapsed(file))).collect();
        let entries: Vec<(&Comment, FileDiff, Snippet)> = listed.into_iter().filter_map(|comment| {
            let snippet = comment.snippet()?;
            Some((comment, snippet.file()?, snippet))
        }).collect();
        self.sheet = entries.iter().map(|(comment, ..)| (comment.id, comment.resolved)).collect();
        let notes = entries.iter().enumerate().map(|(file, (comment, _, snippet))| Annotation::new(file, LineRef::new(comment.anchor.side, snippet.line), comment.id.to_string())).collect();
        self.comment_sheet.set_files(entries.into_iter().map(|(_, file, _)| file).collect());
        self.comment_sheet.set_annotations(notes);
        for (file, entry) in self.sheet.iter().enumerate() {
            self.comment_sheet.set_collapsed(file, collapsed.get(entry).copied().unwrap_or(entry.1));
        }
        self.sheet_listing = listing;
    }

    /// Puts each open comment, the one opened from the comments page and the draft under their
    /// lines in the diff on screen.
    fn place_annotations(&mut self) {
        let shown_resolved = self.shown_comment.and_then(|id| self.comments.get(id)).filter(|comment| comment.resolved);
        let files = self.diff.files();
        let note = |anchor: &Anchor, id: String| anchor.position(files).map(|(file, at)| Annotation::new(file, at, id));
        let comments = self.comments.open().into_iter().chain(shown_resolved).filter_map(|comment| note(&comment.anchor, comment.id.to_string()));
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

        self.show_files(files);

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

    /// Shows `files` in the diff, in the order of a file list holding them all, and in the file
    /// lists.
    fn show_files(&mut self, mut files: Vec<FileDiff>) {
        let mut order = file_tree("order", files.iter().map(|file| (file.path().to_owned(), (0, 0))));
        order.expand_all();
        files.sort_by_key(|file| order.visible_row(file.path()));
        self.diff.set_files(files);
        self.build_file_lists();
        self.place_annotations();
    }

    /// Fills the file lists from the diff on screen and what is staged, keeping their closed
    /// folders and selection.
    fn build_file_lists(&mut self) {
        let counts = |file: &FileDiff| (file.additions() as u32, file.deletions() as u32);
        let files = file_tree("files", self.listed_files().map(|file| (file.path().to_owned(), counts(file))));
        let staged = self.overview.files.iter().filter(|(_, status)| status.staged.is_some() && self.reviews_worktree());
        let staged = file_tree("staged", staged.map(|(path, _)| (path.clone(), self.overview.staged_lines.get(path).copied().unwrap_or_default())));
        self.files = kept_open(&self.files, files);
        self.staged = kept_open(&self.staged, staged);
        if !self.shows_staged() {
            self.staged.clear_selection();
        }
    }

    /// Fills the list for the current kind, keeping the selection while it still exists and
    /// otherwise selecting the current branch among branches, else the first entry.
    fn rebuild_list(&mut self) {
        let kept = self.list.selected().next().map(str::to_owned);
        let entries = match self.kind {
            Kind::Changes | Kind::Staging => &[][..],
            Kind::Commits => &self.overview.commits,
            Kind::Branches => &self.overview.branches,
            Kind::Stashes => &self.overview.stashes,
        };
        self.list = TreeModel::new().key("list");
        for entry in entries {
            self.list.insert(None, entry.id.clone(), entry.clone());
        }
        let current = entries.iter().find(|entry| self.kind == Kind::Branches && entry.id == self.overview.head);
        let selected = kept.filter(|id| self.list.contains(id)).or_else(|| current.or(entries.first()).map(|entry| entry.id.clone()));
        if let Some(id) = selected {
            self.list.select(&id);
        }
    }
}

fn file_tree(key: &str, files: impl Iterator<Item = (String, LineCounts)>) -> TreeModel<FileNode<LineCounts>> {
    FileTree::new().key(key).compact_folders(true).build(files)
}

/// `new`, open but where `old` had folders closed, and with `old`'s selection where it can.
fn kept_open<T>(old: &TreeModel<T>, mut new: TreeModel<T>) -> TreeModel<T> {
    new.expand_all();
    let rows = (0..old.visible_len()).filter_map(|row| old.visible_id(row));
    for folder in rows.filter(|id| old.has_children(id) && !old.is_expanded(id)) {
        new.set_expanded(folder, false);
    }
    if let Some(id) = old.selected().next() {
        new.select(id);
    }
    new
}

/// Selects `id` alone, or nothing when `tree` doesn't hold it.
fn mark<T>(tree: &mut TreeModel<T>, id: &str) {
    if tree.contains(id) {
        tree.select(id);
    } else {
        tree.clear_selection();
    }
}

/// What copying `count` open comments says it does.
pub fn copy_label(count: usize) -> String {
    match count {
        0 => "Copy open comments".to_owned(),
        1 => "Copy 1 open comment".to_owned(),
        n => format!("Copy {n} open comments"),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::git::testing::Fixture;
    use crate::overview::{Change, Upstream};

    const TWO_FILES: &str = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-a\n+A\ndiff --git a/c.txt b/c.txt\n--- a/c.txt\n+++ b/c.txt\n@@ -1 +1 @@\n-c\n+C\n";
    const THREE_FILES: &str = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-a\n+A\ndiff --git a/b.txt b/b.txt\n--- a/b.txt\n+++ b/b.txt\n@@ -1 +1 @@\n-b\n+B\ndiff --git a/c.txt b/c.txt\n--- a/c.txt\n+++ b/c.txt\n@@ -1 +1,2 @@\n-c\n+C\n+D\n";

    fn state() -> (Fixture, State) {
        let fixture = Fixture::new();
        let state = State::new(Some(Opened::Repository(fixture.repo.clone(), Comments::load(&fixture.repo))), Shell::default());
        (fixture, state)
    }

    fn land(state: &mut State, review: Review, patch: &str) {
        state.show_patch(Ok(Loaded { review, patch: patch.to_owned(), commit: None }), &mut Tx::default());
    }

    fn file_paths(state: &State) -> Vec<&str> {
        state.diff.files().iter().map(|file| file.path()).collect()
    }

    fn status(staged: bool, unstaged: bool) -> FileStatus {
        FileStatus { staged: staged.then_some(Change::Changed), unstaged: unstaged.then_some(Change::Changed) }
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

    /// The leaf ids of `tree`, in order.
    fn listed(tree: &TreeModel<FileNode<LineCounts>>) -> Vec<&str> {
        (0..tree.visible_len()).filter_map(|row| tree.visible_id(row)).filter(|id| !tree.has_children(id)).collect()
    }

    #[test]
    fn worktree_reviews_list_staged_files_apart_and_partly_staged_ones_in_both() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, false)), ("b.txt".to_owned(), status(true, true)), ("c.txt".to_owned(), status(false, true))]);
        state.overview.staged_lines = HashMap::from([("a.txt".to_owned(), (1, 1))]);

        land(&mut state, Review::Changes, THREE_FILES);
        assert_eq!((listed(&state.staged), listed(&state.files)), (vec!["a.txt", "b.txt"], vec!["b.txt", "c.txt"]));
        assert_eq!(state.staged.get("a.txt").and_then(|node| node.data), Some((1, 1)));

        land(&mut state, Review::Commit("abc".into()), THREE_FILES);
        assert_eq!((listed(&state.staged), listed(&state.files)), (vec![], vec!["a.txt", "b.txt", "c.txt"]));
    }

    #[test]
    fn a_folder_in_a_file_list_stands_for_every_file_under_it() {
        let (_fixture, mut state) = state();
        let added = |path: &str| format!("diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+x\n");
        let patch: String = ["src/a.txt", "src/deep/b.txt", "c.txt"].map(added).concat();
        state.overview.files = HashMap::from([
            ("src/a.txt".to_owned(), status(true, true)),
            ("src/deep/b.txt".to_owned(), status(false, true)),
            ("src/staged.txt".to_owned(), status(true, false)),
            ("c.txt".to_owned(), status(false, true)),
        ]);

        land(&mut state, Review::Changes, &patch);

        assert_eq!(state.stageable_in("src"), ["src/deep/b.txt", "src/a.txt"]);
        assert_eq!(state.stageable_in("c.txt"), ["c.txt"]);
        let mut staged = state.unstageable_in("src");
        staged.sort();
        assert_eq!(staged, ["src/a.txt", "src/staged.txt"]);
    }

    #[test]
    fn only_a_diff_holding_what_is_staged_marks_it_as_it_scrolls() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, false)), ("c.txt".to_owned(), status(true, true))]);

        land(&mut state, Review::Changes, TWO_FILES);
        state.follow_visible_file(0);
        assert_eq!((state.staged.selected().next(), state.files.selected().next()), (Some("a.txt"), None));
        state.follow_visible_file(1);
        assert_eq!((state.staged.selected().next(), state.files.selected().next()), (Some("c.txt"), Some("c.txt")));

        land(&mut state, Review::Unstaged, TWO_FILES);
        assert_eq!(state.staged.selected().next(), None);
        state.follow_visible_file(1);
        assert_eq!(state.staged.selected().next(), None);
    }

    #[test]
    fn a_stash_of_what_is_staged_needs_something_staged() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(false, true))]);
        state.start_stash();
        assert!(state.stash_ready());

        state.stashing = Some(Stashed::Staged);
        assert!(!state.stash_ready());
        state.stash();
        assert_eq!((state.stashing, state.running), (Some(Stashed::Staged), None));
    }

    #[test]
    fn a_failed_commit_keeps_its_message_and_a_good_one_clears_it() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, false))]);
        state.start_commit();
        state.commit_text = TextEdit::new("Fix it".to_owned());
        state.cancel_commit();
        state.start_commit();
        assert_eq!(state.commit_text.text, "Fix it");

        state.commit();
        assert_eq!((state.committing, state.running), (false, Some("Commit")));
        let action = Action::Commit("Fix it".into());
        state.finish_action(&action, Err(crate::git::GitError::new("hook said no")));
        assert_eq!(state.commit_text.text, "Fix it");
        assert_eq!(state.action_failure.as_ref().map(|failure| failure.message.as_str()), Some("hook said no"));

        state.finish_action(&action, Ok(()));
        assert_eq!(state.commit_text.text, "");

        state.commit_text = TextEdit::new("Take it all".to_owned());
        state.finish_action(&Action::CommitAll("Take it all".into()), Ok(()));
        assert_eq!(state.commit_text.text, "");
    }

    #[test]
    fn discarding_everything_means_what_the_view_shows_and_counts_the_new_files_it_deletes() {
        let (_fixture, mut state) = state();
        let new = |staged| FileStatus { staged, unstaged: staged.is_none().then_some(Change::New) };
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, false)), ("b.txt".to_owned(), new(Some(Change::New))), ("c.txt".to_owned(), new(None))]);

        state.select_kind(Kind::Changes);
        state.ask_to_discard_all();
        let everything = state.confirming.clone().unwrap();
        assert_eq!(everything, Action::DiscardChanges(vec!["a.txt".into(), "b.txt".into(), "c.txt".into()]));
        assert_eq!(state.deleted_by(&everything), 2);

        state.select_kind(Kind::Staging);
        state.ask_to_discard_all();
        let unstaged = state.confirming.clone().unwrap();
        assert_eq!(unstaged, Action::DiscardUnstaged(vec!["c.txt".into()]));
        assert_eq!(state.deleted_by(&unstaged), 1);

        state.toggle_comments();
        state.cancel_action();
        state.ask_to_discard_all();
        assert_eq!(state.confirming, None, "the comments page shows no changes");
    }

    #[test]
    fn undoing_a_commit_asks_only_once_pushed_and_brings_its_message_back_unless_another_was_written() {
        let (_fixture, mut state) = state();
        let last = |parents| Some(LastCommit { hash: "abc".into(), parents, message: "Fix it".into() });
        let undo = Action::UndoCommit { commit: "abc".into(), message: "Fix it".into() };
        state.overview.last_commit = last(2);
        assert!(!state.commands().iter().any(|command| command.label == "Undo last commit"), "a merge commit");

        state.overview.last_commit = last(1);
        state.undo_commit();
        assert_eq!((state.confirming.as_ref(), state.running), (None, Some("Undo commit")));
        state.finish_action(&undo, Ok(()));
        assert_eq!(state.commit_text.text, "Fix it");

        state.commit_text = TextEdit::new("Newer".to_owned());
        state.finish_action(&undo, Ok(()));
        assert_eq!(state.commit_text.text, "Newer");

        state.overview.upstream = Some(Upstream { ahead: 0, behind: 0 });
        state.undo_commit();
        assert_eq!(state.confirming, Some(undo));
    }

    #[test]
    fn a_stopped_merge_or_a_conflict_holds_up_committing_and_only_settled_conflicts_continue() {
        let (_fixture, mut state) = state();
        state.overview.files = HashMap::from([("a.txt".to_owned(), FileStatus { staged: None, unstaged: Some(Change::Conflicted) })]);
        assert!(!state.can_start_commit(), "committing everything would stage the conflict");

        state.overview.operation = Some(Operation::Merge { branch: "feature".into() });
        assert!(!state.can_continue());
        state.overview.files = HashMap::from([("a.txt".to_owned(), status(true, false))]);
        assert!(!state.can_start_commit() && state.can_continue());

        state.open_merger();
        state.continue_operation();
        assert_eq!((state.merger, state.running), (None, Some("Continue merge")));
    }

    #[test]
    fn the_merge_dialog_offers_every_branch_but_the_current_one_and_brings_in_the_highlighted() {
        let (_fixture, mut state) = state();
        state.overview.head = "main".into();
        state.overview.last_commit = Some(LastCommit { hash: "abc".into(), parents: 1, message: String::new() });
        state.overview.branches = ["main", "feature"].map(|name| Entry { id: name.to_owned(), title: name.to_owned(), detail: String::new() }).to_vec();
        state.overview.remote_branches = vec![crate::overview::RemoteBranch { remote: "origin".into(), name: "fix".into(), age: String::new() }];
        let ctrl_shift_m = KeyEvent { key: Key::Command('m'), mods: rune::Mods { ctrl: true, cmd: true, shift: true, ..rune::Mods::default() } };

        state.on_key(&ctrl_shift_m, &mut Tx::default());
        assert_eq!((state.merger, state.merge_choice(0), state.merge_choice(1)), (Some(0), Some("feature".into()), Some("origin/fix".into())));

        state.move_merger(5);
        state.integration = Integration::Rebase;
        state.integrate(state.merger.unwrap());
        assert_eq!((state.merger, state.running), (None, Some("Rebase")));
    }

    #[test]
    fn a_finished_action_says_what_it_did_but_staging_says_nothing() {
        let (_fixture, mut state) = state();
        state.finish_action(&Action::Stage(vec!["a.txt".into()]), Ok(()));
        assert_eq!(state.outcome(), None);

        state.finish_action(&Action::Pull, Err(crate::git::GitError::new("no remote")));
        assert_eq!(state.outcome(), None);

        state.finish_action(&Action::Checkout("feature".into()), Ok(()));
        assert_eq!(state.outcome(), Some("Switched to feature"));
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
        state.palette_text = TextEdit::new("BRANCH sw".to_owned());
        assert_eq!(state.palette_commands().iter().map(|command| command.label.as_str()).collect::<Vec<_>>(), ["Switch branch"]);

        state.move_palette(5);
        assert_eq!(state.palette, Some(0));
        state.move_palette(-1);
        assert_eq!(state.palette, Some(0));

        state.run_command(0, &mut Tx::default());
        assert_eq!((state.palette, state.switcher), (None, Some(0)));
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
    fn git_keys_run_only_what_the_palette_offers() {
        let (_fixture, mut state) = state();
        let ctrl = |key, shift| KeyEvent { key, mods: rune::Mods { ctrl: true, cmd: true, shift, ..rune::Mods::default() } };

        state.on_key(&ctrl(Key::Enter, false), &mut Tx::default());
        assert!(!state.committing, "nothing to commit");

        state.overview.files.insert("a.txt".into(), status(false, true));
        state.open_remotes();
        state.on_key(&ctrl(Key::Enter, false), &mut Tx::default());
        assert!(!state.committing, "a dialog is open");

        state.close_remotes();
        state.on_key(&ctrl(Key::Enter, false), &mut Tx::default());
        assert!(state.committing);
    }

    #[test]
    fn ctrl_enter_goes_ahead_with_a_confirm_dialog_rather_than_committing() {
        let (_fixture, mut state) = state();
        let ctrl_enter = KeyEvent { key: Key::Enter, mods: rune::Mods { ctrl: true, cmd: true, ..rune::Mods::default() } };
        state.overview.files.insert("a.txt".into(), status(false, true));
        state.ask_to_delete_group(Group::All);

        state.on_key(&ctrl_enter, &mut Tx::default());
        assert!(state.deleting_group.is_none() && !state.committing);
    }

    #[test]
    fn the_keys_list_every_key_even_those_that_do_nothing_now() {
        let (_fixture, mut state) = state();
        state.overview.upstream = Some(Upstream { ahead: 0, behind: 0 });
        let keys = state.keyed_commands().into_iter().map(|command| command.label).collect::<Vec<_>>();
        assert!(keys.contains(&"Push".to_owned()) && keys.contains(&"Next file".to_owned()), "{keys:?}");
        assert!(!state.commands().iter().any(|command| command.label == "Push"));

        let ctrl = |key, shift| KeyEvent { key, mods: rune::Mods { ctrl: true, cmd: true, shift, ..rune::Mods::default() } };
        state.overview.files.insert("a.txt".into(), status(false, true));
        state.on_key(&ctrl(Key::Command('h'), false), &mut Tx::default());
        state.on_key(&ctrl(Key::Enter, false), &mut Tx::default());
        assert!(state.showing_keys && !state.committing, "the keys only list");

        state.on_key(&ctrl(Key::Command('h'), false), &mut Tx::default());
        assert!(!state.showing_keys);
        state.on_key(&ctrl(Key::Command('2'), false), &mut Tx::default());
        assert_eq!(state.kind, Kind::Changes);
        state.on_key(&ctrl(Key::Command('0'), false), &mut Tx::default());
        assert!(state.showing_comments);
        state.on_key(&ctrl(Key::Command('2'), false), &mut Tx::default());
        assert!(!state.showing_comments, "a view's key leaves the comments page");
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
    fn the_palette_switches_to_any_other_view_and_from_the_comments_page_to_any() {
        let (_fixture, mut state) = state();
        let views = |state: &mut State| {
            state.open_palette();
            state.palette_text = TextEdit::new("view".to_owned());
            state.palette_commands().into_iter().map(|command| command.label).collect::<Vec<_>>()
        };
        assert_eq!(views(&mut state), ["Changes", "Branches", "Commits", "Stashes", "Comments", "Show unified"]);

        state.toggle_comments();
        assert_eq!(views(&mut state), ["Staging", "Changes", "Branches", "Commits", "Stashes", "Show unified"]);
        state.run_command(3, &mut Tx::default());
        assert_eq!((state.kind, state.showing_comments), (Kind::Commits, false));
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
    fn opening_another_repository_starts_afresh_but_for_the_settings() {
        let (_fixture, mut state) = state();
        land(&mut state, Review::Changes, TWO_FILES);
        state.select_kind(Kind::Commits);
        state.toggle_comments();
        state.set_layout(DiffMode::Unified);
        state.shell.open_text = TextEdit::new("~/elsewhere".to_owned());
        let other = Fixture::new();

        state.show_opened(Opened::Repository(other.repo.clone(), Comments::load(&other.repo)));

        assert_eq!(state.repo.as_ref().map(Repo::root), Some(other.repo.root()));
        assert_eq!((state.kind, state.showing_comments, state.diff.files().len()), (Kind::Commits, false, 0));
        assert_eq!((state.diff.mode(), state.comment_sheet.mode()), (DiffMode::Unified, DiffMode::Unified));
        assert_eq!(state.shell.open_text.text, "~/elsewhere");
    }

    fn on_line(line: u32) -> Anchor {
        Anchor { path: "a.txt".into(), side: rune_kit::diff::Side::New, lines: Some(crate::comments::Lines { start: line, end: line, quoted: vec!["A".into()] }) }
    }

    #[test]
    fn a_new_comment_keeps_the_diff_it_was_written_on() {
        let (_fixture, mut state) = state();
        land(&mut state, Review::Changes, TWO_FILES);
        state.compose_on_lines(LineRange::line(1, rune_kit::diff::LineRef::new(rune_kit::diff::Side::New, 1)));
        state.draft_text = TextEdit::new("Why?".to_owned());

        state.submit_draft();

        let snippet = state.comments.open()[0].snippet.clone().unwrap();
        assert!(snippet.patch.contains("+C") && !snippet.patch.contains("+A"), "{}", snippet.patch);
        assert_eq!(state.comment_sheet.files()[0].path(), "c.txt");
    }

    #[test]
    fn the_comments_page_lists_resolved_comments_last_and_closed_and_keeps_what_the_reader_opened() {
        let (_fixture, mut state) = state();
        for body in ["one", "two", "three"] {
            state.comments.add(on_line(1), Review::Changes, None, None, body.into());
        }
        state.resolve_comment(1);
        let ids = |state: &State| state.sheet.iter().map(|(id, _)| *id).collect::<Vec<_>>();
        let closed = |state: &State| (0..state.sheet.len()).map(|file| state.comment_sheet.collapsed(file)).collect::<Vec<_>>();
        assert_eq!((ids(&state), closed(&state)), (vec![2, 3, 1], vec![false, false, true]));

        state.comment_sheet.set_collapsed(0, true);
        state.comment_sheet.set_collapsed(2, false);
        state.comments.add(on_line(1), Review::Changes, None, None, "four".into());
        state.comments_changed();
        assert_eq!((ids(&state), closed(&state)), (vec![2, 3, 4, 1], vec![true, false, false, false]));

        state.reopen_comment(1);
        state.resolve_comment(1);
        assert!(closed(&state)[3], "resolving again closes it again");
    }

    #[test]
    fn show_in_diff_opens_the_comments_review_while_it_is_on_offer() {
        let (_fixture, mut state) = state();
        state.overview.commits = vec![Entry { id: "abc".into(), title: "Fix".into(), detail: String::new() }];
        state.comments.add(on_line(1), Review::Commit("abc".into()), None, None, "Here".into());
        state.comments.add(on_line(1), Review::Commit("gone".into()), None, None, "Lost".into());
        state.toggle_comments();

        state.show_in_diff(2);
        assert!(state.showing_comments);

        state.show_in_diff(1);
        assert_eq!((state.kind, state.showing_comments, state.list.selected().next()), (Kind::Commits, false, Some("abc")));
    }

    #[test]
    fn a_window_without_a_repository_offers_only_to_open_one_and_its_keys() {
        let state = State::new(Some(Opened::Folder(PathBuf::from("/somewhere"))), Shell::default());

        assert_eq!(state.commands().into_iter().map(|command| command.title()).collect::<Vec<_>>(), ["Application: Open repository", "Application: Keyboard shortcuts"]);
        assert_eq!(state.not_repository, Some(PathBuf::from("/somewhere")));
    }

    #[test]
    fn the_switcher_closes_on_the_current_branch_and_waits_out_a_running_action() {
        let (_fixture, mut state) = state();
        state.overview.head = "main".to_owned();
        state.overview.branches = vec![Entry { id: "main".to_owned(), title: "main".to_owned(), detail: String::new() }];
        state.overview.remote_branches = vec![crate::overview::RemoteBranch { remote: "origin".into(), name: "feature".into(), age: String::new() }];

        state.on_key(&KeyEvent { key: Key::Command('b'), mods: rune::Mods { ctrl: true, cmd: true, shift: true, ..rune::Mods::default() } }, &mut Tx::default());
        state.pick_branch(0);
        assert_eq!((state.switcher, state.running), (None, None));

        state.open_switcher();
        state.running = Some("Fetch");
        state.pick_branch(1);
        assert_eq!(state.switcher, Some(0));

        state.running = None;
        state.pick_branch(1);
        assert_eq!((state.switcher, state.running), (None, Some("Check out")));
    }

    #[test]
    fn the_branches_review_follows_a_switch_of_branch() {
        let (_fixture, mut state) = state();
        let branches = ["main", "feature", "other"].map(|name| Entry { id: name.to_owned(), title: name.to_owned(), detail: String::new() }).to_vec();
        let on = |head: &str| Overview { head: head.to_owned(), branches: branches.clone(), ..Overview::default() };
        state.show_overview(Ok(on("main")));
        state.select_kind(Kind::Branches);
        state.list.select("feature");

        state.show_overview(Ok(on("main")));
        assert_eq!(state.list.selected().next(), Some("feature"), "no switch, so the pick stays");

        state.show_overview(Ok(on("other")));
        assert_eq!(state.list.selected().next(), Some("other"));

        state.show_overview(Ok(on("abc1234")));
        assert_eq!(state.list.selected().next(), Some("other"), "a detached head is no branch to show");
    }

    #[test]
    fn a_remote_is_named_origin_unless_one_is() {
        let (_fixture, mut state) = state();
        state.remote_url_text = TextEdit::new("git@host.example:me/thing.git".to_owned());
        assert_eq!(state.remote_to_add(), Some(Action::AddRemote { name: "origin".into(), url: "git@host.example:me/thing.git".into() }));

        state.overview.remotes = vec![crate::overview::Remote { name: "origin".into(), url: "elsewhere".into() }];
        assert_eq!(state.remote_to_add(), None);
        state.remote_name_text = TextEdit::new(" fork ".to_owned());
        assert_eq!(state.remote_to_add(), Some(Action::AddRemote { name: "fork".into(), url: "git@host.example:me/thing.git".into() }));
    }

    #[test]
    fn a_branch_offers_its_pull_request_once_found_or_the_forges_form() {
        let (_fixture, mut state) = state();
        state.overview.head = "main".to_owned();
        state.select_kind(Kind::Branches);
        let offered = |state: &State| match state.pull_request_offer() {
            Some(PullRequestOffer::Existing(found)) => Some(found.url),
            Some(PullRequestOffer::New { url, .. }) => Some(url),
            None => None,
        };
        land(&mut state, Review::Branch { base: "main".into(), branch: "main".into() }, "");
        state.overview.hosted = Hosted::parse("git@codeberg.org:org/thing.git");
        assert_eq!(offered(&state), None, "the default branch compared with itself");

        // Landed before the remote is on GitHub, so nothing runs gh.
        land(&mut state, Review::Branch { base: "main".into(), branch: "feature".into() }, "");
        assert_eq!(offered(&state).as_deref(), Some("https://codeberg.org/org/thing/compare/main...feature"));

        let github = Hosted::parse("git@github.com:org/thing.git").unwrap();
        state.overview.hosted = Some(github.clone());
        assert_eq!(offered(&state), None, "GitHub is asked first");
        let found = PullRequest { number: 7, title: "Feature".into(), status: gh::Status::Open, url: "https://github.com/org/thing/pull/7".into() };
        state.pull_requests.insert((github.clone(), "other".into()), Ok(Some(found.clone())));
        assert_eq!(offered(&state), None, "an answer about another branch");
        state.pull_requests.insert((github.clone(), "feature".into()), Ok(Some(found.clone())));
        assert_eq!(offered(&state).as_deref(), Some("https://github.com/org/thing/pull/7"));
        state.pull_requests.insert((github, "feature".into()), Err("gh isn't installed".into()));
        assert_eq!(offered(&state).as_deref(), Some("https://github.com/org/thing/compare/main...feature"));
    }

    #[test]
    fn the_current_branch_shows_its_pull_request_unless_it_is_the_default_branch() {
        let (_fixture, mut state) = state();
        let github = Hosted::parse("git@github.com:org/thing.git").unwrap();
        state.overview = Overview {
            head: "feature".into(),
            base: "main".into(),
            branches: ["main", "feature"].map(|name| Entry { id: name.to_owned(), title: name.to_owned(), detail: String::new() }).to_vec(),
            hosted: Some(github.clone()),
            ..Overview::default()
        };
        let found = PullRequest { number: 7, title: "Feature".into(), status: gh::Status::Open, url: "https://github.com/org/thing/pull/7".into() };
        state.pull_requests.insert((github.clone(), "feature".into()), Ok(Some(found)));
        state.pull_requests.insert((github, "main".into()), Ok(None));
        assert_eq!(state.head_pull_request().map(|found| found.number), Some(7));

        state.overview.head = "main".into();
        assert_eq!(state.head_pull_request(), None);
    }

    #[test]
    fn the_current_branch_is_picked_first_and_any_other_can_be_checked_out() {
        let (_fixture, mut state) = state();
        state.overview.head = "main".to_owned();
        state.overview.branches = ["feature", "main"].map(|name| Entry { id: name.to_owned(), title: name.to_owned(), detail: String::new() }).to_vec();
        state.select_kind(Kind::Branches);
        assert_eq!(state.branch_to_check_out(), None);

        state.list.select("feature");
        assert_eq!(state.branch_to_check_out().as_deref(), Some("feature"));
    }
}
