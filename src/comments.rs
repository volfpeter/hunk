//! Review comments: what they point at, how they follow the code, where they are kept and the
//! prompt they become.

use std::fs;
use std::io;
use std::mem;
use std::path::{Path, PathBuf};

use rune::{Latest, Waker};
use rune_kit::diff::{FileDiff, Hunk, Line, LineRange, LineRef, Side};
use serde::{Deserialize, Serialize};

use crate::git::Repo;
use crate::review::Review;

const FILE_NAME: &str = "comments.json";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
#[serde(remote = "Side", rename_all = "lowercase")]
enum SideName {
    Old,
    New,
}

/// A run of lines and the text they held, which is what finds them again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lines {
    /// 1-based.
    pub start: u32,
    /// 1-based, inclusive.
    pub end: u32,
    pub quoted: Vec<String>,
}

/// What a comment is about: lines of a file, or all of it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    /// The file's path on `side`.
    pub path: String,
    #[serde(with = "SideName")]
    pub side: Side,
    /// `None` for the whole file.
    pub lines: Option<Lines>,
}

impl Anchor {
    /// The lines of `range` on the side it started on. Without the file's whole text, a range
    /// running out of its hunk shrinks to its first line, since what lies between is unknown.
    pub fn on_lines(file: &FileDiff, range: LineRange) -> Option<Anchor> {
        let side = range.start.side;
        let numbered = within_hunk(file, range)
            .or_else(|| within_text(file, range))
            .or_else(|| within_hunk(file, LineRange { end: range.start, ..range }))?;
        let path = match side {
            Side::Old => file.old_path.as_deref(),
            Side::New => file.new_path.as_deref(),
        };
        Some(Anchor {
            path: path.unwrap_or(file.path()).to_owned(),
            side,
            lines: Some(Lines {
                start: numbered.first()?.0,
                end: numbered.last()?.0,
                quoted: numbered.iter().map(|(_, text)| (*text).to_owned()).collect(),
            }),
        })
    }

    /// All of `file`, on the side that still has lines.
    pub fn on_file(file: &FileDiff) -> Anchor {
        let has_new_lines = file.hunks.iter().any(|hunk| !numbered(&hunk.lines, Side::New).is_empty());
        let side = if has_new_lines { Side::New } else { Side::Old };
        Anchor { path: file.path().to_owned(), side, lines: None }
    }

    /// Where this shows in `files`: the file's index and the line it goes under, when they
    /// hold its text. A whole file goes under the first line its hunks show.
    pub fn position(&self, files: &[FileDiff]) -> Option<(usize, LineRef)> {
        let index = files.iter().position(|file| file.path() == self.path || file.old_path.as_deref() == Some(&self.path))?;
        let file = &files[index];
        let hunks = file.hunks.iter().map(|hunk| numbered(&hunk.lines, self.side));
        let line = match &self.lines {
            None => hunks.flatten().next()?.0,
            Some(lines) => {
                let runs: Vec<_> = match file.text(self.side) {
                    Some(text) => vec![numbered_text(text)],
                    None => hunks.collect(),
                };
                let start = runs.iter().filter_map(|run| nearest(run, &lines.quoted, lines.start)).min_by_key(|start| start.abs_diff(lines.start))?;
                start + lines.quoted.len() as u32 - 1
            }
        };
        Some((index, LineRef::new(self.side, line)))
    }

    /// Moves onto this anchor's text in `contents`, the file as it stands now. Returns whether
    /// the text is still there; lines stay where they were when it isn't.
    pub fn follow(&mut self, contents: Option<&str>) -> bool {
        let Some(contents) = contents else { return false };
        let Some(lines) = &mut self.lines else { return true };
        let Some(start) = nearest(&numbered_text(contents), &lines.quoted, lines.start) else { return false };
        lines.start = start;
        lines.end = start + lines.quoted.len() as u32 - 1;
        true
    }

    /// `path:12`, `path:12-14`, or the path for a whole file.
    pub fn location(&self) -> String {
        match &self.lines {
            None => self.path.clone(),
            Some(Lines { start, end, .. }) if start == end => format!("{}:{start}", self.path),
            Some(Lines { start, end, .. }) => format!("{}:{start}-{end}", self.path),
        }
    }
}

