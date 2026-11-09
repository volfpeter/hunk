//! The window: a bar to pick what to review, the lists behind that choice, and the diff.

use rune::{Color, Cx, El, Icon, ScrollSpec, TextEdit, TextStyle, Tx, col, icon, row, text};
use rune::{Key, KeyEvent, Mods};
use rune_kit::diff::{Annotation, DiffMode, DiffView};
use rune_kit::{Badge, Button, ButtonVariant, ControlSize, Dialog, FileNode, IconButton, IconButtonVariant, Kit, RadioGroup, RadioItem, SegmentedControl, Spinner, TextArea, TextInput, Toast, ToastRegion, Tree, TreeItem, TreeModel, TreeStyle, root};

use crate::action::{Action, Stashed};
use crate::app::{COMMIT, Command, DRAFT_NOTE, Kind, PULL, PUSH, State, copy_label};
use crate::comments::Comment;
use crate::open::{Choice, tilde};
use crate::overview::{CommitPlan, Entry, LineCounts};
use crate::review::Review;
use crate::switcher::BranchChoice;

const BAR_HEIGHT: f32 = 48.0;
/// The page showing between the sheets, and around them.
const SHEET_GAP: f32 = 8.0;
const PANEL_HEIGHT: f32 = 200.0;
const DRAFT_ROWS: (usize, usize) = (3, 12);
const COMMIT_ROWS: usize = 4;
const CONFIRM_WIDTH: f32 = 400.0;
const UNDONE: &str = "This can't be undone.";
const COMMIT_WIDTH: f32 = 800.0;
const COMMIT_SUMMARY_WIDTH: f32 = 180.0;
const FAILURE_WIDTH: f32 = 640.0;
/// The most height git's output takes in a dialog before it scrolls.
const FAILURE_MAX_HEIGHT: f32 = 320.0;
/// Dialogs whose rows are picked with the keyboard: the palette, the open dialog and the branch
/// switcher.
const PICKER_WIDTH: f32 = 560.0;
const PICKER_ROW_HEIGHT: f32 = 32.0;
const PICKER_MARKER_WIDTH: f32 = 12.0;
/// The most width a list entry's detail takes from its title.
const DETAIL_MAX_WIDTH: f32 = 240.0;
/// The most width where a folder is takes from its name.
const PLACE_MAX_WIDTH: f32 = 360.0;
/// The most width the repository's name and its branch each take from the bar.
const HEAD_MAX_WIDTH: f32 = 160.0;
const REMOTE_NAME_WIDTH: f32 = 140.0;

/// Two points compared, neither leading to the other.
static COMPARED: Icon = Icon {
    name: "diffactive-compared",
    view: 16.0,
    body: r#"<path d="M2.5 5H13M10.5 2.5L13 5L10.5 7.5M13.5 11H3M5.5 8.5L3 11L5.5 13.5" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round"/>"#,
};

/// Committing: a commit's dot on its line.
static COMMIT_ICON: Icon = Icon {
    name: "diffactive-commit",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="2.75" stroke="currentColor" stroke-width="1.5" fill="none"/><path d="M1.5 8H5.25M10.75 8H14.5" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"#,
};

/// Pulling: down onto a base.
static PULL_ICON: Icon = Icon {
    name: "diffactive-pull",
    view: 16.0,
    body: r#"<path d="M8 2V10.5M4.5 7L8 10.5L11.5 7M3 13.75H13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// Pushing: up off a base.
static PUSH_ICON: Icon = Icon {
    name: "diffactive-push",
    view: 16.0,
    body: r#"<path d="M8 13.75V5.25M4.5 8.75L8 5.25L11.5 8.75M3 2H13" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" fill="none"/>"#,
};

/// Comments: a speech bubble.
static COMMENTS_ICON: Icon = Icon {
    name: "diffactive-comments",
    view: 16.0,
    body: r#"<path d="M2.5 3.5C2.5 2.95 2.95 2.5 3.5 2.5H12.5C13.05 2.5 13.5 2.95 13.5 3.5V10C13.5 10.55 13.05 11 12.5 11H7L4.5 13.5V11H3.5C2.95 11 2.5 10.55 2.5 10V3.5Z" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round" fill="none"/>"#,
};

/// Where a comment card sits, which decides what it repeats of its surroundings.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CardPlace {
    /// Under its line in a diff.
    Line,
    /// Above a diff that has nowhere to show it, so it quotes its code.
    AboveDiff,
    /// Under its snippet on the comments page, whose header says where it is.
    Snippet,
}

/// Which text a comment field writes to and what its buttons do.
#[derive(Clone, Copy)]
struct Writing {
    text: for<'a> fn(&'a mut State) -> &'a mut TextEdit,
    save_label: &'static str,
    save: fn(&mut State),
    cancel: fn(&mut State),
}

const NEW_COMMENT: Writing = Writing { text: |s| &mut s.draft_text, save_label: "Comment", save: State::submit_draft, cancel: State::cancel_draft };
const EDITED_COMMENT: Writing = Writing { text: |s| &mut s.edit_text, save_label: "Save", save: State::submit_edit, cancel: State::cancel_edit };

/// Text on one line that gives way to what sits beside it, whole in a tooltip when cut.
fn one_line(line: String) -> El {
    text(line.clone()).truncate().shrink().min_w(0.0).help_when_truncated(line)
}

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let k = Kit::neutral(cx.system_dark());
    let mut overlays = Vec::new();
    // Under the confirm for removing a remote.
    remotes_dialog(s, &k, &mut overlays);
    delete_dialog(s, &k, &mut overlays);
    action_dialog(s, &k, &mut overlays);
    commit_dialog(s, &k, &mut overlays);
    stash_dialog(s, &k, &mut overlays);
    failure_dialog(s, &k, &mut overlays);
    palette_dialog(s, &k, &mut overlays);
    open_dialog(s, &k, &mut overlays);
    branch_dialog(s, &k, &mut overlays);
    outcome_toast(s, &k, &mut overlays);
    if !s.has_repository() {
        return root(col().fill(k.c.page).padding(SHEET_GAP).child(welcome(s, &k)), overlays);
    }
    let sheets = col().w_full().grow(1.0).min_h(0.0).px(SHEET_GAP).pb(SHEET_GAP).gap(SHEET_GAP);
    let sheets = if s.showing_comments { sheets.child(comments_page(s, &k)) } else { sheets.child(panel(s, &k)).child(main(s, &k)) };
    root(col().fill(k.c.page).child(bar(s, &k)).child(sheets), overlays)
}

