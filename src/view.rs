//! The window: a bar to pick what to review, the lists behind that choice, and the diff.

use rune::{Cx, El, Tx, col, row, text};
use rune_kit::diff::{DiffMode, DiffView};
use rune_kit::{Button, ButtonVariant, FileNode, Kit, SegmentedControl, Tree, TreeItem};

use crate::app::{Kind, State};
use crate::overview::Entry;

const BAR_HEIGHT: f32 = 48.0;
const PANEL_HEIGHT: f32 = 200.0;

pub fn view(s: &State, cx: &mut Cx<State>) -> El {
    let k = Kit::neutral(cx.system_dark());
    col().w_full().h_full().fill(k.c.bg_base).child(bar(s, &k)).child(panel(s, &k)).child(main(s, &k))
}

fn bar(s: &State, k: &Kit) -> El {
    let labels = Kind::ALL.map(Kind::label);
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
        .on_select(|s: &mut State, _, _| s.load_review())
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
    if s.diff.files().is_empty() {
        return area.center().child(text("Nothing to review").font(k.ty.ui).color(k.c.text_muted));
    }
    area.child(DiffView::new(|s: &State| &s.diff, |s: &mut State| &mut s.diff).on_visible_file(State::follow_visible_file).build(s, k))
}