/// The lines of `lines` that exist on `side`, with their numbers.
fn numbered(lines: &[Line], side: Side) -> Vec<(u32, &str)> {
    lines.iter().filter_map(|line| Some((line.number(side)?, line.text.as_str()))).collect()
}

/// The lines of `text`, numbered from 1.
fn numbered_text(text: &str) -> Vec<(u32, &str)> {
    (1..).zip(text.lines()).collect()
}

/// The lines of `range` on its start's side, when one hunk holds both its ends.
fn within_hunk(file: &FileDiff, range: LineRange) -> Option<Vec<(u32, &str)>> {
    let position = |hunk: &Hunk, at: LineRef| hunk.lines.iter().position(|line| line.number(at.side) == Some(at.line));
    file.hunks.iter().find_map(|hunk| {
        let (first, last) = (position(hunk, range.start)?, position(hunk, range.end)?);
        Some(numbered(&hunk.lines[first.min(last)..=first.max(last)], range.start.side))
    })
}

/// The lines of `range` in the whole text of its start's side, when the diff has it. A range
/// ending on the other side, which numbers lines differently, shrinks to its first line.
fn within_text(file: &FileDiff, range: LineRange) -> Option<Vec<(u32, &str)>> {
    let side = range.start.side;
    let end = if range.end.side == side { range.end.line } else { range.start.line };
    let (first, last) = (range.start.line.min(end), range.start.line.max(end));
    numbered_text(file.text(side)?).get(first.checked_sub(1)? as usize..last as usize).map(<[_]>::to_vec)
}

/// The first line of the run of `lines` matching `quoted` closest to line `near`.
fn nearest(lines: &[(u32, &str)], quoted: &[String], near: u32) -> Option<u32> {
    lines
        .windows(quoted.len())
        .filter(|window| window.iter().zip(quoted).all(|((_, text), quote)| text == quote))
        .map(|window| window[0].0)
        .min_by_key(|start| start.abs_diff(near))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub id: u64,
    pub anchor: Anchor,
    /// The review it was written in, whose side `anchor` points at.
    pub review: Review,
    /// The commit under review when it was written.
    pub commit: Option<String>,
    pub body: String,
    pub resolved: bool,
    /// The code it is about is gone.
    pub outdated: bool,
}

impl Comment {
    /// What sets it apart from an open comment whose code is there. A resolved comment's code
    /// isn't looked for, so its `outdated` says nothing.
    pub fn status(&self) -> Option<&'static str> {
        match (self.resolved, self.outdated) {
            (true, _) => Some("Resolved"),
            (false, true) => Some("Outdated"),
            (false, false) => None,
        }
    }

    /// Sorts by file, a whole-file comment before its lines, then line, then age.
    fn review_order(&self) -> (&str, u32, u64) {
        (&self.anchor.path, self.anchor.lines.as_ref().map_or(0, |lines| lines.start), self.id)
    }
}

/// `comments` as one prompt: a block per comment, each saying where it points, quoting the code
/// and giving the comment.
pub fn prompt(comments: &[&Comment]) -> String {
    comments.iter().map(|comment| prompt_block(comment)).collect::<Vec<_>>().join("\n")
}

fn prompt_block(comment: &Comment) -> String {
    let anchor = &comment.anchor;
    let mut labels = Vec::new();
    if anchor.lines.is_none() {
        labels.push("(file)");
    }
    if anchor.side == Side::Old {
        labels.push("(removed)");
    }
    if comment.outdated {
        labels.push("(outdated)");
    }
    let location = anchor.location();
    labels.push(&location);
    let quote = anchor.lines.iter().flat_map(|lines| &lines.quoted).map(|line| format!("> {line}").trim_end().to_owned()).collect::<Vec<_>>();
    let quote = if quote.is_empty() { String::new() } else { format!("{}\n\n", quote.join("\n")) };
    format!("{}\n\n{quote}{}\n\n---\n", labels.join(" "), comment.body)
}