/// The way to a repository, without one on screen, and the way to make the folder opened one.
fn welcome(s: &State, k: &Kit) -> El {
    let open = Button::new("Open a repository").shortcut("Ctrl+O").on_press(State::open_opener);
    let content = match &s.not_repository {
        Some(folder) => {
            let buttons = row().gap(8.0).child(Button::new("Initialise repository").on_press(State::init_folder).build(k)).child(open.variant(ButtonVariant::Ghost).build(k));
            let said = text(format!("{} isn't in a Git repository.", tilde(folder))).font(k.ty.ui_medium).color(k.c.text_base).wrap_anywhere();
            col().w_full().items_center().gap(16.0).child(said).child(buttons)
        }
        None => col().items_center().gap(8.0).child(open.build(k)).child(text("Ctrl+O").font(k.ty.ui).color(k.c.text_faint)),
    };
    sheet(k, col().key("welcome").w_full().grow(1.0).center().padding(32.0).child(content))
}

/// `content` on a sheet of its own, apart from the page and the other sheets.
fn sheet(k: &Kit, content: El) -> El {
    content.fill(k.c.bg_base).border(1.0, k.c.border_base).radius(k.radius.lg).clip_rounded(k.radius.lg)
}

fn bar(s: &State, k: &Kit) -> El {
    let selected = Kind::ALL.iter().position(|kind| *kind == s.kind).filter(|_| !s.showing_comments);
    let kinds = SegmentedControl::new(&Kind::ALL.map(Kind::label), selected).fit().on_pick(|s: &mut State, i, _: &mut Tx| s.select_kind(Kind::ALL[i]));

    let open = s.comments.open().len();
    let comments = Button::new(format!("Comments, {open} open"))
        .icon(&COMMENTS_ICON)
        .variant(ButtonVariant::Ghost)
        .expanded(s.showing_comments)
        .help(if s.showing_comments { "Back to the review" } else { "Every comment, with the code it was written on" })
        .label_with(move |font, ink| text(if open > 0 { format!("Comments {open}") } else { "Comments".to_owned() }).font(font).color(ink))
        .on_press(State::toggle_comments);
    let commit = Button::new("Commit")
        .icon(&COMMIT_ICON)
        .variant(ButtonVariant::Ghost)
        .help("Commit what is staged, or everything when nothing is")
        .shortcut(COMMIT)
        .disabled(!s.can_start_commit())
        .on_press(State::start_commit);
    let behind = s.overview.upstream.map_or(0, |upstream| upstream.behind);
    let ahead = s.overview.upstream.map_or(0, |upstream| upstream.ahead);
    let pull = sync_button(k, &PULL_ICON, "Pull", behind, PULL, s.running.is_some(), |s| s.run(Action::Pull));
    let push = sync_button(k, &PUSH_ICON, "Push", ahead, PUSH, !s.can_push(), |s| s.run(Action::Push));

    let repository = head_button(s.repo_name(), "Open another repository", "Ctrl+O").disabled(!s.can_open()).on_press(State::open_opener);
    let branch = head_button(s.overview.head.clone(), "Switch branch", "Ctrl+B").variant(ButtonVariant::GhostMuted).on_press(State::open_switcher);

    row()
        .key("bar")
        .w_full()
        .h(BAR_HEIGHT)
        .px(16.0)
        .gap(12.0)
        .items_center()
        .child(repository.build(k).key("repository").max_w(HEAD_MAX_WIDTH).min_w(0.0))
        .child(branch.build(k).key("branch").max_w(HEAD_MAX_WIDTH).min_w(0.0))
        .child(kinds.build(k))
        .child(row().grow(1.0))
        .children(s.running.map(|action| progress(k, action)))
        .child(comments.build(k).key("comments"))
        .child(commit.build(k).key("commit"))
        .child(pull.key("pull"))
        .child(push.key("push"))
}

/// A button of the bar that moves commits, showing how many there are to move, if any.
fn sync_button(k: &Kit, glyph: &'static Icon, label: &str, commits: u32, shortcut: &str, disabled: bool, act: fn(&mut State)) -> El {
    if commits == 0 {
        return IconButton::new(glyph).variant(IconButtonVariant::Ghost).label(label).shortcut(shortcut).disabled(disabled).on_press(act).build(k);
    }
    let button = Button::new(format!("{label}, {commits} commits")).icon(glyph).variant(ButtonVariant::Ghost).help(label).shortcut(shortcut);
    button.label_with(move |font, ink| text(commits.to_string()).font(font).color(ink)).disabled(disabled).on_press(act).build(k)
}

/// A button in the bar naming what is on screen, cut to fit.
fn head_button(name: String, help: &str, shortcut: &str) -> Button {
    Button::new(name.clone()).variant(ButtonVariant::Ghost).label_with(move |font, ink| one_line(name).font(font).color(ink)).help(help).shortcut(shortcut)
}

/// Says that `action` is running.
fn progress(k: &Kit, action: &str) -> El {
    row().gap(8.0).items_center().child(Spinner::new().size(14.0).build(k)).child(text(format!("{action}…")).font(k.ty.ui).color(k.c.text_muted))
}

/// The list to pick from, or what is staged when the kind has no list, beside the files of the
/// review.
fn panel(s: &State, k: &Kit) -> El {
    let panel = sheet(k, row().key("panel").w_full().h(PANEL_HEIGHT));
    panel.child(if s.kind.has_list() { entries(s, k) } else { staged(s, k) }).child(files(s, k))
}

/// What a commit would take, and the way to unstage it.
fn staged(s: &State, k: &Kit) -> El {
    let column = col().key("staged-column").grow(1.0).min_w(0.0).h_full().padding(8.0).gap(4.0);
    let header = list_header(k, format!("Staged ({})", s.overview.staged_files()))
        .child(small_button("Unstage all").disabled(!s.can_unstage_all()).on_press(|s: &mut State| s.run(Action::UnstageAll)).build(k));
    if s.staged.is_empty() {
        return column.child(header).child(muted(k, "Nothing staged"));
    }
    let (kit, idle) = (k.clone(), s.running.is_none());
    let tree = file_tree(|s| &s.staged, |s| &mut s.staged, move |node, item| {
        item.trailing(row_action(&kit, "Unstage", idle, &node.path, |s, path| s.run(Action::Unstage(s.unstageable_in(path)))))
    });
    let tree = tree.name("Staged files").on_select(|s: &mut State, path, tx| s.scroll_to_staged(path, tx));
    column.child(header).child(tree.build(s, k).grow(1.0).min_h(0.0))
}

