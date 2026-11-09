# How to use diffactive

## Open a repository

`diffactive ~/dev/project` opens the repository that folder is in, and `diffactive .` the one you're in. `diffactive` alone opens the last repository you had open. Without one it shows an empty window, with **Open a repository**.

Open a folder that isn't in a repository, and it offers **Initialise repository**, which turns it into one.

**Ctrl+O**, or clicking the repository's name in the top left, opens another repository. With nothing typed, it lists the ones you opened recently. Type a path, like `~/dev/`, to see the folders there, with repositories marked. A path without `/` or `~` at the start starts in your home folder. Use the arrow keys to pick, **Tab** to complete the picked folder and **Enter** to open it. Nothing can be opened while a Git action runs.

The name in the bar comes from the repository's `origin` remote, like `owner/project`, or from its folder when it has none.

## Pick what to review

The bar at the top picks what you're looking at:

- **Changes:** everything not committed yet
- **Staging:** what changed since you last staged
- **Commits:** any of the latest 200 on the current branch
- **Branches:** what a branch added since it left the default branch
- **Stashes:** what a stash holds
- **Comments:** every comment, from any review, open ones first

**Unified** and **Split**, in the diff's top row, switch how it is laid out. That row also says what the diff compares and how much changed.

The file list and the diff go by path, folders first.

Unchanged lines between changes fold into a bar that says how many. Its arrows show 20 more above or below, and clicking the count shows them all. Folds close again when the diff updates.

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

A comment appears in any review that contains its lines, not only the one you wrote it in. Lines in a fold count: the comment shows once the fold is open, and opening it from **Comments** opens the fold. You can comment on unfolded lines too. An outdated comment has nowhere to sit in the diff, so opening it from **Comments** shows it above the diff.

## Hand comments to an agent

**Copy prompt** copies every open comment as one prompt, with the file, lines and code each one refers to. Paste it into your coding agent.

Once a comment is dealt with, press **Resolve** on it. Resolved comments leave the diff and future prompts, but stay at the end of **Comments**. Select one there and press **Reopen** to bring it back.

## Stage, commit and discard

In **Changes** and **Staging**, the panel above the diff lists what is **Staged** on the left and what is **Unstaged** on the right. A partly staged file is in both. In **Changes**, a file in either list jumps to it in the diff. **Staging** leaves staged files out of the diff, so the staged list there is for seeing what a commit would take.

Each row has **Stage** or **Unstage**. On a folder, it acts on every file under it.

Each file in the diff has its buttons in its header:

- **Stage** and **Unstage** work on the whole file.
- **Discard** drops what the review shows for that file, after asking. In **Changes** it goes back to the last commit, and in **Staging** back to what you staged. Either way a new file is deleted. This can't be undone.

**Stage all** above the unstaged list and **Unstage all** above the staged list act on everything at once. The [command palette](#commands) has them too.

To commit, press **Ctrl+Enter** or **Ctrl+Shift+C** (or run **Git: Commit** from the [command palette](#commands)), write a message and press the dialog's **Commit**, or **Ctrl+Enter**. On its left, the dialog sums up what the commit takes: new, changed and deleted files. It also lists what stays uncommitted, marking files the commit takes only part of, which often means you forgot to stage something. With nothing staged, the commit takes everything, new files included. **Enter** starts a new line. Your commit hooks run as usual. Closing the dialog keeps the message, and so does a commit that fails.

## Branches and stashes

- **Branches:** select a branch and press **Check out**, or press **New branch** to start one from where you are.
- **Ctrl+Shift+S**, or **Git: Stash** in the [command palette](#commands), puts changes aside. Give the stash a name if you like, and pick what it takes: your changes and new files, your changes alone, or only what's staged.
- **Stashes:** select one and press **Apply** to bring it back and keep it, **Pop** to bring it back and remove it, or **Drop** to delete it, which asks first.

## Pull and push

**Pull** and **Push** sit in the bar, with arrows showing how many commits there are to get or send. **Ctrl+Shift+↓** pulls and **Ctrl+Shift+↑** pushes. A branch that was never pushed goes to `origin`.

They use your own Git setup, but can't answer prompts: if Git needs a password it fails and shows you why. When anything fails, you see what Git said. When it works, a notice in the bottom right says what happened, for a few seconds; staging and unstaging show in the lists instead.

## Commands

**Ctrl+K** opens a list of everything you can do right now. Each command starts with its topic, such as **Git:** or **Review:**. Type to filter, by topic too, use the arrow keys to pick and **Enter** to run the command marked with **›**. Actions on a file act on the one at the top of the diff, and those on a branch or stash on the one selected in the list.

The list also moves through the diff, switches to another review and opens another repository. A command with a key shows it, to use without the list: **]** and **[** go to the next and previous file, **j** and **k** to the next and previous change.

Keys follow a pattern: **Ctrl** with a letter opens something to pick from, like **Ctrl+K** and **Ctrl+O**, and **Ctrl+Shift** with a letter or arrow runs a Git operation.
