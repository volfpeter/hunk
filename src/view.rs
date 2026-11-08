//! The window: a bar to pick what to review, the lists behind that choice, and the diff.

use rune::{Cx, El, ScrollSpec, TextEdit, Tx, col, row, text};
use rune::{Key, KeyEvent, Mods};
use rune_kit::diff::{Annotation, DiffMode, DiffView};
use rune_kit::{Badge, Button, ButtonVariant, ControlSize, Dialog, FileNode, Kit, RadioGroup, RadioItem, SegmentedControl, Spinner, TextArea, TextInput, Tree, TreeItem, TreeModel, TreeStyle, root};

use crate::action::{Action, Stashed};
use crate::app::{Command, DRAFT_NOTE, Kind, State};
use crate::comments::Comment;
use crate::open::{Choice, tilde};
use crate::overview::{Entry, LineCounts};

const BAR_HEIGHT: f32 = 48.0;
/// The page showing between the sheets, and around them.
const SHEET_GAP: f32 = 8.0;
const PANEL_HEIGHT: f32 = 200.0;
const DRAFT_ROWS: (usize, usize) = (3, 12);
const COMMIT_ROWS: usize = 4;
const CONFIRM_WIDTH: f32 = 400.0;
const COMMIT_WIDTH: f32 = 560.0;
const FAILURE_WIDTH: f32 = 640.0;
/// The most height git's output takes in a dialog before it scrolls.
const FAILURE_MAX_HEIGHT: f32 = 320.0;
/// Dialogs whose rows are picked with the keyboard: the palette and the open dialog.
const PICKER_WIDTH: f32 = 560.0;
const PICKER_ROW_HEIGHT: f32 = 32.0;
const PICKER_MARKER_WIDTH: f32 = 12.0;
/// The most width a list entry's detail takes from its title.
const DETAIL_MAX_WIDTH: f32 = 240.0;
/// The most width where a folder is takes from its name.
const PLACE_MAX_WIDTH: f32 = 360.0;
/// The most width the repository's name and its branch each take from the bar.
const HEAD_MAX_WIDTH: f32 = 160.0;

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
    delete_dialog(s, &k, &mut overlays);
    action_dialog(s, &k, &mut overlays);
    new_branch_dialog(s, &k, &mut overlays);
    commit_dialog(s, &k, &mut overlays);
    stash_dialog(s, &k, &mut overlays);
    failure_dialog(s, &k, &mut overlays);
    palette_dialog(s, &k, &mut overlays);
    open_dialog(s, &k, &mut overlays);
    if !s.has_repository() {
        return root(col().fill(k.c.page).padding(SHEET_GAP).child(welcome(s, &k)), overlays);
    }
    let sheets = col().w_full().grow(1.0).min_h(0.0).px(SHEET_GAP).pb(SHEET_GAP).gap(SHEET_GAP).child(panel(s, &k)).child(main(s, &k));
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
    let open = s.comments.open().len();
    let labels = Kind::ALL.map(|kind| match kind {
        Kind::Comments if open > 0 => format!("{} ({open})", kind.label()),
        _ => kind.label().to_owned(),
    });
    let selected = Kind::ALL.iter().position(|kind| *kind == s.kind);
    let kinds = SegmentedControl::new(&labels, selected).fit().on_pick(|s: &mut State, i, _: &mut Tx| s.select_kind(Kind::ALL[i]));

    let behind = s.overview.upstream.map_or(0, |upstream| upstream.behind);
    let ahead = s.overview.upstream.map_or(0, |upstream| upstream.ahead);
    let pull = if behind > 0 { format!("Pull ↓{behind}") } else { "Pull".to_owned() };
    let push = if ahead > 0 { format!("Push ↑{ahead}") } else { "Push".to_owned() };

    let name = s.repo_name();
    let repository = Button::new(name.clone())
        .variant(ButtonVariant::Ghost)
        .label_with(move |font, ink| one_line(name).font(font).color(ink))
        .help("Open another repository")
        .shortcut("Ctrl+O")
        .disabled(!s.can_open())
        .on_press(State::open_opener)
        .build(k)
        .key("repository")
        .max_w(HEAD_MAX_WIDTH)
        .min_w(0.0);

    row()
        .key("bar")
        .w_full()
        .h(BAR_HEIGHT)
        .px(16.0)
        .gap(12.0)
        .items_center()
        .child(repository)
        .child(one_line(s.overview.head.clone()).max_w(HEAD_MAX_WIDTH).font(k.ty.ui).color(k.c.text_muted))
        .child(kinds.build(k))
        .child(row().grow(1.0))
        .children(s.running.map(|action| progress(k, action)))
        .child(
            Button::new(if s.just_copied() { "Copied" } else { "Copy prompt" })
                .variant(ButtonVariant::Ghost)
                .disabled(open == 0)
                .help("Copy all open comments as a prompt for an agent")
                .on_press_tx(State::copy_prompt)
                .build(k),
        )
        .child(Button::new("Refresh").variant(ButtonVariant::Ghost).on_press(|s: &mut State| s.refresh()).build(k))
        .child(Button::new(pull).variant(ButtonVariant::Ghost).disabled(s.running.is_some()).on_press(|s: &mut State| s.run(Action::Pull)).build(k))
        .child(Button::new(push).variant(ButtonVariant::Ghost).disabled(!s.can_push()).on_press(|s: &mut State| s.run(Action::Push)).build(k))
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
        .child(small_button("Commit").disabled(!s.can_start_commit()).on_press(State::start_commit).build(k));
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
        Kind::Comments if !s.list.is_empty() => Some(comments_summary(s, k)),
        Kind::Branches => Some(branch_actions(s, k)),
        Kind::Stashes if !s.list.is_empty() => Some(stash_actions(s, k)),
        _ => None,
    }
}

