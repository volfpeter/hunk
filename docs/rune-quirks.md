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

## Dialogs

- A dialog opens focused on its close button, so Enter dismisses it.
- Short dialogs need `.fit()`, or they stay at the standard height.

## The diff

- `scroll_to_line` moves the line even when it's on screen, so don't call it for something done in place.
- Rows holding annotations rebuild every commit. Only adding or removing an annotation needs `set_annotations`.
- `set_files` collapses folds and clears the selection.
- Folds open only for diffs built from two texts, not parsed patches, which is what we use.

## Headless scripts

- A `key` matches the end of an element's path, and `--tree` shows paths. Elements without a key go by position: `#0`, `#1`.
- Key names are capitalised: `Enter`, `Escape`.
- The diff's "+" only takes a press once its row is hovered, so `move` there in an earlier step.
