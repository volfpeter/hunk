//! A pull request review made of comments: those GitHub can show on their lines, and a summary
//! holding the rest.

use rune_kit::diff::FileDiff;

use crate::comments::Comment;
use crate::gh::LineComment;

/// Between the parts of a summary: a comment's text can hold any Markdown, headings included.
const RULE: &str = "\n\n---\n\n";

/// What a review sends.
#[derive(Debug, PartialEq, Eq)]
pub struct Composed {
    pub on_lines: Vec<LineComment>,
    pub summary: String,
}

/// `comment` on its lines of `files`, a pull request's diff, when its hunks show them.
pub fn on_lines(comment: &Comment, files: &[FileDiff]) -> Option<LineComment> {
    let lines = comment.anchor.lines.as_ref()?;
    let (file, end) = comment.anchor.position(files)?;
    let start = end.line + 1 - lines.quoted.len() as u32;
    Some(LineComment { path: files[file].path().to_owned(), side: end.side, start, end: end.line, body: comment.body.clone() })
}

/// `comments` as one review of the pull request whose diff is `files`, its summary starting with
/// `summary`.
pub fn compose(comments: &[&Comment], files: &[FileDiff], summary: &str) -> Composed {
    let (mut placed, mut unplaced) = (Vec::new(), Vec::new());
    for comment in comments {
        match on_lines(comment, files) {
            Some(comment) => placed.push(comment),
            None => unplaced.push(summary_block(comment)),
        }
    }
    let summary = Some(summary.trim().to_owned()).filter(|summary| !summary.is_empty()).into_iter().chain(unplaced).collect::<Vec<_>>().join(RULE);
    Composed { on_lines: placed, summary }
}

/// `comment` as Markdown: where it points, the code it quotes, then what it says.
fn summary_block(comment: &Comment) -> String {
    let labels: String = comment.labels().iter().map(|label| format!(" {label}")).collect();
    let mut block = format!("**`{}`**{labels}\n\n", comment.anchor.location());
    if let Some(lines) = &comment.anchor.lines {
        let fence = fence(&lines.quoted);
        block += &format!("{fence}\n{}\n{fence}\n\n", lines.quoted.join("\n"));
    }
    block + comment.body.trim()
}

/// A code fence longer than any run of backticks in `lines`.
fn fence(lines: &[String]) -> String {
    let longest = lines.iter().flat_map(|line| line.split(|c| c != '`')).map(str::len).max().unwrap_or(0);
    "`".repeat(longest.max(2) + 1)
}

#[cfg(test)]
mod tests {
    use rune_kit::diff::Side;

    use super::*;
    use crate::comments::{Anchor, Lines};
    use crate::review::Review;

    const PATCH: &str = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,3 +1,3 @@\n one\n-two\n+deux\n three\n";

    fn comment(lines: Option<(u32, &[&str])>, body: &str) -> Comment {
        let lines = lines.map(|(start, quoted)| Lines { start, end: start + quoted.len() as u32 - 1, quoted: quoted.iter().map(|line| (*line).to_owned()).collect() });
        let anchor = Anchor { path: "a.rs".into(), side: Side::New, lines };
        Comment { id: 1, anchor, review: Review::Changes, commit: None, body: body.into(), resolved: false, outdated: false, snippet: None }
    }

    #[test]
    fn lines_the_diff_shows_go_on_them_and_the_rest_into_the_summary() {
        let files = rune_kit::diff::parse(PATCH);
        // Found where the diff has it, though it was written further down.
        let shown = comment(Some((7, &["deux", "three"])), "Why?");
        let (gone, whole) = (comment(Some((2, &["two `x`"])), "Gone"), comment(None, "All of it"));
        let composed = compose(&[&shown, &gone, &whole], &files, " Looks fine. ");
        assert_eq!(composed.on_lines, vec![LineComment { path: "a.rs".into(), side: Side::New, start: 2, end: 3, body: "Why?".into() }]);
        assert_eq!(composed.summary, "Looks fine.\n\n---\n\n**`a.rs:2`**\n\n```\ntwo `x`\n```\n\nGone\n\n---\n\n**`a.rs`** (file)\n\nAll of it");
    }

    #[test]
    fn a_summary_has_no_rule_at_either_end() {
        let files = rune_kit::diff::parse(PATCH);
        let whole = comment(None, "All of it");
        assert_eq!(compose(&[&whole], &files, " ").summary, "**`a.rs`** (file)\n\nAll of it");
        assert_eq!(compose(&[], &files, "Fine").summary, "Fine");
    }

    #[test]
    fn a_fence_outlasts_the_backticks_it_holds() {
        assert_eq!(fence(&["let a = 1;".into()]), "```");
        assert_eq!(fence(&["````".into()]), "`````");
    }
}