fn entries(s: &State, k: &Kit) -> El {
    let column = col().key("entries").grow(1.0).min_w(0.0).h_full().padding(8.0).gap(4.0).children(list_actions(s, k));
    if s.list.is_empty() {
        return column.child(text(format!("No {}", s.kind.label().to_lowercase())).font(k.ty.ui).color(k.c.text_muted));
    }
    let (detail_font, detail_color) = (k.ty.text_small, k.c.text_faint);
    let list = Tree::new(|s: &State| &s.list, |s: &mut State| &mut s.list, |entry: &Entry| entry.title.as_str())
        .name(s.kind.label())
        .row(move |entry, _| TreeItem::new(entry.title.clone()).trailing(one_line(entry.detail.clone()).truncate_head().max_w(DETAIL_MAX_WIDTH).font(detail_font).color(detail_color)))
        .on_select(|s: &mut State, _, _| s.show_selected())
        .build(s, k)
        .grow(1.0)
        .min_h(0.0);
    column.child(list)
}

/// What sits above the list: what can be done to its selected entry.
fn list_actions(s: &State, k: &Kit) -> Option<El> {
    match s.kind {
        Kind::Branches => Some(branch_actions(s, k)),
        Kind::Stashes if !s.list.is_empty() => Some(stash_actions(s, k)),
        _ => None,
    }
}

fn branch_actions(s: &State, k: &Kit) -> El {
    let idle = s.running.is_none();
    list_header(k, format!("On {}", s.overview.head)).child(small_button("Check out").disabled(!idle || s.branch_to_check_out().is_none()).on_press(State::check_out_selected).build(k))
}

fn stash_actions(s: &State, k: &Kit) -> El {
    let busy = s.running.is_some();
    let count = match s.overview.stashes.len() {
        1 => "1 stash".to_owned(),
        n => format!("{n} stashes"),
    };
    list_header(k, count)
        .child(small_button("Apply").disabled(busy).help("Bring the changes back and keep the stash").on_press(State::apply_selected_stash).build(k))
        .child(small_button("Pop").disabled(busy).help("Bring the changes back and remove the stash").on_press(State::pop_selected_stash).build(k))
        .child(small_button("Drop").disabled(busy).on_press(State::ask_to_drop_stash).build(k))
}

/// A row saying `summary`, ready for buttons at its end.
fn list_header(k: &Kit, summary: String) -> El {
    row().w_full().gap(4.0).px(8.0).h(24.0).items_center().child(one_line(summary).font(k.ty.ui).color(k.c.text_muted)).child(row().grow(1.0))
}

/// The files of the review, but for those wholly staged when it is of the worktree.
fn files(s: &State, k: &Kit) -> El {
    let column = col().key("files-column").grow(1.0).min_w(0.0).h_full().padding(8.0).gap(4.0);
    let (kit, worktree, idle) = (k.clone(), !s.kind.has_list(), s.running.is_none());
    let tree = file_tree(|s| &s.files, |s| &mut s.files, move |node, item| {
        if !worktree {
            return item;
        }
        item.trailing(row_action(&kit, "Stage", idle, &node.path, |s, path| s.run(Action::Stage(s.stageable_in(path)))))
    });
    let tree = tree.name("Changed files").on_select(|s: &mut State, path, tx| s.scroll_to_path(path, tx));
    let tree = tree.build(s, k).grow(1.0).min_h(0.0);
    if s.kind.has_list() {
        return column.child(tree);
    }
    let header = list_header(k, format!("Unstaged ({})", s.listed_files().count()))
        .child(small_button("Stage all").disabled(!s.can_stage_all()).on_press(|s: &mut State| s.run(Action::StageAll)).build(k));
    column.child(header).child(if s.files.is_empty() { muted(k, "Nothing unstaged") } else { tree })
}

/// A tree of files with their changed lines, `decorate` adding to each row.
fn file_tree(
    get: fn(&State) -> &TreeModel<FileNode<LineCounts>>,
    get_mut: fn(&mut State) -> &mut TreeModel<FileNode<LineCounts>>,
    decorate: impl Fn(&FileNode<LineCounts>, TreeItem) -> TreeItem + 'static,
) -> Tree<State, FileNode<LineCounts>> {
    Tree::new(get, get_mut, FileNode::label).row(move |node, row| {
        let item = TreeItem::file(node, row);
        let item = match node.data {
            Some((added, removed)) => item.changes(added, removed),
            None => item,
        };
        decorate(node, item)
    })
}

/// A button on the file list's row for `path`, a file or a folder, doing `act` to it.
fn row_action(k: &Kit, label: &str, idle: bool, path: &str, act: fn(&mut State, &str)) -> El {
    let path = path.to_owned();
    let button = Button::new(label).variant(ButtonVariant::GhostMuted).control_size(ControlSize::Small).disabled(!idle);
    button.on_press(move |s: &mut State| act(s, &path)).build(k)
}

fn muted(k: &Kit, line: &str) -> El {
    text(line.to_owned()).font(k.ty.ui).color(k.c.text_muted).px(8.0)
}

fn main(s: &State, k: &Kit) -> El {
    let area = sheet(k, col().key("main").w_full().grow(1.0).min_h(0.0));
    if let Some(error) = &s.error {
        return area.center().padding(16.0).child(text(error.clone()).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere());
    }
    let watch_error = s.watch_error.as_ref().map(|error| format!("Changes won't show up by themselves, so run Refresh from the palette (Ctrl+K). {error}"));
    let area = area
        .child(diff_status(s, k))
        .children(s.comments.error.as_ref().map(|error| error_line(k, error)))
        .children(watch_error.as_ref().map(|error| error_line(k, error)))
        .children(s.draft_unplaced().then(|| note(k, composer(s, k))))
        .children(s.unplaced_comment().map(|comment| note(k, comment_card(s, k, comment, CardPlace::AboveDiff))));
    if s.diff.files().is_empty() {
        return area.child(col().w_full().grow(1.0).center().child(text("Nothing to review").font(k.ty.ui).color(k.c.text_muted)));
    }
    let (annotation_kit, header_kit) = (k.clone(), k.clone());
    let diff = DiffView::new(|s: &State| &s.diff, |s: &mut State| &mut s.diff)
        .on_visible_file(State::follow_visible_file)
        .on_comment(|s: &mut State, lines, _| s.compose_on_lines(lines))
        .annotation(move |s: &State, annotation: &Annotation| annotated(s, &annotation_kit, annotation))
        .header_actions(move |s: &State, file| file_actions(s, &header_kit, file));
    area.child(diff.build(s, k))
}

