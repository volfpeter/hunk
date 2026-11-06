//! The window: a bar to pick what to review, the lists behind that choice, and the diff.

use rune::{Cx, El, TextEdit, Tx, col, row, text};
use rune::{Key, KeyEvent, Mods};
use rune_kit::diff::{Annotation, DiffMode, DiffView};
use rune_kit::{Badge, Button, ButtonVariant, ControlSize, Dialog, FileNode, Kit, SegmentedControl, TextArea, Tree, TreeItem, root};

use crate::app::{DRAFT_NOTE, Kind, State};
use crate::comments::Comment;
use crate::overview::Entry;

const BAR_HEIGHT: f32 = 48.0;
const PANEL_HEIGHT: f32 = 200.0;
const DRAFT_ROWS: (usize, usize) = (3, 12);
const CONFIRM_WIDTH: f32 = 400.0;
/// The most width a list entry's detail takes from its title.
const DETAIL_MAX_WIDTH: f32 = 240.0;
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
    root(col().fill(k.c.bg_base).child(bar(s, &k)).child(panel(s, &k)).child(main(s, &k)), overlays)
}

fn bar(s: &State, k: &Kit) -> El {
    let open = s.comments.open().len();
    let labels = Kind::ALL.map(|kind| match kind {
        Kind::Comments if open > 0 => format!("{} ({open})", kind.label()),
        _ => kind.label().to_owned(),
    });
    let selected = Kind::ALL.iter().position(|kind| *kind == s.kind);
    let kinds = SegmentedControl::new(&labels, selected).fit().on_pick(|s: &mut State, i, _: &mut Tx| s.select_kind(Kind::ALL[i]));

    let split = s.diff.mode() == DiffMode::Split;
    let modes = SegmentedControl::new(&["Unified", "Split"], Some(split as usize)).fit().on_pick(|s: &mut State, i, tx: &mut Tx| {
        tx.motion(rune::tok::motion::SNAPPY);
        s.diff.set_mode(if i == 1 { DiffMode::Split } else { DiffMode::Unified });
    });

    row()
        .key("bar")
        .w_full()
        .h(BAR_HEIGHT)
        .px(16.0)
        .gap(12.0)
        .items_center()
        .fill(k.c.layer_01)
        .child(one_line(s.repo_name()).max_w(HEAD_MAX_WIDTH).font(k.ty.ui_medium).color(k.c.text_base))
        .child(one_line(s.overview.head.clone()).max_w(HEAD_MAX_WIDTH).font(k.ty.ui).color(k.c.text_muted))
        .child(kinds.build(k))
        .child(row().grow(1.0))
        .child(modes.build(k))
        .child(
            Button::new(if s.just_copied() { "Copied" } else { "Copy prompt" })
                .variant(ButtonVariant::Ghost)
                .disabled(open == 0)
                .help("Copy all open comments as a prompt for an agent")
                .on_press_tx(State::copy_prompt)
                .build(k),
        )
        .child(Button::new("Refresh").variant(ButtonVariant::Ghost).on_press(|s: &mut State| s.refresh()).build(k))
}

/// The list to pick from, when the kind has one, beside the files of the review.
fn panel(s: &State, k: &Kit) -> El {
    let panel = row().key("panel").w_full().h(PANEL_HEIGHT).border(1.0, k.c.border_weak);
    if s.kind.has_list() { panel.child(entries(s, k)).child(files(s, k)) } else { panel.child(files(s, k)) }
}

fn entries(s: &State, k: &Kit) -> El {
    let column = col().key("entries").grow(1.0).min_w(0.0).h_full().padding(8.0).gap(4.0);
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
    column.children((s.kind == Kind::Comments).then(|| comments_summary(s, k))).child(list)
}

/// How many comments there are, and what can be done to the selected one.
fn comments_summary(s: &State, k: &Kit) -> El {
    let open = s.comments.open().len();
    let resolved = s.comments.listed().len() - open;
    let actions = s.selected_comment().filter(|comment| s.editing != Some(comment.id));
    row()
        .w_full()
        .gap(12.0)
        .px(8.0)
        .h(24.0)
        .items_center()
        .child(text(format!("{open} open, {resolved} resolved")).font(k.ty.ui).color(k.c.text_muted))
        .child(row().grow(1.0))
        .children(actions.map(|comment| comment_actions(k, comment, comment_button("Edit").on_press_tx(State::edit_selected))))
}

