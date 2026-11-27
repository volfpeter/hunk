//! Finding text in the lines of a diff's hunks.

use rune_kit::diff::{FileDiff, Line, LineKind, LineRange, LineRef, Side};
use rune_kit::search::Fold;

/// Where a line is in the order a diff shows it: its file, hunk, and index in the hunk.
type Place = (usize, usize, usize);

/// A line holding what is looked for.
pub struct Found {
    place: Place,
    pub lines: LineRange,
}

/// Which match to go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The one at the selection, or after it.
    Here,
    Next,
    Previous,
}

/// The hunk lines of `files` holding `query`, in the diff's order, ignoring case and accents
/// the query leaves out. Each is on its new side, unless it was removed.
pub fn find(files: &[FileDiff], query: &str) -> Vec<Found> {
    if query.is_empty() {
        return Vec::new();
    }
    let fold = Fold::default();
    let mut found = Vec::new();
    for (place, line) in lines(files) {
        if !fold.find(&line.text, query).is_empty() {
            let at = match line.kind {
                LineKind::Removed => LineRef::new(Side::Old, line.old.unwrap_or_default()),
                _ => LineRef::new(Side::New, line.new.unwrap_or_default()),
            };
            found.push(Found { place, lines: LineRange::line(place.0, at) });
        }
    }
    found
}

/// The match `step` goes to from where `selection` starts, wrapping round. Without a selection
/// on a hunk line, it goes from the start of `file`.
pub fn go<'a>(found: &'a [Found], files: &[FileDiff], selection: Option<LineRange>, file: usize, step: Step) -> Option<&'a Found> {
    let (from, on_line) = match selection.and_then(|lines| place_of(files, lines)) {
        Some(place) => (place, true),
        None => ((file, 0, 0), false),
    };
    let after = |f: &&Found| if step == Step::Next && on_line { f.place > from } else { f.place >= from };
    match step {
        Step::Previous => found.iter().rev().find(|f| f.place < from).or(found.last()),
        _ => found.iter().find(after).or(found.first()),
    }
}

/// The index among `found` of the match where `selection` starts.
pub fn current(found: &[Found], files: &[FileDiff], selection: Option<LineRange>) -> Option<usize> {
    let place = place_of(files, selection?)?;
    found.iter().position(|f| f.place == place)
}

/// Every line of `files`' hunks, in the diff's order.
fn lines(files: &[FileDiff]) -> impl Iterator<Item = (Place, &Line)> {
    files.iter().enumerate().flat_map(|(index, file)| file_lines(index, file))
}

/// Every line of the hunks of `file`, the diff's file `index`.
fn file_lines(index: usize, file: &FileDiff) -> impl Iterator<Item = (Place, &Line)> {
    file.hunks.iter().enumerate().flat_map(move |(hunk, h)| h.lines.iter().enumerate().map(move |(i, line)| ((index, hunk, i), line)))
}

/// Where the hunk line `lines` starts on is, unless it is in no hunk.
fn place_of(files: &[FileDiff], lines: LineRange) -> Option<Place> {
    let at = lines.start;
    file_lines(lines.file, files.get(lines.file)?).find(|(_, line)| line.number(at.side) == Some(at.line)).map(|(place, _)| place)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two files: `a.rs` changing its first line, its seventh folded away, and `b.rs` changing
    /// its only line.
    fn files() -> Vec<FileDiff> {
        let old_a = "Needle old\nb\nc\nd\ne\nf\nneedle folded\ng\nh\ni\nj\nk\nl\nm\n";
        let new_a = "needle new\nb\nc\nd\ne\nf\nneedle folded\ng\nh\ni\nj\nk\nl\nm\n";
        vec![FileDiff::from_texts("a.rs", old_a, new_a), FileDiff::from_texts("b.rs", "x\n", "a NEEDLE\n")]
    }

    fn on(file: usize, side: Side, line: u32) -> LineRange {
        LineRange::line(file, LineRef::new(side, line))
    }

    fn lines(found: &[Found]) -> Vec<LineRange> {
        found.iter().map(|found| found.lines).collect()
    }

    #[test]
    fn finds_hunk_lines_ignoring_case_on_the_side_they_show() {
        let files = files();
        assert_eq!(lines(&find(&files, "needle")), [on(0, Side::Old, 1), on(0, Side::New, 1), on(1, Side::New, 1)]);
        assert!(find(&files, "").is_empty());
    }

    #[test]
    fn goes_from_the_selection_and_wraps_round() {
        let files = files();
        let found = find(&files, "needle");
        let go = |selection, file, step| go(&found, &files, selection, file, step).map(|found| found.lines);
        let first = Some(on(0, Side::Old, 1));
        assert_eq!(go(first, 0, Step::Here), first);
        assert_eq!(go(first, 0, Step::Next), Some(on(0, Side::New, 1)));
        assert_eq!(go(first, 0, Step::Previous), Some(on(1, Side::New, 1)));
        assert_eq!(go(Some(on(1, Side::New, 1)), 0, Step::Next), first);
        assert_eq!(go(None, 1, Step::Next), Some(on(1, Side::New, 1)));
        assert_eq!(go(None, 1, Step::Previous), Some(on(0, Side::New, 1)));
        assert_eq!(current(&found, &files, Some(on(1, Side::New, 1))), Some(2));
    }
}
