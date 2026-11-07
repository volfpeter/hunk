# How to use diffactive

Run `diffactive` from any Git repository.

## Pick what to review

The bar at the top picks what you're looking at:

- **Changes:** everything not committed yet
- **Staged vs unstaged:** what changed since you last staged
- **Commits:** any of the latest 200 on the current branch
- **Branches:** what a branch added since it left the default branch
- **Stashes:** what a stash holds
- **Comments:** every comment, from any review, open ones first

**Unified** and **Split**, right above the diff, switch how it is laid out.

The file list and the diff go by path, folders first.

Everything updates by itself when the repository changes, so you can read while an agent edits. You stay where you are in the diff, and a comment you're writing is kept. If a file above you joins or leaves the diff, you stay in your file but go back to its start. **Refresh** is there in case something was missed.

## Comment on code

- **A line:** hover it and press **+**.
- **Several lines:** drag over their line numbers, then press **+**.
- **A whole file:** press **Comment on file** in the file's header.

**Ctrl+Enter** saves the comment, **Enter** starts a new line and **Esc** throws the draft away.

Comments are saved in the repository's Git folder, at `.git/diffactive/comments.json`, so they never end up in a commit.

## Change a comment

Press **Edit** on a comment to reword it. **Ctrl+Enter** saves, **Enter** starts a new line and **Esc** keeps the old wording. Editing changes only the text: the comment stays on its lines.

Press **Delete** to remove a comment for good. It asks first, and can't be undone.

In **Comments**, the same buttons sit above the list and act on the selected comment.

## How comments follow the code

A comment remembers the lines it was written on. Whenever the repository changes, or you open the app again, it looks for those lines and moves with them.

If the lines are gone, the comment is marked **outdated** rather than lost. It keeps showing the code it was written on, and recovers if that code comes back.

A comment appears in any review that contains its lines, not only the one you wrote it in. An outdated comment has nowhere to sit in the diff, so opening it from **Comments** shows it above the diff.

## Hand comments to an agent

**Copy prompt** copies every open comment as one prompt, with the file, lines and code each one refers to. Paste it into your coding agent.

Once a comment is dealt with, press **Resolve** on it. Resolved comments leave the diff and future prompts, but stay at the end of **Comments**. Select one there and press **Reopen** to bring it back.

## Stage, commit and discard

Each file in **Changes** and **Staged vs unstaged** has its buttons in its header:

- **Stage** and **Unstage** work on the whole file. The file list marks what is **staged** or **partly staged**.
- **Discard** drops what the review shows for that file, after asking. In **Changes** it goes back to the last commit, and in **Staged vs unstaged** back to what you staged. Either way a new file is deleted. This can't be undone.

To commit, write a message in the box above the diff and press **Commit**, or **Ctrl+Enter**. **Enter** starts a new line. Your commit hooks run as usual, and if the commit fails the message stays.

## Branches and stashes

- **Branches:** select a branch and press **Check out**, or press **New branch** to start one from where you are.
- **Stash**, beside **Commit**, puts all your changes aside, new files too.
- **Stashes:** select one and press **Apply** to bring it back and keep it, **Pop** to bring it back and remove it, or **Drop** to delete it, which asks first.

## Pull and push

**Pull** and **Push** sit in the bar, with arrows showing how many commits there are to get or send. A branch that was never pushed goes to `origin`.

They use your own Git setup, but can't answer prompts: if Git needs a password it fails and shows you why. When anything fails, you see what Git said.

## Commands

**Ctrl+K** opens a list of everything you can do right now. Type to filter, use the arrow keys to pick and **Enter** to run. Actions on a file act on the one at the top of the diff, and those on a branch or stash on the one selected in the list.