/// What the diff compares and how much changed, and how it is laid out.
fn diff_status(s: &State, k: &Kit) -> El {
    let (added, removed) = s.diff.files().iter().fold((0, 0), |(a, r), file| (a + file.additions(), r + file.deletions()));
    let count = match s.diff.files().len() {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    let status = status_line()
        .children(s.shown_review().map(|review| compared(k, review, k.ty.ui_medium, k.c.text_base)))
        .child(one_line(format!("{count}  +{added} −{removed}")).font(k.ty.ui).color(k.c.text_muted))
        .child(row().grow(1.0))
        .child(layout_control(s, k));
    sheet_header(k, "diff-status", status)
}

/// The row at the top of a sheet, saying what it shows.
fn status_line() -> El {
    row().w_full().gap(12.0).px(12.0).py(6.0).items_center()
}

/// `status` set apart from the rest of its sheet.
fn sheet_header(k: &Kit, key: &str, status: El) -> El {
    col().key(key).w_full().child(status).child(row().w_full().h(1.0).fill(k.c.border_weak))
}

/// Unified or Split, for every diff.
fn layout_control(s: &State, k: &Kit) -> El {
    let split = s.diff.mode() == DiffMode::Split;
    let modes = SegmentedControl::new(&["Unified", "Split"], Some(split as usize)).fit().on_pick(|s: &mut State, i, tx: &mut Tx| {
        tx.motion(rune::tok::motion::SNAPPY);
        s.set_layout(if i == 1 { DiffMode::Split } else { DiffMode::Unified });
    });
    modes.build(k)
}

/// The two points `review` compares, set apart by an icon, in `font` and `color`.
fn compared(k: &Kit, review: &Review, font: TextStyle, color: Color) -> El {
    let name = |name: String| one_line(name).font(font).color(color);
    let (old, new) = review.sides();
    row()
        .label(review.title())
        .gap(6.0)
        .items_center()
        .shrink()
        .min_w(0.0)
        .child(name(old).max_w(DETAIL_MAX_WIDTH))
        .child(icon(&COMPARED, 14.0).color(k.c.text_muted).a11y_hidden())
        .child(name(new))
}

/// Every comment on one sheet, open first, each under the code it was written on.
fn comments_page(s: &State, k: &Kit) -> El {
    let area = sheet(k, col().key("comments-page").w_full().grow(1.0).min_h(0.0));
    let listed = s.comments.listed();
    let outdated = listed.iter().filter(|comment| comment.status() == Some("Outdated")).count();
    let resolved = listed.iter().filter(|comment| comment.resolved).count();
    let open = listed.len() - outdated - resolved;
    let to_copy = s.comments_to_copy().len();
    let copy = Button::new(if s.just_copied(None) { "Copied".to_owned() } else { copy_label(to_copy) })
        .variant(ButtonVariant::Ghost)
        .disabled(to_copy == 0)
        .help("Copy them as one prompt for an agent, leaving out outdated ones")
        .on_press_tx(State::copy_open_comments);
    let status = status_line()
        .child(one_line(format!("{open} open, {outdated} outdated, {resolved} resolved")).font(k.ty.ui_medium).color(k.c.text_base))
        .child(row().grow(1.0))
        .child(copy.build(k))
        .child(layout_control(s, k));
    let area = area
        .child(sheet_header(k, "comments-status", status))
        .children(s.comments.error.as_ref().map(|error| error_line(k, error)))
        .children(s.comments_off_sheet().into_iter().map(|comment| note(k, comment_card(s, k, comment, CardPlace::AboveDiff))));
    if listed.is_empty() {
        return area.child(col().w_full().grow(1.0).center().child(text("No comments yet").font(k.ty.ui).color(k.c.text_muted)));
    }
    let (annotation_kit, header_kit) = (k.clone(), k.clone());
    let entries = DiffView::new(|s: &State| &s.comment_sheet, |s: &mut State| &mut s.comment_sheet)
        .key("comment-sheet")
        .annotation(move |s: &State, annotation: &Annotation| match annotation.id.parse().ok().and_then(|id| s.comments.get(id)) {
            Some(comment) => note(&annotation_kit, comment_card(s, &annotation_kit, comment, CardPlace::Snippet)),
            None => col(),
        })
        .header_actions(move |s: &State, file| entry_actions(s, &header_kit, file));
    area.child(entries.build(s, k))
}

/// Where the comment of the comments page's entry `file` is now, the review it was written in
/// and its status, with the ways to copy it and to see it in its review.
fn entry_actions(s: &State, k: &Kit, file: usize) -> El {
    let Some(comment) = s.sheet_comment(file) else { return row() };
    let id = comment.id;
    let lines = match &comment.anchor.lines {
        None => "whole file".to_owned(),
        Some(lines) if lines.start == lines.end => format!("line {}", lines.start),
        Some(lines) => format!("lines {}–{}", lines.start, lines.end),
    };
    let copy = small_button(if s.just_copied(Some(id)) { "Copied" } else { "Copy" }).help("Copy it as a prompt for an agent");
    let show = s.review_place(&comment.review).map(|_| small_button("Show in diff").on_press(move |s: &mut State| s.show_in_diff(id)).build(k));
    row()
        .gap(8.0)
        .items_center()
        .child(text(lines).font(k.ty.text_small).color(k.c.text_muted))
        .child(compared(k, &comment.review, k.ty.text_small, k.c.text_muted).max_w(2.0 * DETAIL_MAX_WIDTH))
        .children(comment.status().map(|status| Badge::new(status).build(k)))
        .child(copy.on_press_tx(move |s: &mut State, tx: &mut Tx| s.copy_comment(id, tx)).build(k))
        .children(show)
}

fn error_line(k: &Kit, error: &str) -> El {
    col().w_full().px(16.0).py(8.0).child(text(error.to_owned()).font(k.ty.ui).color(k.c.danger.fg).wrap_anywhere())
}

/// Comments on a whole file go under its first line, which a file without lines lacks.
fn file_actions(s: &State, k: &Kit, file: usize) -> El {
    let button = |label: &str| Button::new(label).variant(ButtonVariant::GhostMuted).control_size(ControlSize::Small).disabled(s.running.is_some());
    let stage = (!s.stageable(file).is_empty()).then(|| button("Stage").on_press(move |s: &mut State| s.stage_file(file)).build(k));
    let unstage = (!s.unstageable(file).is_empty()).then(|| button("Unstage").on_press(move |s: &mut State| s.unstage_file(file)).build(k));
    let discard = (!s.discardable(file).is_empty()).then(|| button("Discard").on_press(move |s: &mut State| s.ask_to_discard(file)).build(k));
    let comment = (!s.diff.files()[file].hunks.is_empty()).then(|| {
        Button::new("Comment on file").variant(ButtonVariant::GhostMuted).control_size(ControlSize::Small).on_press_tx(move |s: &mut State, tx: &mut Tx| s.compose_on_file(file, tx)).build(k)
    });
    row().gap(4.0).items_center().children(stage).children(unstage).children(discard).children(comment)
}

fn annotated(s: &State, k: &Kit, annotation: &Annotation) -> El {
    if annotation.id == DRAFT_NOTE {
        return note(k, composer(s, k));
    }
    match annotation.id.parse().ok().and_then(|id| s.comments.get(id)) {
        Some(comment) => note(k, comment_card(s, k, comment, CardPlace::Line)),
        None => col(),
    }
}

/// `card` set apart from the code around it.
fn note(k: &Kit, card: El) -> El {
    let card = card.w_full().padding(12.0).gap(8.0).radius(8.0).fill(k.c.layer_01).border(1.0, k.c.border_weak);
    col().w_full().px(12.0).py(8.0).fill(k.c.bg_base).child(card)
}

fn composer(s: &State, k: &Kit) -> El {
    let location = s.draft.as_ref().map_or_else(String::new, |anchor| anchor.location());
    col().child(text(location).font(k.ty.ui_medium).color(k.c.text_base).wrap_anywhere()).child(comment_field(k, &s.draft_text, NEW_COMMENT))
}

/// A field for a comment's text, with its buttons.
fn comment_field(k: &Kit, edit: &TextEdit, writing: Writing) -> El {
    let Writing { text: field_text, save_label, save, cancel } = writing;
    let field = TextArea::new(edit)
        .width(None)
        .auto_grow(DRAFT_ROWS.0, DRAFT_ROWS.1)
        .placeholder("What should change here?")
        .label("Comment")
        .on_edit(field_text)
        .on_key(move |s: &mut State, event: &KeyEvent, _: &mut Tx| {
            if event.key == Key::Escape {
                cancel(s);
                return true;
            }
            multi_line_key(s, event, field_text, save)
        });
    let buttons = row()
        .gap(8.0)
        .items_center()
        .child(Button::new(save_label).disabled(edit.text.trim().is_empty()).on_press(save).build(k))
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(cancel).build(k))
        .child(text("Ctrl+Enter saves").font(k.ty.text_small).color(k.c.text_faint));
    col().w_full().gap(8.0).child(field.build(k)).child(buttons)
}