#[derive(Serialize, Deserialize)]
struct StoreFile {
    version: u32,
    comments: Vec<Comment>,
}

/// The comments in `file`; none when it doesn't exist yet.
fn read_file(file: &Path) -> Result<Vec<Comment>, String> {
    let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", file.display());
    let text = match fs::read_to_string(file) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(describe(&error)),
    };
    let stored: StoreFile = serde_json::from_str(&text).map_err(|error| describe(&error))?;
    if stored.version != VERSION {
        return Err(describe(&format!("unknown version {}", stored.version)));
    }
    Ok(stored.comments)
}

/// Replaces `file` in one step, so a crash leaves the old comments or the new, never half.
fn write_file(file: &Path, comments: Vec<Comment>) -> Result<(), String> {
    let describe = |error: &dyn std::fmt::Display| format!("{}: {error}", file.display());
    let text = serde_json::to_string_pretty(&StoreFile { version: VERSION, comments }).map_err(|error| describe(&error))?;
    let partial = file.with_extension("json.tmp");
    let parent = file.parent().ok_or_else(|| describe(&"no parent folder"))?;
    fs::create_dir_all(parent).and_then(|()| fs::write(&partial, text)).and_then(|()| fs::rename(&partial, file)).map_err(|error| describe(&error))
}

/// The open `comments`, moved onto where their code is now.
fn followed(repo: &Repo, mut comments: Vec<Comment>) -> Vec<Comment> {
    for comment in &mut comments {
        let contents = comment.review.file(repo, comment.anchor.side, &comment.anchor.path);
        comment.outdated = !comment.anchor.follow(contents.as_deref());
    }
    comments
}

/// A repository's comments, kept in its git folder. Writes happen in the background, one at a
/// time, so the file always ends up with the latest state. The default has none and keeps none.
#[derive(Default)]
pub struct Comments {
    /// Where they are kept: `None` when the file holds something unreadable, which saving would
    /// destroy.
    file: Option<PathBuf>,
    all: Vec<Comment>,
    /// Why comments can't be read or kept.
    pub error: Option<String>,
    saving: bool,
    unsaved: bool,
    follow_load: Latest<Vec<Comment>>,
    save_load: Latest<Result<(), String>>,
}

impl Comments {
    /// Reads the repository's comments.
    pub fn load(repo: &Repo) -> Comments {
        let file = repo.state_dir().join(FILE_NAME);
        let read = read_file(&file);
        Comments {
            file: read.is_ok().then_some(file),
            error: read.as_ref().err().cloned(),
            all: read.unwrap_or_default(),
            ..Comments::default()
        }
    }

    pub fn set_waker(&mut self, waker: Waker) {
        self.follow_load.set_waker(waker.clone());
        self.save_load.set_waker(waker);
    }

    /// Lands finished work; returns whether any did.
    pub fn poll(&mut self) -> bool {
        let saved = self.save_load.take();
        let followed = self.follow_load.take();
        let landed = saved.is_some() || followed.is_some();
        if let Some(saved) = saved {
            self.saving = false;
            self.error = saved.err();
            if mem::take(&mut self.unsaved) {
                self.save();
            }
        }
        if let Some(followed) = followed {
            self.apply_followed(followed);
        }
        landed
    }

    /// Comments not resolved yet, in review order.
    pub fn open(&self) -> Vec<&Comment> {
        let mut open: Vec<&Comment> = self.all.iter().filter(|comment| !comment.resolved).collect();
        open.sort_by_key(|comment| comment.review_order());
        open
    }

    /// Every comment: the open ones, then the resolved ones, each in review order.
    pub fn listed(&self) -> Vec<&Comment> {
        let mut listed: Vec<&Comment> = self.all.iter().collect();
        listed.sort_by_key(|comment| (comment.resolved, comment.review_order()));
        listed
    }

    pub fn get(&self, id: u64) -> Option<&Comment> {
        self.all.iter().find(|comment| comment.id == id)
    }

    pub fn add(&mut self, anchor: Anchor, review: Review, commit: Option<String>, body: String) {
        let id = self.all.iter().map(|comment| comment.id + 1).max().unwrap_or(1);
        self.all.push(Comment { id, anchor, review, commit, body, resolved: false, outdated: false });
        self.save();
    }

