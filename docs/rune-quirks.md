# Rune quirks

Traps in Rune that reading our code won't show. Read before UI work.

## Long text

Paths, branch names, commit subjects, comments and errors can all be long, and a bare `text()` overflows its container.

- Use `.wrap_anywhere()` for text people read in full. `.wrap()` only breaks at spaces, so paths and URLs overflow.
- Inside a row, wrapped or truncated text also needs `.shrink().min_w(0.0)`.
- Truncating isn't enough when two texts share a row: one can be squeezed to a lone "…". Cap the less important one with `.max_w()`.
- Cut paths from the start, so the file name survives.
- `Dialog` descriptions never wrap. Variable text goes in the body.
- `TreeItem` truncates its label, not its `.trailing`.

Check with a long string without spaces, in a narrow window.

## Text fields

- `TextArea` has a fixed width until `.width(None)`.
- `TextEdit::focused` is ours to manage: the field taking focus must clear the other's.
- Ctrl reads as Cmd off macOS, so Ctrl+Backspace takes the whole line unless remapped.
- A multi-line field without `on_submit` drops plain Enter, looking for a default button. Only Shift+Enter makes a newline, so a field where Enter should do that has to remap it.

## Dialogs

- A dialog opens focused on its close button, so Enter dismisses it.
- Short dialogs need `.fit()`, or they stay at the standard height.

## The diff

- `scroll_to_line` moves the line even when it's on screen, so don't call it for something done in place.
- Rows holding annotations rebuild every commit. Only adding or removing an annotation needs `set_annotations`.
- `set_files` collapses folds and clears the selection.
- Folds open only for diffs built from two texts, not parsed patches, which is what we use.
- Rows are keyed by file position and line number, and scrolling holds on to a key. So files changing position move the reader to another file, and nothing public says which row is on top. The best we can do is put them back at the start of their file, with `visible_file` and `scroll_to_file`. Peter doesn't want a Rune issue for this while trying the library.

## Components

- `SegmentedControl` makes every segment as wide as the widest, so one long label costs six times over.
- `List` cuts long labels at the end, so give it the file name rather than a path.

## Headless scripts

- A `key` matches the end of an element's path, and `--tree` shows paths. Elements without a key go by position: `#0`, `#1`.
- Key names are capitalised: `Enter`, `Escape`.
- The diff's "+" only takes a press once its row is hovered, so `move` there in an earlier step.
- `scroll` needs an `x` and `y` over the view, and a negative `dy` scrolls down.
- The script's clock runs about twice as fast as real time, which matters when changing files from outside during a run.