/// Handles the keys every multi-line field shares: Ctrl+Enter runs `save`, Enter starts a new
/// line and word chords work by word. Returns whether `event` was one of them.
fn multi_line_key(s: &mut State, event: &KeyEvent, text: for<'a> fn(&'a mut State) -> &'a mut TextEdit, save: fn(&mut State)) -> bool {
    if event.key != Key::Enter {
        return edit_by_word(text(s), event);
    }
    if event.mods.cmd {
        save(s);
    } else {
        // Without Shift it would go looking for a default button.
        text(s).key(&KeyEvent { key: Key::Enter, mods: Mods { shift: true, ..Mods::default() } }, None);
    }
    true
}

/// Applies `event` to `edit` when it is a word chord; returns whether it was.
fn edit_by_word(edit: &mut TextEdit, event: &KeyEvent) -> bool {
    let word = word_chord(event);
    if let Some(word) = &word {
        edit.key(word, None);
    }
    word.is_some()
}

/// The Alt chord for a Ctrl word chord. Rune reads Ctrl as Cmd off macOS, which takes the whole line.
fn word_chord(event: &KeyEvent) -> Option<KeyEvent> {
    let Mods { ctrl, cmd, shift, .. } = event.mods;
    let by_word = matches!(event.key, Key::Backspace | Key::Left | Key::Right);
    (by_word && ctrl && cmd).then(|| KeyEvent { key: event.key.clone(), mods: Mods { alt: true, shift, ..Mods::default() } })
}

/// A comment, saying where it is unless it sits under its snippet, and quoting its code when it
/// sits above a diff.
fn comment_card(s: &State, k: &Kit, comment: &Comment, place: CardPlace) -> El {
    let id = comment.id;
    let editing = s.editing == Some(id);
    let actions = (!editing).then(|| comment_actions(k, comment, small_button("Edit").on_press(move |s: &mut State| s.start_edit(id))));
    let body = if editing { comment_field(k, &s.edit_text, EDITED_COMMENT) } else { text(comment.body.clone()).font(k.ty.body).color(k.c.text_base).wrap_anywhere() };
    if place == CardPlace::Snippet {
        // Its entry's header says where it is, so the actions sit beside the text.
        return col().child(row().w_full().gap(8.0).items_start().child(body.grow(1.0).shrink().min_w(0.0)).children(actions));
    }
    let heading = row()
        .w_full()
        .gap(8.0)
        .items_center()
        .child(text(comment.anchor.location()).font(k.ty.ui_medium).color(k.c.text_base).wrap_anywhere().shrink().min_w(0.0))
        .children(comment.status().map(|status| Badge::new(status).build(k)))
        .child(row().grow(1.0))
        .children(actions);
    let quote = comment.anchor.lines.as_ref().filter(|_| place == CardPlace::AboveDiff).map(|lines| text(lines.quoted.join("\n")).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere());
    col().child(heading).children(quote).child(body)
}

fn small_button(label: &str) -> Button {
    Button::new(label).variant(ButtonVariant::Ghost).control_size(ControlSize::Small)
}

/// `edit`, then what else can be done to the comment.
fn comment_actions(k: &Kit, comment: &Comment, edit: Button) -> El {
    let id = comment.id;
    let (label, toggle): (_, fn(&mut State, u64)) = if comment.resolved { ("Reopen", State::reopen_comment) } else { ("Resolve", State::resolve_comment) };
    row()
        .gap(4.0)
        .child(edit.build(k))
        .child(small_button(label).on_press(move |s: &mut State| toggle(s, id)).build(k))
        .child(small_button("Delete").on_press(move |s: &mut State| s.ask_to_delete(id)).build(k))
}

fn delete_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(comment) = s.deleting.and_then(|id| s.comments.get(id)) else { return };
    let location = text(comment.anchor.location()).font(k.ty.ui).color(k.c.text_base).wrap_anywhere();
    confirm_dialog(k, overlays, Dialog::new("delete-comment", "Delete comment?").description(UNDONE), location, "Delete", State::confirm_delete, State::cancel_delete);
}