    pub fn set_resolved(&mut self, id: u64, resolved: bool) {
        if let Some(comment) = self.all.iter_mut().find(|comment| comment.id == id) {
            comment.resolved = resolved;
            self.save();
        }
    }

    /// Rewords the comment; what it points at stays.
    pub fn edit(&mut self, id: u64, body: String) {
        if let Some(comment) = self.all.iter_mut().find(|comment| comment.id == id) {
            comment.body = body;
            self.save();
        }
    }

    pub fn delete(&mut self, id: u64) {
        self.all.retain(|comment| comment.id != id);
        self.save();
    }

    /// Looks for every open comment's code in `repo` again, in the background.
    pub fn follow_code(&mut self, repo: &Repo) {
        let open = self.open().into_iter().cloned().collect();
        self.follow_load.request((repo.clone(), open), |(repo, open)| followed(&repo, open));
    }

    fn apply_followed(&mut self, followed: Vec<Comment>) {
        let mut changed = false;
        for new in followed {
            if let Some(comment) = self.all.iter_mut().find(|comment| comment.id == new.id)
                && (comment.anchor != new.anchor || comment.outdated != new.outdated)
            {
                comment.anchor = new.anchor;
                comment.outdated = new.outdated;
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }

    fn save(&mut self) {
        let Some(file) = &self.file else { return };
        if self.saving {
            self.unsaved = true;
            return;
        }
        self.saving = true;
        self.save_load.request((file.clone(), self.all.clone()), |(file, all)| write_file(&file, all));
    }
}

#[cfg(test)]
mod tests {
    use rune_kit::diff::parse;
    use tempfile::TempDir;

    use super::*;
    use crate::git::testing::Fixture;

    const PATCH: &str = "diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,3 +1,4 @@\n a\n-b\n+B\n+C\n d\n@@ -20,2 +21,2 @@\n x\n-y\n+z\n";

    fn range(start: LineRef, end: LineRef) -> LineRange {
        LineRange { file: 0, start, end }
    }

    fn new_line(line: u32) -> LineRef {
        LineRef::new(Side::New, line)
    }

    fn anchor(start: u32, quoted: &[&str]) -> Anchor {
        let end = start + quoted.len() as u32 - 1;
        Anchor { path: "f.txt".into(), side: Side::New, lines: Some(Lines { start, end, quoted: quoted.iter().map(|line| (*line).to_owned()).collect() }) }
    }

    fn comment(id: u64, anchor: Anchor, body: &str) -> Comment {
        Comment { id, anchor, review: Review::Changes, commit: None, body: body.into(), resolved: false, outdated: false }
    }

    #[test]
    fn quotes_the_selected_lines_of_one_side() {
        let files = parse(PATCH);
        assert_eq!(Anchor::on_lines(&files[0], range(new_line(2), new_line(3))), Some(anchor(2, &["B", "C"])));
        // Either way round.
        assert_eq!(Anchor::on_lines(&files[0], range(new_line(3), new_line(2))), Some(anchor(2, &["B", "C"])));
    }

    #[test]
    fn a_selection_across_sides_quotes_the_side_it_started_on() {
        let files = parse(PATCH);
        let lines = Anchor::on_lines(&files[0], range(LineRef::new(Side::Old, 1), new_line(3))).unwrap();
        assert_eq!((lines.side, lines.lines.unwrap().quoted), (Side::Old, vec!["a".to_owned(), "b".to_owned()]));
    }

    #[test]
    fn a_selection_across_hunks_shrinks_to_its_first_line() {
        let files = parse(PATCH);
        assert_eq!(Anchor::on_lines(&files[0], range(new_line(3), new_line(22))), Some(anchor(3, &["C"])));
    }

    #[test]
    fn follows_quoted_text_and_notices_when_it_is_gone() {
        let mut moved = anchor(2, &["b", "c"]);
        assert!(moved.follow(Some("new\nnew\na\nb\nc\n")));
        assert_eq!(moved, anchor(4, &["b", "c"]));

        let mut gone = anchor(2, &["b", "c"]);
        assert!(!gone.follow(Some("a\nb\nx\nc\n")));
        assert_eq!(gone, anchor(2, &["b", "c"]));

        assert!(!anchor(1, &["a"]).follow(None));
    }

    #[test]
    fn follows_to_the_copy_nearest_where_it_was() {
        let mut anchor_at_six = anchor(6, &["x"]);
        assert!(anchor_at_six.follow(Some("x\na\nb\nc\nd\ne\nx\n")));
        assert_eq!(anchor_at_six, anchor(7, &["x"]));
    }

    #[test]
    fn a_whole_file_only_needs_the_file() {
        let mut whole = Anchor { path: "f.txt".into(), side: Side::New, lines: None };
        assert!(whole.follow(Some("anything\n")));
        assert!(!whole.follow(None));
    }

    #[test]
    fn shows_under_the_last_line_of_its_text_wherever_a_review_has_it() {
        let files = parse(PATCH);
        assert_eq!(anchor(40, &["B", "C"]).position(&files), Some((0, new_line(3))));
        assert_eq!(anchor(2, &["not in the diff"]).position(&files), None);
    }

    #[test]
    fn does_not_match_text_split_over_two_hunks() {
        assert_eq!(anchor(4, &["d", "x"]).position(&parse(PATCH)), None);
    }

    /// `f.txt`, twelve numbered lines with the second changed, folded so lines 6 to 12 are hidden.
    fn folded() -> Vec<FileDiff> {
        let lines: String = (1..=12).map(|n| if n == 2 { "-2\n+two\n".to_owned() } else { format!(" {n}\n") }).collect();
        let files = crate::review::files(&format!("diff --git a/f.txt b/f.txt\n--- a/f.txt\n+++ b/f.txt\n@@ -1,12 +1,12 @@\n{lines}"));
        assert_eq!(files[0].hunks.iter().map(|hunk| (hunk.new_start, hunk.new_count)).collect::<Vec<_>>(), [(1, 5)]);
        files
    }

    #[test]
    fn quotes_lines_outside_the_hunks_from_the_whole_text() {
        let files = folded();
        assert_eq!(Anchor::on_lines(&files[0], range(new_line(9), new_line(10))), Some(anchor(9, &["9", "10"])));
        // From a change into the fold.
        assert_eq!(Anchor::on_lines(&files[0], range(new_line(2), new_line(7))), Some(anchor(2, &["two", "3", "4", "5", "6", "7"])));
    }

    #[test]
    fn shows_under_text_the_diff_folds_away() {
        assert_eq!(anchor(9, &["9", "10"]).position(&folded()), Some((0, new_line(10))));
    }

    #[test]
    fn a_whole_file_shows_under_its_first_line_and_a_deleted_one_on_the_old_side() {
        let files = parse(PATCH);
        assert_eq!(Anchor::on_file(&files[0]).position(&files), Some((0, new_line(1))));

        let deleted = parse("diff --git a/g b/g\ndeleted file mode 100644\n--- a/g\n+++ /dev/null\n@@ -1 +0,0 @@\n-gone\n");
        let anchor = Anchor::on_file(&deleted[0]);
        assert_eq!((anchor.path.as_str(), anchor.side), ("g", Side::Old));
        assert_eq!(anchor.position(&deleted), Some((0, LineRef::new(Side::Old, 1))));
    }

    #[test]
    fn the_old_side_of_a_rename_keeps_the_old_path() {
        let files = parse("diff --git a/old.txt b/new.txt\nsimilarity index 90%\nrename from old.txt\nrename to new.txt\n--- a/old.txt\n+++ b/new.txt\n@@ -1 +1 @@\n-a\n+b\n");
        let anchor = Anchor::on_lines(&files[0], LineRange::line(0, LineRef::new(Side::Old, 1))).unwrap();
        assert_eq!(anchor.path, "old.txt");
        assert_eq!(anchor.position(&files), Some((0, LineRef::new(Side::Old, 1))));
    }

    #[test]
    fn prompt_says_where_quotes_the_code_and_gives_the_comment() {
        let removed = Anchor { side: Side::Old, ..anchor(3, &["old", ""]) };
        let whole = Anchor { path: "f.txt".into(), side: Side::New, lines: None };
        let outdated = Comment { outdated: true, ..comment(3, anchor(9, &["gone"]), "Was this right?") };
        let (removed, whole) = (comment(1, removed, "Why remove this?"), comment(2, whole, "Split this file."));
        let expected = "\
(file) f.txt

Split this file.

---

(removed) f.txt:3-4

> old
>

Why remove this?

---

(outdated) f.txt:9

> gone

Was this right?

---
";
        assert_eq!(prompt(&[&whole, &removed, &outdated]), expected);
    }

    #[test]
    fn keeps_comments_across_restarts() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("diffactive").join(FILE_NAME);
        assert_eq!(read_file(&file), Ok(Vec::new()));

        let comments = vec![comment(1, anchor(2, &["b"]), "Fix this.")];
        write_file(&file, comments.clone()).unwrap();
        assert_eq!(read_file(&file), Ok(comments));
    }

    #[test]
    fn leaves_a_file_it_cannot_read_alone() {
        let fixture = Fixture::new();
        let file = fixture.repo.state_dir().join(FILE_NAME);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "not json").unwrap();

        let mut comments = Comments::load(&fixture.repo);
        assert!(comments.error.is_some());
        comments.add(anchor(1, &["a"]), Review::Changes, None, "Lost for now".into());
        assert_eq!(fs::read_to_string(&file).unwrap(), "not json");
    }