/// How many comments there are, and what can be done to the selected one.
fn comments_summary(s: &State, k: &Kit) -> El {
    let open = s.comments.open().len();
    let resolved = s.comments.listed().len() - open;
    let actions = s.selected_comment().filter(|comment| s.editing != Some(comment.id));
    list_header(k, format!("{open} open, {resolved} resolved")).children(actions.map(|comment| comment_actions(k, comment, small_button("Edit").on_press_tx(State::edit_selected))))
}

fn branch_actions(s: &State, k: &Kit) -> El {
    let idle = s.running.is_none();
    list_header(k, format!("On {}", s.overview.head))
        .child(small_button("Check out").disabled(!idle || s.branch_to_check_out().is_none()).on_press(State::check_out_selected).build(k))
        .child(small_button("New branch").disabled(!idle).on_press(State::start_new_branch).build(k))
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
        .child(small_button("Stash").disabled(!s.can_stash()).help("Put changes aside").on_press(State::start_stash).build(k));
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
    let watch_error = s.watch_error.as_ref().map(|error| format!("Changes won't show up by themselves, so press Refresh. {error}"));
    let area = area
        .child(diff_status(s, k))
        .children(s.comments.error.as_ref().map(|error| error_line(k, error)))
        .children(watch_error.as_ref().map(|error| error_line(k, error)))
        .children(s.draft_unplaced().then(|| note(k, composer(s, k))))
        .children(s.unplaced_comment().map(|comment| note(k, comment_card(s, k, comment, true))));
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
    let title = s.review().map_or_else(String::new, |review| review.title());
    let split = s.diff.mode() == DiffMode::Split;
    let modes = SegmentedControl::new(&["Unified", "Split"], Some(split as usize)).fit().on_pick(|s: &mut State, i, tx: &mut Tx| {
        tx.motion(rune::tok::motion::SNAPPY);
        s.diff.set_mode(if i == 1 { DiffMode::Split } else { DiffMode::Unified });
    });
    let status = row()
        .w_full()
        .gap(12.0)
        .px(12.0)
        .py(6.0)
        .items_center()
        .child(one_line(title).font(k.ty.ui_medium).color(k.c.text_base))
        .child(one_line(format!("{count}  +{added} −{removed}")).font(k.ty.ui).color(k.c.text_muted))
        .child(row().grow(1.0))
        .child(modes.build(k));
    col().key("diff-status").w_full().child(status).child(row().w_full().h(1.0).fill(k.c.border_weak))
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
        Some(comment) => note(k, comment_card(s, k, comment, false)),
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

/// A comment, with the code it quotes when `quoted`.
fn comment_card(s: &State, k: &Kit, comment: &Comment, quoted: bool) -> El {
    let id = comment.id;
    let editing = s.editing == Some(id);
    let heading = row()
        .w_full()
        .gap(8.0)
        .items_center()
        .child(text(comment.anchor.location()).font(k.ty.ui_medium).color(k.c.text_base).wrap_anywhere().shrink().min_w(0.0))
        .children(comment.status().map(|status| Badge::new(status).build(k)))
        .child(row().grow(1.0))
        .children((!editing).then(|| comment_actions(k, comment, small_button("Edit").on_press(move |s: &mut State| s.start_edit(id)))));
    let quote = comment.anchor.lines.as_ref().filter(|_| quoted).map(|lines| text(lines.quoted.join("\n")).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere());
    let body = if editing { comment_field(k, &s.edit_text, EDITED_COMMENT) } else { text(comment.body.clone()).font(k.ty.body).color(k.c.text_base).wrap_anywhere() };
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
    confirm_dialog(k, overlays, Dialog::new("delete-comment", "Delete comment?"), location, "Delete", State::confirm_delete, State::cancel_delete);
}

/// Asks before an action that can't be taken back.
fn action_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(action) = &s.confirming else { return };
    let (title, subject, consequence) = match action {
        Action::DiscardChanges(paths) => ("Discard changes?", paths.join("\n"), "Everything not committed in this file is lost."),
        Action::DiscardUnstaged(paths) => ("Discard changes?", paths.join("\n"), "Everything not staged in this file is lost."),
        Action::DropStash(stash) => ("Drop stash?", stash.clone(), "The changes it holds are lost."),
        _ => return,
    };
    let body = col()
        .w_full()
        .gap(8.0)
        .child(text(subject).font(k.ty.ui).color(k.c.text_base).wrap_anywhere())
        .child(text(consequence).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere());
    confirm_dialog(k, overlays, Dialog::new("confirm-action", title), body, action.label(), State::confirm_action, State::cancel_action);
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
    dialog.description("This can't be undone.").width(CONFIRM_WIDTH).fit().open(true).on_close(cancel).child(col().w_full().px(16.0).child(body)).child(buttons).build(k, overlays);
}