/// Asks before an action that can't be taken back.
fn action_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(action) = &s.confirming else { return };
    let (title, subject, consequence) = match action {
        Action::DiscardChanges(paths) => ("Discard changes?", paths.join("\n"), "Everything not committed in this file is lost."),
        Action::DiscardUnstaged(paths) => ("Discard changes?", paths.join("\n"), "Everything not staged in this file is lost."),
        Action::DropStash(stash) => ("Drop stash?", stash.clone(), "The changes it holds are lost."),
        Action::RemoveRemote(name) => ("Remove remote?", name.clone(), "Its branches go from here too, and local branches stop tracking them."),
        _ => return,
    };
    let dialog = Dialog::new("confirm-action", title);
    // Adding a remote again and fetching brings its branches back.
    let dialog = if matches!(action, Action::RemoveRemote(_)) { dialog } else { dialog.description(UNDONE) };
    let body = col()
        .w_full()
        .gap(8.0)
        .child(text(subject).font(k.ty.ui).color(k.c.text_base).wrap_anywhere())
        .child(text(consequence).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere());
    confirm_dialog(k, overlays, dialog, body, action.label(), State::confirm_action, State::cancel_action);
}

/// `dialog`, holding `body`, with the buttons to go ahead or back out.
fn confirm_dialog(k: &Kit, overlays: &mut Vec<El>, dialog: Dialog, body: El, label: &str, confirm: fn(&mut State), cancel: fn(&mut State)) {
    let buttons = row()
        .w_full()
        .padding(16.0)
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(cancel).build(k))
        .child(Button::new(label).variant(ButtonVariant::Danger).on_press(confirm).build(k));
    dialog.width(CONFIRM_WIDTH).fit().open(true).on_close(cancel).child(col().w_full().px(16.0).child(body)).child(buttons).build(k, overlays);
}

/// The message for what is staged. Closing keeps it, and so does a commit that failed.
fn commit_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    if !s.committing {
        return;
    }
    let field = TextArea::new(&s.commit_text)
        .width(None)
        .rows(COMMIT_ROWS)
        .placeholder("Commit message")
        .label("Commit message")
        .on_edit(|s: &mut State| &mut s.commit_text)
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| multi_line_key(s, event, |s| &mut s.commit_text, State::commit));
    let plan = s.overview.commit_plan();
    let label = match (plan.files(), plan.takes_all) {
        (1, _) => "Commit 1 file".to_owned(),
        (files, true) => format!("Commit all {files} files"),
        (files, false) => format!("Commit {files} files"),
    };
    let buttons = row()
        .w_full()
        .gap(8.0)
        .items_center()
        .child(one_line("Ctrl+Enter commits".to_owned()).font(k.ty.text_small).color(k.c.text_faint))
        .child(row().grow(1.0))
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(State::cancel_commit).build(k))
        .child(Button::new(label).disabled(!s.can_commit()).on_press(State::commit).build(k));
    let on = text(format!("On {}", s.overview.head)).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere();
    let message = col().grow(1.0).min_w(0.0).gap(8.0).child(on).child(field.build(k)).child(buttons);
    Dialog::new("commit", "Commit")
        .width(COMMIT_WIDTH)
        .fit()
        .open(true)
        .on_close(State::cancel_commit)
        .child(row().w_full().px(16.0).pb(16.0).gap(24.0).child(commit_summary(k, plan)).child(message))
        .build(k, overlays);
}

/// What the commit does to files, and what it leaves uncommitted.
fn commit_summary(k: &Kit, plan: CommitPlan) -> El {
    let heading = |title: &str| one_line(title.to_owned()).font(k.ty.ui).color(k.c.text_muted);
    let count = |files: usize, what: &str, color| (files > 0).then(|| one_line(format!("{files} {what}")).font(k.ty.ui).color(color).pl(8.0));
    let summary = col().key("commit-summary").w(COMMIT_SUMMARY_WIDTH).gap(4.0).child(heading("This commit"));
    let summary = summary.children([count(plan.new, "new", k.c.text_base), count(plan.changed, "changed", k.c.text_base), count(plan.deleted, "deleted", k.c.text_base)].into_iter().flatten());
    if plan.partly + plan.left_out == 0 {
        return summary;
    }
    summary
        .child(heading("Stays uncommitted").pt(12.0))
        .children([count(plan.partly, "partly committed", k.c.warning.fg), count(plan.left_out, "not included", k.c.text_base)].into_iter().flatten())
}

/// What a stash takes and what it is called.
fn stash_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(takes) = s.stashing else { return };
    let field = TextInput::new(&s.stash_text)
        .width(None)
        .placeholder("Name, if you want one")
        .label("Stash name")
        .on_edit(|s: &mut State| &mut s.stash_text)
        .on_submit(|s: &mut State, _: &mut Tx| s.stash())
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| edit_by_word(&mut s.stash_text, event));
    let nothing_staged = s.overview.staged_files() == 0;
    let choices = [
        (Stashed::WithNewFiles, "Changes and new files", false),
        (Stashed::Tracked, "Changes, leaving new files", false),
        (Stashed::Staged, "Only what's staged", nothing_staged),
    ];
    let group = choices.into_iter().fold(RadioGroup::new().label("Takes"), |group, (choice, label, disabled)| {
        group.item(RadioItem::new(label, takes == choice).disabled(disabled).on_select(move |s: &mut State| s.stashing = Some(choice)))
    });
    let buttons = row()
        .w_full()
        .padding(16.0)
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(State::cancel_stash).build(k))
        .child(Button::new("Stash").disabled(!s.stash_ready()).on_press(State::stash).build(k));
    Dialog::new("stash", "Stash")
        .width(CONFIRM_WIDTH)
        .fit()
        .open(true)
        .on_close(State::cancel_stash)
        .child(col().w_full().px(16.0).gap(16.0).child(field.build(k)).child(group.build(k)))
        .child(buttons)
        .build(k, overlays);
}

/// What git said when an action failed.
fn failure_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(failure) = &s.action_failure else { return };
    let said = text(failure.message.clone()).font(k.ty.mono).color(k.c.text_base).wrap_anywhere();
    let output = col().key("git-output").w_full().max_h(FAILURE_MAX_HEIGHT).px(16.0).scroll(ScrollSpec { stick_bottom: false }).child(said);
    let buttons = row().w_full().padding(16.0).justify_end().child(Button::new("Close").variant(ButtonVariant::Ghost).on_press(State::dismiss_failure).build(k));
    Dialog::new("action-failure", format!("{} failed", failure.action)).width(FAILURE_WIDTH).fit().open(true).on_close(State::dismiss_failure).child(output).child(buttons).build(k, overlays);
}

/// What the last action did.
fn outcome_toast(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(outcome) = s.outcome() else { return };
    let toast = Toast::new("outcome", outcome).icon(k, k.glyph.check).on_close(State::dismiss_outcome).build(k);
    overlays.push(ToastRegion::new().toast(toast).build(k));
}