    #[test]
    fn resolved_comments_list_after_open_ones_and_leave_the_prompt_until_reopened() {
        let fixture = Fixture::new();
        let mut comments = Comments::load(&fixture.repo);
        comments.add(anchor(1, &["a"]), Review::Changes, None, "First".into());
        comments.add(anchor(2, &["b"]), Review::Changes, None, "Second".into());
        let ids = |comments: &[&Comment]| comments.iter().map(|comment| comment.id).collect::<Vec<_>>();

        comments.set_resolved(1, true);
        assert_eq!((ids(&comments.listed()), ids(&comments.open())), (vec![2, 1], vec![2]));

        comments.set_resolved(1, false);
        assert_eq!((ids(&comments.listed()), ids(&comments.open())), (vec![1, 2], vec![1, 2]));
    }

    #[test]
    fn code_found_before_an_edit_or_delete_does_not_undo_it() {
        let fixture = Fixture::new();
        let mut comments = Comments::load(&fixture.repo);
        comments.add(anchor(1, &["a"]), Review::Changes, None, "Before".into());
        comments.add(anchor(2, &["b"]), Review::Changes, None, "Doomed".into());
        let found: Vec<Comment> = comments.all.iter().cloned().map(|comment| Comment { outdated: true, ..comment }).collect();

        comments.edit(1, "After".into());
        comments.delete(2);
        comments.apply_followed(found);

        assert_eq!(comments.all, vec![Comment { outdated: true, ..comment(1, anchor(1, &["a"]), "After") }]);
    }

    #[test]
    fn a_comment_follows_its_review_as_the_worktree_changes() {
        let fixture = Fixture::new();
        fixture.commit("f.txt", "a\nb\nc\n", "first");
        fixture.write("f.txt", "a\nB\nc\n");
        let open = vec![comment(1, anchor(2, &["B"]), "Why capital?")];

        fixture.write("f.txt", "new\na\nB\nc\n");
        let moved = followed(&fixture.repo, open.clone());
        assert_eq!((&moved[0].anchor, moved[0].outdated), (&anchor(3, &["B"]), false));

        fixture.write("f.txt", "a\nb\nc\n");
        let gone = followed(&fixture.repo, moved);
        assert_eq!((&gone[0].anchor, gone[0].outdated), (&anchor(3, &["B"]), true));

        fixture.write("f.txt", "B\n");
        let back = followed(&fixture.repo, gone);
        assert_eq!((&back[0].anchor, back[0].outdated), (&anchor(1, &["B"]), false));
    }
}