fn new_branch_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    if !s.naming_branch {
        return;
    }
    let field = TextInput::new(&s.branch_text)
        .width(None)
        .placeholder("Branch name")
        .label("Branch name")
        .on_edit(|s: &mut State| &mut s.branch_text)
        .on_submit(|s: &mut State, _: &mut Tx| s.create_branch())
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| edit_by_word(&mut s.branch_text, event));
    let buttons = row()
        .w_full()
        .padding(16.0)
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(State::cancel_new_branch).build(k))
        .child(Button::new("Create").disabled(s.branch_text.text.trim().is_empty()).on_press(State::create_branch).build(k));
    Dialog::new("new-branch", "New branch")
        .width(CONFIRM_WIDTH)
        .fit()
        .open(true)
        .on_close(State::cancel_new_branch)
        .child(col().w_full().px(16.0).gap(8.0).child(text(format!("From {}", s.overview.head)).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere()).child(field.build(k)))
        .child(buttons)
        .build(k, overlays);
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
    let label = match s.overview.staged_files() {
        1 => "Commit 1 file".to_owned(),
        staged => format!("Commit {staged} files"),
    };
    let buttons = row()
        .w_full()
        .padding(16.0)
        .gap(8.0)
        .items_center()
        .child(one_line("Ctrl+Enter commits".to_owned()).font(k.ty.text_small).color(k.c.text_faint))
        .child(row().grow(1.0))
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(State::cancel_commit).build(k))
        .child(Button::new(label).disabled(!s.can_commit()).on_press(State::commit).build(k));
    let on = text(format!("On {}", s.overview.head)).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere();
    Dialog::new("commit", "Commit")
        .width(COMMIT_WIDTH)
        .fit()
        .open(true)
        .on_close(State::cancel_commit)
        .child(col().w_full().px(16.0).gap(8.0).child(on).child(field.build(k)))
        .child(buttons)
        .build(k, overlays);
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
        let save: fn(&mut State) = |s| s.naming_branch = true;

        assert!(multi_line_key(&mut state, &press(Mods::default()), text, save));
        assert_eq!((state.draft_text.text.as_str(), state.naming_branch), ("one\n", false));

        assert!(multi_line_key(&mut state, &press(Mods { ctrl: true, cmd: true, ..Mods::default() }), text, save));
        assert_eq!((state.draft_text.text.as_str(), state.naming_branch), ("one\n", true));
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
