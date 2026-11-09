//! The diff around a comment's lines, kept with the comment to show what it was written on.

use rune_kit::diff::{FileDiff, FileStatus, Hunk, Line, LineKind, Side, parse};
use serde::{Deserialize, Serialize};

use crate::comments::{Anchor, Lines};

/// Lines kept above and below the lines commented on.
const CONTEXT: usize = 3;
/// Lines a comment on a whole file keeps from the start of the file's diff.
const FILE_LINES: usize = 2 * CONTEXT + 1;

/// A piece of a diff, and where in it a comment goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snippet {
    /// A patch of one file with one hunk.
    pub patch: String,
    /// The line, on its anchor's side, the comment goes under.
    pub line: u32,
}

impl Snippet {
    /// The lines `anchor` is on in `file`, with the lines around them. A whole file keeps the
    /// start of its first hunk.
    pub fn new(file: &FileDiff, anchor: &Anchor) -> Option<Snippet> {
        let side = anchor.side;
        let (rows, line) = match &anchor.lines {
            None => {
                let rows: Vec<Row> = file.hunks.first()?.lines.iter().take(FILE_LINES).map(Row::from).collect();
                let line = rows.iter().rev().find_map(|row| row.number(side))?;
                (rows, line)
            }
            Some(lines) => runs(file).into_iter().find_map(|run| {
                let at = |line: u32| run.iter().position(|row| row.number(side) == Some(line));
                let (first, last) = (at(lines.start)?, at(lines.end)?);
                let around = first.saturating_sub(CONTEXT)..(last + CONTEXT + 1).min(run.len());
                Some((run[around].to_vec(), lines.end))
            })?,
        };
        Some(Snippet { patch: header(file) + &hunk(&rows), line })
    }

    /// The quoted lines of `anchor` alone, as unchanged lines: all there is of a comment kept
    /// before comments kept snippets.
    pub fn quoted(anchor: &Anchor) -> Option<Snippet> {
        let Lines { start, end, quoted } = anchor.lines.as_ref()?;
        let rows: Vec<Row> = (*start..).zip(quoted).map(|(line, text)| Row { kind: LineKind::Context, old: Some(line), new: Some(line), text, no_newline: false }).collect();
        let path = &anchor.path;
        Some(Snippet { patch: format!("diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n") + &hunk(&rows), line: *end })
    }

    /// The snippet as a file of a diff.
    pub fn file(&self) -> Option<FileDiff> {
        parse(&self.patch).into_iter().next()
    }
}

/// A line of a diff, as a snippet takes it.
#[derive(Clone, Copy)]
struct Row<'a> {
    kind: LineKind,
    old: Option<u32>,
    new: Option<u32>,
    text: &'a str,
    no_newline: bool,
}

impl Row<'_> {
    fn number(&self, side: Side) -> Option<u32> {
        match side {
            Side::Old => self.old,
            Side::New => self.new,
        }
    }
}

impl<'a> From<&'a Line> for Row<'a> {
    fn from(line: &'a Line) -> Row<'a> {
        Row { kind: line.kind, old: line.old, new: line.new, text: &line.text, no_newline: line.no_newline }
    }
}

/// The lines of `file` in runs without a break: one run of every line when the diff holds the
/// file's whole text, else one per hunk.
fn runs<'a>(file: &'a FileDiff) -> Vec<Vec<Row<'a>>> {
    let Some(text) = file.text(Side::New) else {
        return file.hunks.iter().map(|hunk| hunk.lines.iter().map(Row::from).collect()).collect();
    };
    let unchanged: Vec<&str> = text.lines().collect();
    let mut run = Vec::new();
    // The next line on each side.
    let (mut old, mut new) = (1, 1);
    let fill_to = |run: &mut Vec<Row<'a>>, old: &mut u32, new: &mut u32, end: u32| {
        while *new < end
            && let Some(text) = unchanged.get(*new as usize - 1)
        {
            run.push(Row { kind: LineKind::Context, old: Some(*old), new: Some(*new), text, no_newline: false });
            (*old, *new) = (*old + 1, *new + 1);
        }
    };
    for hunk in &file.hunks {
        fill_to(&mut run, &mut old, &mut new, first(hunk, Side::New));
        run.extend(hunk.lines.iter().map(Row::from));
        old = first(hunk, Side::Old) + hunk.old_count;
        new = first(hunk, Side::New) + hunk.new_count;
    }
    fill_to(&mut run, &mut old, &mut new, u32::MAX);
    vec![run]
}

/// The first line `hunk` covers on `side`, or the line after it when it covers none there.
fn first(hunk: &Hunk, side: Side) -> u32 {
    let (start, count) = match side {
        Side::Old => (hunk.old_start, hunk.old_count),
        Side::New => (hunk.new_start, hunk.new_count),
    };
    if count == 0 { start + 1 } else { start.max(1) }
}

