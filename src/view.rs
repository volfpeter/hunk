//! The window: a bar to pick what to review, the lists behind that choice, and the diff.

use rune::{Cx, El, Tx, col, row, text};
use rune::{Key, KeyEvent, Mods};
use rune_kit::diff::{Annotation, DiffMode, DiffView};
use rune_kit::{Badge, Button, ButtonVariant, ControlSize, FileNode, Kit, SegmentedControl, TextArea, Tree, TreeItem};

use crate::app::{DRAFT_NOTE, Kind, State};
use crate::comments::Comment;
use crate::overview::Entry;

const BAR_HEIGHT: f32 = 48.0;
const PANEL_HEIGHT: f32 = 200.0;
const DRAFT_ROWS: (usize, usize) = (3, 12);

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let k = Kit::neutral(cx.system_dark());
    col().w_full().h_full().fill(k.c.bg_base).child(bar(s, &k)).child(panel(s, &k)).child(main(s, &k))
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
        .child(text(s.repo_name()).font(k.ty.ui_medium).color(k.c.text_base))
        .child(text(s.overview.head.clone()).font(k.ty.ui).color(k.c.text_muted))
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
    let column = col().key("entries").grow(1.0).min_w(0.0).h_full().padding(8.0);
    if s.list.is_empty() {
        return column.child(text(format!("No {}", s.kind.label().to_lowercase())).font(k.ty.ui).color(k.c.text_muted));
    }
    let (detail_font, detail_color) = (k.ty.text_small, k.c.text_faint);
    let list = Tree::new(|s: &State| &s.list, |s: &mut State| &mut s.list, |entry: &Entry| entry.title.as_str())
        .name(s.kind.label())
        .row(move |entry, _| TreeItem::new(entry.title.clone()).trailing(text(entry.detail.clone()).font(detail_font).color(detail_color)))
        .on_select(|s: &mut State, _, _| s.show_selected())
        .build(s, k)
        .grow(1.0)
        .min_h(0.0);
    column.child(list)
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
        .child(text(title).font(k.ty.ui_medium).color(k.c.text_base))
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
        return area.center().child(text(error.clone()).font(k.ty.ui).color(k.c.text_muted));
    }
    let area = area
        .children(s.comments.error.as_ref().map(|error| comments_error(k, error)))
        .children(s.unplaced_comment().map(|comment| note(k, comment_card(k, comment, true))));
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
    row().w_full().px(16.0).py(8.0).child(text(error.to_owned()).font(k.ty.ui).color(k.c.danger.fg).wrap())
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
        Some(comment) => note(k, comment_card(k, comment, false)),
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
    let field = TextArea::new(&s.draft_text)
        .width(None)
        .auto_grow(DRAFT_ROWS.0, DRAFT_ROWS.1)
        .placeholder("What should change here?")
        .label("Comment")
        .on_edit(|s: &mut State| &mut s.draft_text)
        .on_submit(|s: &mut State, _: &mut Tx| s.submit_draft())
        .on_key(|s: &mut State, event: &KeyEvent, _: &mut Tx| {
            if event.key == Key::Escape {
                s.cancel_draft();
                return true;
            }
            let word = word_chord(event);
            if let Some(word) = &word {
                s.draft_text.key(word, None);
            }
            word.is_some()
        });
    let buttons = row()
        .gap(8.0)
        .items_center()
        .child(Button::new("Comment").disabled(s.draft_text.text.trim().is_empty()).on_press(State::submit_draft).build(k))
        .child(Button::new("Cancel").variant(ButtonVariant::Ghost).on_press(State::cancel_draft).build(k))
        .child(text("Enter saves, Shift+Enter starts a new line").font(k.ty.text_small).color(k.c.text_faint));
    col().child(text(location).font(k.ty.ui_medium).color(k.c.text_base)).child(field.build(k)).child(buttons)
}

/// The Alt chord for a Ctrl word chord. Rune reads Ctrl as Cmd off macOS, which takes the whole line.
fn word_chord(event: &KeyEvent) -> Option<KeyEvent> {
    let Mods { ctrl, cmd, shift, .. } = event.mods;
    let by_word = matches!(event.key, Key::Backspace | Key::Left | Key::Right);
    (by_word && ctrl && cmd).then(|| KeyEvent { key: event.key.clone(), mods: Mods { alt: true, shift, ..Mods::default() } })
}

/// A comment, with the code it quotes when `quoted`.
fn comment_card(k: &Kit, comment: &Comment, quoted: bool) -> El {
    let id = comment.id;
    let heading = row()
        .w_full()
        .gap(8.0)
        .items_center()
        .child(text(comment.anchor.location()).font(k.ty.ui_medium).color(k.c.text_base))
        .children(comment.outdated.then(|| Badge::new("Outdated").build(k)))
        .child(row().grow(1.0))
        .child(Button::new("Resolve").variant(ButtonVariant::Ghost).control_size(ControlSize::Small).on_press(move |s: &mut State| s.resolve_comment(id)).build(k));
    let quote = comment.anchor.lines.as_ref().filter(|_| quoted).map(|lines| text(lines.quoted.join("\n")).font(k.ty.mono).color(k.c.text_muted));
    col().child(heading).children(quote).child(text(comment.body.clone()).font(k.ty.body).color(k.c.text_base).wrap())
}

#[cfg(test)]
mod tests {
    use rune::TextEdit;

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
