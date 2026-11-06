# How to use diffactive

Run `diffactive` from any Git repository. It shows diffs and keeps review comments, but doesn't change your repository yet.

## Pick what to review

The bar at the top picks what you're looking at:

- **Changes:** everything not committed yet
- **Staged vs unstaged:** what changed since you last staged
- **Commits:** any of the latest 200 on the current branch
- **Branches:** what a branch added since it left the default branch
- **Stashes:** what a stash holds
- **Comments:** every comment, from any review, open ones first

**Unified** and **Split** switch how the diff is laid out.

The file list, diff and comments don't update on their own yet: press **Refresh** to see new changes.

## Comment on code

- **A line:** hover it and press **+**.
- **Several lines:** drag over their line numbers, then press **+**.
- **A whole file:** press **Comment on file** in the file's header.

**Enter** saves the comment, **Shift+Enter** starts a new line and **Esc** throws the draft away.

Comments are saved in the repository's Git folder, at `.git/diffactive/comments.json`, so they never end up in a commit.

## Change a comment

Press **Edit** on a comment to reword it. **Enter** saves, **Shift+Enter** starts a new line and **Esc** keeps the old wording. Editing changes only the text: the comment stays on its lines.

Press **Delete** to remove a comment for good. It asks first, and can't be undone.

In **Comments**, the same buttons sit above the list and act on the selected comment.

## How comments follow the code

A comment remembers the lines it was written on. When you press **Refresh**, or open the app again, it looks for those lines and moves with them.

If the lines are gone, the comment is marked **outdated** rather than lost. It keeps showing the code it was written on, and recovers if that code comes back.

A comment appears in any review that contains its lines, not only the one you wrote it in. An outdated comment has nowhere to sit in the diff, so opening it from **Comments** shows it above the diff.

## Hand comments to an agent

**Copy prompt** copies every open comment as one prompt, with the file, lines and code each one refers to. Paste it into your coding agent.

Once a comment is dealt with, press **Resolve** on it. Resolved comments leave the diff and future prompts, but stay at the end of **Comments**. Select one there and press **Reopen** to bring it back.