fn files(s: &State, k: &Kit) -> El {
    let (added, removed) = s.diff.files().iter().fold((0, 0), |(a, r), file| (a + file.additions(), r + file.deletions()));
    let count = match s.diff.files().len() {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    };
    let title = s.review().map_or_else(String::new, |review| review.title());
    let summary = row()
        .gap(12.0)
        .px(8.0)
        .h(24.0)
        .items_center()
        .child(one_line(title).font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(format!("{count}  +{added} −{removed}")).font(k.ty.ui).color(k.c.text_muted));
    let tree = Tree::new(|s: &State| &s.files, |s: &mut State| &mut s.files, FileNode::label)
        .name("Changed files")
        .row(|node, row| {
            let item = TreeItem::file(node, row);
            match node.data {
                Some((added, removed)) => item.changes(added, removed),
                None => item,
            }
        })
        .on_select(|s: &mut State, path, tx| s.scroll_to_path(path, tx))
        .build(s, k)
        .grow(1.0)
        .min_h(0.0);
    col().key("files-column").grow(1.0).min_w(0.0).h_full().padding(8.0).gap(4.0).child(summary).child(tree)
}

fn main(s: &State, k: &Kit) -> El {
    let area = col().key("main").w_full().grow(1.0).min_h(0.0);
    if let Some(error) = &s.error {
        return area.center().padding(16.0).child(text(error.clone()).font(k.ty.ui).color(k.c.text_muted).wrap_anywhere());
    }
    let area = area
        .children(s.comments.error.as_ref().map(|error| comments_error(k, error)))
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

fn comments_error(k: &Kit, error: &str) -> El {
    col().w_full().px(16.0).py(8.0).child(text(error.to_owned()).font(k.ty.ui).color(k.c.danger.fg).wrap_anywhere())
}

/// Comments on a whole file go under its first line, which a file without lines lacks.
fn file_actions(s: &State, k: &Kit, file: usize) -> El {
    if s.diff.files()[file].hunks.is_empty() {
        return row();
    }
    Button::new("Comment on file")
        .variant(ButtonVariant::GhostMuted)
        .control_size(ControlSize::Small)
        .on_press_tx(move |s: &mut State, tx: &mut Tx| s.compose_on_file(file, tx))
        .build(k)
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
        .on_submit(move |s: &mut State, _: &mut Tx| save(s))
        .on_key(move |s: &mut State, event: &KeyEvent, _: &mut Tx| {
            if event.key == Key::Escape {
                cancel(s);
                return true;
            }
            let word = word_chord(event);
            if let Some(word) = &word {
                field_text(s).key(word, None);
            }
            word.is_some()
        });
    let buttons = row()
        .gap(8.0)
        .items_center()
        .child(Button::new(save_label).disabled(edit.text.trim().is_empty()).on_press(save).build(k))
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(cancel).build(k))
        .child(text("Enter saves, Shift+Enter starts a new line").font(k.ty.text_small).color(k.c.text_faint));
    col().w_full().gap(8.0).child(field.build(k)).child(buttons)
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
        .children((!editing).then(|| comment_actions(k, comment, comment_button("Edit").on_press(move |s: &mut State| s.start_edit(id)))));
    let quote = comment.anchor.lines.as_ref().filter(|_| quoted).map(|lines| text(lines.quoted.join("\n")).font(k.ty.mono).color(k.c.text_muted).wrap_anywhere());
    let body = if editing { comment_field(k, &s.edit_text, EDITED_COMMENT) } else { text(comment.body.clone()).font(k.ty.body).color(k.c.text_base).wrap_anywhere() };
    col().child(heading).children(quote).child(body)
}

fn comment_button(label: &str) -> Button {
    Button::new(label).variant(ButtonVariant::Ghost).control_size(ControlSize::Small)
}

/// `edit`, then what else can be done to the comment.
fn comment_actions(k: &Kit, comment: &Comment, edit: Button) -> El {
    let id = comment.id;
    let (label, toggle): (_, fn(&mut State, u64)) = if comment.resolved { ("Reopen", State::reopen_comment) } else { ("Resolve", State::resolve_comment) };
    row()
        .gap(4.0)
        .child(edit.build(k))
        .child(comment_button(label).on_press(move |s: &mut State| toggle(s, id)).build(k))
        .child(comment_button("Delete").on_press(move |s: &mut State| s.ask_to_delete(id)).build(k))
}

fn delete_dialog(s: &State, k: &Kit, overlays: &mut Vec<El>) {
    let Some(comment) = s.deleting.and_then(|id| s.comments.get(id)) else { return };
    let location = text(comment.anchor.location()).font(k.ty.ui).color(k.c.text_base).wrap_anywhere();
    let buttons = row()
        .w_full()
        .padding(16.0)
        .gap(8.0)
        .justify_end()
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(State::cancel_delete).build(k))
        .child(Button::new("Delete").variant(ButtonVariant::Danger).on_press(State::confirm_delete).build(k));
    Dialog::new("delete-comment", "Delete comment?")
        .description("This can't be undone.")
        .width(CONFIRM_WIDTH)
        .fit()
        .open(true)
        .on_close(State::cancel_delete)
        .child(col().w_full().px(16.0).child(location))
        .child(buttons)
        .build(k, overlays);
}

#[cfg(test)]
mod tests {
    use super::*;

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