/// The lines of a patch before its hunks, saying what happened to `file`.
fn header(file: &FileDiff) -> String {
    let path = file.path();
    let (old, new) = (file.old_path.as_deref(), file.new_path.as_deref());
    let mut header = format!("diff --git a/{} b/{}\n", old.unwrap_or(path), new.unwrap_or(path));
    match (file.status, old, new) {
        (FileStatus::Added, ..) => header.push_str("new file mode 100644\n"),
        (FileStatus::Deleted, ..) => header.push_str("deleted file mode 100644\n"),
        (FileStatus::Renamed, Some(old), Some(new)) => header.push_str(&format!("rename from {old}\nrename to {new}\n")),
        _ => {}
    }
    let named = |path: Option<&str>, prefix: &str| path.map_or_else(|| "/dev/null".to_owned(), |path| format!("{prefix}/{path}"));
    header + &format!("--- {}\n+++ {}\n", named(old, "a"), named(new, "b"))
}

/// `rows` as one hunk of a patch.
fn hunk(rows: &[Row]) -> String {
    let range = |side: Side| {
        let numbers = rows.iter().filter_map(|row| row.number(side));
        let (start, count) = (numbers.clone().next().unwrap_or(0), numbers.count());
        format!("{start},{count}")
    };
    let mut hunk = format!("@@ -{} +{} @@\n", range(Side::Old), range(Side::New));
    for row in rows {
        let marker = match row.kind {
            LineKind::Context => ' ',
            LineKind::Removed => '-',
            LineKind::Added => '+',
        };
        hunk.push_str(&format!("{marker}{}\n", row.text));
        if row.no_newline {
            hunk.push_str("\\ No newline at end of file\n");
        }
    }
    hunk
}

#[cfg(test)]
mod tests {
    use rune_kit::diff::LineRange;
    use rune_kit::diff::LineRef;

    use super::*;

    /// The lines of `snippet`'s file, as a patch writes them.
    fn shown(snippet: &Snippet) -> Vec<String> {
        let file = snippet.file().unwrap();
        let marker = |kind| match kind {
            LineKind::Context => ' ',
            LineKind::Removed => '-',
            LineKind::Added => '+',
        };
        file.hunks.iter().flat_map(|hunk| &hunk.lines).map(|line| format!("{}{}", marker(line.kind), line.text)).collect()
    }

    /// `f.txt`, twenty numbered lines with the tenth changed, folded as a review folds it.
    fn folded() -> FileDiff {
        let lines: String = (1..=20).map(|n| if n == 10 { "-10\n+ten\n".to_owned() } else { format!(" {n}\n") }).collect();
        crate::review::files(&format!("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,20 +1,20 @@\n{lines}")).remove(0)
    }

    fn on_lines(file: &FileDiff, start: u32, end: u32) -> Anchor {
        let at = |line| LineRef::new(Side::New, line);
        Anchor::on_lines(file, LineRange { file: 0, start: at(start), end: at(end) }).unwrap()
    }

    #[test]
    fn keeps_the_lines_commented_on_with_three_rows_around_them() {
        let file = folded();
        let snippet = Snippet::new(&file, &on_lines(&file, 10, 10)).unwrap();
        assert_eq!(shown(&snippet), [" 8", " 9", "-10", "+ten", " 11", " 12", " 13"]);
        assert_eq!(snippet.line, 10);
    }

    #[test]
    fn takes_lines_the_diff_folds_away_from_the_whole_text() {
        let file = folded();
        let snippet = Snippet::new(&file, &on_lines(&file, 17, 18)).unwrap();
        assert_eq!(shown(&snippet), [" 14", " 15", " 16", " 17", " 18", " 19", " 20"]);
        assert_eq!(snippet.file().unwrap().hunks[0].new_start, 14);
    }

    #[test]
    fn a_new_file_stays_new_and_a_whole_file_keeps_its_start() {
        let lines: String = (1..=10).map(|n| format!("+{n}\n")).collect();
        let file = crate::review::files(&format!("diff --git a/g.rs b/g.rs\nnew file mode 100644\n--- /dev/null\n+++ b/g.rs\n@@ -0,0 +1,10 @@\n{lines}")).remove(0);
        let snippet = Snippet::new(&file, &Anchor::on_file(&file)).unwrap();
        let shown_file = snippet.file().unwrap();
        assert_eq!((shown_file.status, shown_file.path()), (FileStatus::Added, "g.rs"));
        assert_eq!((shown(&snippet).len(), snippet.line), (FILE_LINES, 7));
    }

    #[test]
    fn a_comment_kept_without_one_shows_its_quote() {
        let anchor = Anchor { path: "f.txt".into(), side: Side::Old, lines: Some(Lines { start: 4, end: 5, quoted: vec!["a".into(), "b".into()] }) };
        let snippet = Snippet::quoted(&anchor).unwrap();
        assert_eq!((shown(&snippet), snippet.line), (vec![" a".to_owned(), " b".to_owned()], 5));
    }
}