fn palette_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(active) = s.palette else { return };
    let field = TextInput::new(&s.palette_text)
        .width(None)
        .placeholder("Type to filter")
        .label("Command")
        .on_edit(|s: &mut State| &mut s.palette_text)
        .on_change(|s: &mut State, _: &mut Tx| s.palette = Some(0))
        .on_submit(move |s: &mut State, tx: &mut Tx| s.run_command(s.palette.unwrap_or_default(), tx))
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| match event.key {
            Key::Up => {
                s.move_palette(-1);
                true
            }
            Key::Down => {
                s.move_palette(1);
                true
            }
            _ => edit_by_word(&mut s.palette_text, event),
        });
    let commands = s.palette_commands();
    let rows = commands.iter().enumerate().map(|(index, command)| palette_row(k, command, index == active, index));
    let list = col().key("commands").role("listbox").w_full().gap(2.0).children(rows);
    let list = if commands.is_empty() { list.child(muted(k, "Nothing matches").py(8.0)) } else { list };
    Dialog::new("palette", "Commands").width(PICKER_WIDTH).fit().open(true).on_close(State::close_palette).child(col().w_full().px(16.0).pb(16.0).gap(12.0).child(field.build(k)).child(list)).build(k, overlays);
    pin_to_top(overlays);
}

/// The recent repositories, or the folders completing the path typed.
fn open_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(active) = s.shell.opener else { return };
    let field = TextInput::new(&s.shell.open_text)
        .width(None)
        .placeholder("A folder, like ~/dev/project")
        .label("Folder")
        .on_edit(|s: &mut State| &mut s.shell.open_text)
        .on_change(|s: &mut State, _: &mut Tx| s.offer_folders())
        .on_submit(|s: &mut State, _: &mut Tx| s.open_choice(s.shell.opener.unwrap_or_default()))
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| match event.key {
            Key::Up => {
                s.move_opener(-1);
                true
            }
            Key::Down => {
                s.move_opener(1);
                true
            }
            Key::Tab => {
                s.complete_path();
                true
            }
            _ => edit_by_word(&mut s.shell.open_text, event),
        });
    let offer = &s.shell.offer;
    let rows = offer.choices.iter().enumerate().map(|(index, choice)| choice_row(k, choice, index == active, index));
    let list = col().key("folders").role("listbox").w_full().gap(2.0).children(rows);
    let typed = s.shell.open_text.text.trim();
    let none = match typed {
        "" => "No recent repositories",
        _ if typed.ends_with('/') => "No folders in it, so Enter opens it",
        _ => "No folders match",
    };
    let list = if offer.choices.is_empty() && !s.offer_pending() { list.child(muted(k, none).py(8.0)) } else { list };
    let more = (offer.more > 0).then(|| muted(k, &format!("{} more, type to narrow them down", offer.more)).py(4.0));
    let hint = text("Tab completes the folder, Enter opens it").font(k.ty.text_small).color(k.c.text_faint).px(8.0);
    let content = col().w_full().px(16.0).pb(16.0).gap(12.0).child(field.build(k)).child(list.children(more)).child(hint);
    Dialog::new("open", "Open a repository").width(PICKER_WIDTH).fit().open(true).on_close(State::close_opener).child(content).build(k, overlays);
    pin_to_top(overlays);
}

/// The branches to switch to, and a new one when the name typed is new.
fn branch_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(active) = s.switcher else { return };
    let field = TextInput::new(&s.branch_text)
        .width(None)
        .placeholder("Branch name")
        .label("Branch")
        .on_edit(|s: &mut State| &mut s.branch_text)
        .on_change(|s: &mut State, _: &mut Tx| s.switcher = Some(0))
        .on_submit(|s: &mut State, _: &mut Tx| s.pick_branch(s.switcher.unwrap_or_default()))
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| match event.key {
            Key::Up => {
                s.move_switcher(-1);
                true
            }
            Key::Down => {
                s.move_switcher(1);
                true
            }
            _ => edit_by_word(&mut s.branch_text, event),
        });
    let offer = s.branch_offer();
    let mut list = col().key("branches").role("listbox").w_full().gap(2.0);
    for (index, choice) in offer.choices.iter().enumerate() {
        let before = index.checked_sub(1).map(|before| &offer.choices[before]);
        match choice {
            BranchChoice::Remote(_) if !matches!(before, Some(BranchChoice::Remote(_))) => list = list.child(separator(k, Some("Only on a remote"))),
            BranchChoice::Create(_) if before.is_some() => list = list.child(separator(k, None)),
            _ => {}
        }
        list = list.child(branch_row(s, k, choice, index == active, index));
    }
    let list = if offer.choices.is_empty() { list.child(muted(k, "No branches match").py(8.0)) } else { list };
    let more = (offer.more > 0).then(|| muted(k, &format!("{} more, type to narrow them down", offer.more)).py(4.0));
    let footer = row()
        .w_full()
        .gap(4.0)
        .items_center()
        .child(one_line("Type a new name to create a branch".to_owned()).font(k.ty.text_small).color(k.c.text_faint).px(8.0))
        .child(row().grow(1.0))
        .child(small_button("Fetch").shortcut("Ctrl+Shift+F").help("Get every remote's branches").disabled(!s.can_fetch()).on_press(|s: &mut State| s.run(Action::Fetch)).build(k))
        .child(small_button("Remotes").on_press(State::open_remotes).build(k));
    let content = col().w_full().px(16.0).pb(16.0).gap(12.0).child(field.build(k)).child(list.children(more)).child(footer);
    Dialog::new("switch-branch", "Switch branch").width(PICKER_WIDTH).fit().open(true).on_close(State::close_switcher).child(content).build(k, overlays);
    pin_to_top(overlays);
}

/// A row of the branch switcher.
fn branch_row(s: &State, k: &Kit, choice: &BranchChoice, active: bool, index: usize) -> El {
    let (title, detail) = match choice {
        BranchChoice::Local(branch) if branch.id == s.overview.head => (branch.id.clone(), "current".to_owned()),
        BranchChoice::Local(branch) => (branch.id.clone(), branch.detail.clone()),
        BranchChoice::Remote(branch) => (branch.id(), branch.age.clone()),
        BranchChoice::Create(name) => (format!("Create branch {name}"), format!("from {}", s.overview.head)),
    };
    let detail = one_line(detail).truncate_head().max_w(DETAIL_MAX_WIDTH).font(k.ty.text_small).color(k.c.text_faint);
    picker_row(k, format!("branch{index}"), title, Some(detail), active, move |s, _| s.pick_branch(index))
}

