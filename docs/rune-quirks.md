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
- `rich()` wraps, and `.single_line()` does nothing outside a text field. For coloured text on one line, use `text(..).spans(..)`.
- Syntax highlighting colours only a source's first 128 KiB and 32,768 ranges. Highlight long text in chunks of lines.

Check with a long string without spaces, in a narrow window.

## Text fields

- `TextArea` has a fixed width until `.width(None)`.
- `TextEdit::focused` is ours to manage: the field taking focus must clear the other's.
- Ctrl reads as Cmd off macOS, so Ctrl+Backspace takes the whole line unless remapped.
- A multi-line field without `on_submit` drops plain Enter, looking for a default button. Only Shift+Enter makes a newline, so a field where Enter should do that has to remap it.
- `text_input` has no read-only mode. Putting the text back in `on_input_change` keeps it read-only, with the caret and selection still working.
- A field lays out all of its text on the UI thread whenever it changes, and highlight spans double that. Fine for a message, a hitch for a whole source file: long text goes in virtual rows.

## Dialogs

- A dialog opens focused on its close button, so Enter dismisses it.
- Short dialogs need `.fit()`, or they stay at the standard height.
- A fitted dialog taller than the window runs off both ends, title and close button included. Keep it short, or scroll its body.
- `Dialog` always centres its panel. To place it elsewhere, re-align the layer it pushes last, as the palette does.
- `Dialog` is either 368 points tall or as tall as its content. A panel filling the window is a layer of our own, with `El::modal` as `Dialog` uses.

## Scrolling

- A scroll viewport that unmounts forgets its offset. `on_scroll_offset` reports it but can't change state, so keep it in a `Cell` and put it back with `Tx::scroll_to_instant` when the viewport returns.

## The diff

- `scroll_to_line` moves the line even when it's on screen, so don't call it for something done in place.
- Rows holding annotations rebuild every commit. Only adding or removing an annotation needs `set_annotations`.
- `set_files` collapses folds and clears the selection.
- Folds open only for a diff holding both whole texts. A parsed patch holds them only when git made it with full context and it went through `FileDiff::fold`.
- Opening a fold only shows lines: hunks don't grow, so anything that looks for a line in the hunks misses the revealed ones.
- `scroll_to_line` on a folded line opens its fold but lands short, leaving the line under the sticky file header. Once the fold is open, it lands right.
- A hunk that doesn't start at line 1 gets a gap row above it counting the lines before, even in a diff without the file's text, where it can't open. A snippet of a file shows one.
- Rows are keyed by file position and line number, and scrolling holds on to a key. So files changing position move the reader to another file, and nothing public says which row is on top. The best we can do is put them back at the start of their file, with `visible_file` and `scroll_to_file`. Peter doesn't want a Rune issue for this while trying the library.

## Components

- `SegmentedControl` makes every segment as wide as the widest, so one long label costs six times over.
- `List` cuts long labels at the end, so give it the file name rather than a path.
- `ListItem` has no slot for a shortcut, unlike `MenuItem`, so a key can only go in the label.
- `List`'s highlighted row is a faint fill, hard to see. A list that's driven by the keyboard needs its own rows.

## Headless scripts

- A `key` matches the end of an element's path, and `--tree` shows paths. Elements without a key go by position: `#0`, `#1`.
- Key names are capitalised: `Enter`, `Escape`. Arrows are `Up`, `Down`, `Left` and `Right`.
- Ctrl+letter is `"cmd": true`, as Ctrl reads as Cmd off macOS. With `"ctrl": true`, `is_command` doesn't match.
- The diff's "+" only takes a press once its row is hovered, so `move` there in an earlier step.
- `scroll` needs an `x` and `y` over the view, and a negative `dy` scrolls down.
- The script's clock runs about twice as fast as real time, which matters when changing files from outside during a run.
- `type` enters its text over several frames. A key sent soon after lands mid-text, so leave a gap before Enter.