/// A line between parts of a list, saying what follows when `label` does.
fn separator(k: &Kit, label: Option<&str>) -> El {
    let line = || row().grow(1.0).h(1.0).fill(k.c.border_weak);
    let label = label.map(|label| [text(label.to_owned()).font(k.ty.text_small).color(k.c.text_faint), line()]);
    row().w_full().gap(8.0).pt(6.0).pb(2.0).items_center().child(line()).children(label.into_iter().flatten())
}

/// The remotes, each with its URL, and a way to add one.
fn remotes_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    if !s.managing_remotes {
        return;
    }
    let idle = s.running.is_none();
    let rows = s.overview.remotes.iter().map(|remote| {
        let name = remote.name.clone();
        row()
            .key(format!("remote-{}", remote.name))
            .w_full()
            .h(PICKER_ROW_HEIGHT)
            .px(8.0)
            .gap(12.0)
            .items_center()
            // A column as wide as the name field, which the URL can't squeeze.
            .child(row().w(REMOTE_NAME_WIDTH).min_w(REMOTE_NAME_WIDTH).child(one_line(remote.name.clone()).font(k.ty.ui_medium).color(k.c.text_base)))
            .child(one_line(remote.url.clone()).truncate_head().font(k.ty.ui).color(k.c.text_muted))
            .child(row().grow(1.0))
            .child(small_button("Remove").disabled(!idle).on_press(move |s: &mut State| s.ask_to_remove_remote(&name)).build(k))
    });
    let list = col().key("remotes").w_full().gap(2.0).children(rows);
    let list = if s.overview.remotes.is_empty() { list.child(muted(k, "No remotes yet").py(8.0)) } else { list };
    let name = TextInput::new(&s.remote_name_text)
        .width(Some(REMOTE_NAME_WIDTH))
        .placeholder(s.default_remote_name().unwrap_or("Name"))
        .label("Remote name")
        .on_edit(|s: &mut State| &mut s.remote_name_text)
        .on_submit(|s: &mut State, _: &mut Tx| s.add_remote())
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| edit_by_word(&mut s.remote_name_text, event));
    let url = TextInput::new(&s.remote_url_text)
        .width(None)
        .placeholder("URL, like git@github.com:owner/project.git")
        .label("Remote URL")
        .on_edit(|s: &mut State| &mut s.remote_url_text)
        .on_submit(|s: &mut State, _: &mut Tx| s.add_remote())
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| edit_by_word(&mut s.remote_url_text, event));
    let add = row()
        .key("add-remote")
        .w_full()
        .gap(8.0)
        .items_center()
        .child(name.build(k))
        .child(col().grow(1.0).min_w(0.0).child(url.build(k)))
        .child(Button::new("Add").disabled(!s.can_add_remote()).on_press(State::add_remote).build(k));
    let content = col().w_full().px(16.0).pb(16.0).gap(12.0).child(list).child(add);
    Dialog::new("remotes", "Remotes").width(PICKER_WIDTH).fit().open(true).on_close(State::close_remotes).child(content).build(k, overlays);
}

/// Pins the dialog just built to the top of the window, so it holds still as its list shrinks.
/// Dialog pushes the layer centring its panel last.
fn pin_to_top(overlays: &mut Vec<El>) {
    if let Some(layer) = overlays.pop() {
        overlays.push(layer.items_start());
    }
}

/// A command in the palette, its key at the end.
fn palette_row(k: &Kit, command: &Command, active: bool, index: usize) -> El {
    let shortcut = command.shortcut.map(|key| text(key).font(k.ty.mono).color(k.c.text_faint));
    picker_row(k, format!("command{index}"), command.title(), shortcut, active, move |s, tx| s.run_command(index, tx))
}

/// A folder in the open dialog, with where it is or whether it is a repository.
fn choice_row(k: &Kit, choice: &Choice, active: bool, index: usize) -> El {
    let detail = (!choice.detail.is_empty()).then(|| one_line(choice.detail.clone()).truncate_head().max_w(PLACE_MAX_WIDTH).font(k.ty.text_small).color(k.c.text_faint));
    picker_row(k, format!("folder{index}"), choice.name.clone(), detail, active, move |s, _| s.open_choice(index))
}

/// A row of a dialog's list, `trailing` at its end. The highlighted one is marked, as Enter
/// picks it.
fn picker_row(k: &Kit, key: String, title: String, trailing: Option<El>, active: bool, pick: impl Fn(&mut State, &mut Tx) + 'static) -> El {
    let marker = row().w(PICKER_MARKER_WIDTH).children(active.then(|| text("›").font(k.ty.ui_medium).color(k.c.text_faint)));
    let item = row()
        .key(key)
        .role("option")
        .label(title.clone())
        .w_full()
        .h(PICKER_ROW_HEIGHT)
        .px(8.0)
        .gap(4.0)
        .items_center()
        .radius(k.radius.md)
        .child(marker)
        .child(one_line(title).font(k.ty.ui_medium).color(k.c.text_base))
        .child(row().grow(1.0))
        .children(trailing)
        .on_press_tx(pick);
    // Highlighted as the file lists mark their selection.
    if active { item.fill(TreeStyle::from_kit(k).selected) } else { item.hover(k.c.hover) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Shell;
    use crate::comments::Comments;
    use crate::git::testing::Fixture;
    use crate::open::Opened;

    #[test]
    fn in_a_multi_line_field_enter_starts_a_line_and_ctrl_enter_saves() {
        let fixture = Fixture::new();
        let mut state = State::new(Some(Opened::Repository(fixture.repo.clone(), Comments::load(&fixture.repo))), Shell::default());
        state.draft_text = TextEdit::new("one".to_owned());
        let press = |mods| KeyEvent { key: Key::Enter, mods };
        let text: for<'a> fn(&'a mut State) -> &'a mut TextEdit = |s| &mut s.draft_text;
        let save: fn(&mut State) = |s| s.managing_remotes = true;

        assert!(multi_line_key(&mut state, &press(Mods::default()), text, save));
        assert_eq!((state.draft_text.text.as_str(), state.managing_remotes), ("one\n", false));

        assert!(multi_line_key(&mut state, &press(Mods { ctrl: true, cmd: true, ..Mods::default() }), text, save));
        assert_eq!((state.draft_text.text.as_str(), state.managing_remotes), ("one\n", true));
    }

    #[test]
    fn ctrl_backspace_takes_one_word_of_the_line() {
        let press = |mods| KeyEvent { key: Key::Backspace, mods };
        let mut edit = TextEdit::default();
        edit.key(&KeyEvent { key: Key::Text("one two".into()), mods: Mods::default() }, None);

        edit.key(&word_chord(&press(Mods { ctrl: true, cmd: true, ..Mods::default() })).unwrap(), None);

        assert_eq!(edit.text, "one ");
        assert!(word_chord(&press(Mods::default())).is_none());
    }
}
