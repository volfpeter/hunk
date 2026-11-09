# How to use hunk

## Open a repository

`hunk ~/dev/project` opens the repository that folder is in, and `hunk .` the one you're in. `hunk` alone opens the last repository you had open. Without one it shows an empty window, with **Open a repository**.

Open a folder that isn't in a repository, and it offers **Initialise repository**, which turns it into one.

**Ctrl+O**, or clicking the repository's name in the top left, opens another repository. With nothing typed, it lists the ones you opened recently. Type a path, like `~/dev/`, to see the folders there, with repositories marked. A path without `/` or `~` at the start starts in your home folder. Use the arrow keys to pick, **Tab** to complete the picked folder and **Enter** to open it. Nothing can be opened while a Git action runs.

The name in the bar comes from the repository's `origin` remote, like `owner/project`, or from its folder when it has none.

## Pick what to review

The bar at the top picks what you're looking at, from the narrowest to the widest:

- **Staging:** what changed since you last staged. It's what opens the first time.
- **Changes:** everything not committed yet
- **Branches:** what a branch added since it left its parent: the branch it grew from, else the default branch
- **Commits:** any of the latest 200 on the current branch
- **Stashes:** what a stash holds

The pick holds for every repository, and the next time you start the app.

**Unified** and **Split**, in the diff's top row, switch how it is laid out. The choice holds for every diff and repository, and the next time you start the app. That row also says what the diff compares and how much changed.

The file list and the diff go by path, folders first.

Unchanged lines between changes fold into a bar that says how many. Its arrows show 20 more above or below, and clicking the count shows them all. Folds close again when the diff updates.

Everything updates by itself when the repository changes, so you can read while an agent edits. You stay where you are in the diff, and a comment you're writing is kept. If a file above you joins or leaves the diff, you stay in your file but go back to its start. **Git: Refresh** in the [command palette](#commands) is there in case something was missed.

## Comment on code

- **A line:** hover it and press **+**.
- **Several lines:** drag over their line numbers, then press **+**.
- **A whole file:** press **Comment on file** in the file's header.

**Ctrl+Enter** saves the comment, **Enter** starts a new line and **Esc** throws the draft away.

Comments are saved in the repository's Git folder, at `.git/hunk/comments.json`, so they never end up in a commit.

## Change a comment

Press **Edit** on a comment to reword it. **Ctrl+Enter** saves, **Enter** starts a new line and **Esc** keeps the old wording. Editing changes only the text: the comment stays on its lines.

Press **Delete** to remove a comment for good. It asks first, and can't be undone.

The same buttons sit on each comment on the [comments page](#see-every-comment).

## How comments follow the code

A comment remembers the lines it was written on. Whenever the repository changes, or you open the app again, it looks for those lines and moves with them.

If the lines are gone, the comment is marked **outdated** rather than lost. It keeps showing the code it was written on, and recovers if that code comes back.

A comment appears in any review that contains its lines, not only the one you wrote it in. Lines in a fold count: the comment shows once the fold is open, and **Show in diff** on the comments page opens the fold. You can comment on unfolded lines too. An outdated comment has nowhere to sit in the diff, so **Show in diff** shows it above the diff.

## See every comment

**Comments**, on the right of the bar, shows how many comments are open. Press it for a page of every comment, from any review: open ones first, then outdated ones, then resolved ones. Press it again, or pick a review in the bar, to go back.

Each comment sits under the code it was written on, with a few lines around it, as that code was when you wrote it. So an outdated comment still shows what it was about, and its line numbers can differ from where the comment is now, which its header says. The header also names the review it was written in and its status.

Resolved comments start closed: open one with the arrow by its file name. What you open or close stays so until the comment is resolved or reopened.

**Show in diff** in a comment's header opens its review at the comment. A commit, branch or stash that is gone has no review to open.

## Hand comments to an agent

**Copy 3 open comments**, at the top of the comments page or in the [command palette](#commands), copies them as one prompt, with the file, lines and code each one refers to. Paste it into your coding agent. Outdated comments are left out, as their code is gone. **Copy** in a comment's header copies that one alone, outdated or not.

Once a comment is dealt with, press **Resolve** on it. Resolved comments leave the diff and future prompts, but stay at the end of the comments page. Press **Reopen** on one to bring it back.

## Stage, commit and discard

In **Changes** and **Staging**, the panel above the diff lists what is **Staged** on the left and what is **Unstaged** on the right. A partly staged file is in both. In **Changes**, a file in either list jumps to it in the diff. **Staging** leaves staged files out of the diff, so the staged list there is for seeing what a commit would take.

Each row has **Stage** or **Unstage**. On a folder, it acts on every file under it.

Each file in the diff has its buttons in its header:

- **Stage** and **Unstage** work on the whole file.
- **Discard** drops what the review shows for that file, after asking. In **Changes** it goes back to the last commit, and in **Staging** back to what you staged. Either way a new file is deleted. This can't be undone.

**Stage all** above the unstaged list and **Unstage all** above the staged list act on everything at once. The [command palette](#commands) has them too.

To commit, press **Commit** at the right of the bar, **Ctrl+Enter** or **Ctrl+Shift+C** (or run **Git: Commit** from the [command palette](#commands)), write a message and press the dialog's **Commit**, or **Ctrl+Enter**. On its left, the dialog sums up what the commit takes: new, changed and deleted files. It also lists what stays uncommitted, marking files the commit takes only part of, which often means you forgot to stage something. With nothing staged, the commit takes everything, new files included. **Enter** starts a new line. Your commit hooks run as usual. Closing the dialog keeps the message, and so does a commit that fails.

## Branches and stashes

- **Branches:** the branch you're on is picked when you open it. Select another and press **Check out** to switch to it. To switch or start a branch, see [Switch branches](#switch-branches). Whichever way you switch, even from a terminal, the review moves to the branch you're now on.
- **Ctrl+Shift+S**, or **Git: Stash** in the [command palette](#commands), puts changes aside. Give the stash a name if you like, and pick what it takes: your changes and new files, your changes alone, or only what's staged.
- **Stashes:** select one and press **Apply** to bring it back and keep it, **Pop** to bring it back and remove it, or **Drop** to delete it, which asks first. Double-clicking a stash applies it.

## Switch branches

**Ctrl+B**, or clicking the branch name next to the repository's, opens the branch switcher. It lists your branches, the most recently changed first, then those that are only on a remote, like a colleague's or an agent's. Type to filter, use the arrow keys to pick and **Enter** to switch. A branch that is only on a remote becomes a local branch tracking it.

Type a name no branch has, and the list offers **Create branch** with it, starting from where you are.

**Fetch**, at the bottom of the switcher, or **Ctrl+Shift+F**, gets what every remote has. Branches deleted on a remote leave the list.

## Remotes

**Remotes**, at the bottom of the branch switcher or in the [command palette](#commands), lists the repository's remotes with their URLs. To add one, type its URL and press **Add** or **Enter**. Give it a name, or leave the name empty to call it `origin` when there is no `origin` yet. **Remove** asks first, as the remote's branches go with it. Adding a remote doesn't fetch it.

## Pull and push

**Pull** (the arrow down) and **Push** (the arrow up) sit at the right of the bar, each with the number of commits there are to get or send when either has some, so both show a count, even if one is 0. Hover them for their keys. **Ctrl+Shift+↓** pulls and **Ctrl+Shift+↑** pushes. A branch that was never pushed goes to `origin`.

They use your own Git setup, but can't answer prompts: if Git needs a password it fails and shows you why. When anything fails, you see what Git said. When it works, a notice in the bottom right says what happened, for a few seconds; staging and unstaging show in the lists instead.

## Commands

**Ctrl+K** opens a list of everything you can do right now. Each command starts with its topic, such as **Git:** or **Review:**. Type to filter, by topic too, use the arrow keys to pick and **Enter** to run the command marked with **›**. Actions on a file act on the one at the top of the diff, and those on a branch or stash on the one selected in the list.

The list also moves through the diff, switches to another review and opens another repository. A command with a key shows it, to use without the list: **]** and **[** go to the next and previous file, **j** and **k** to the next and previous change.

Keys follow a pattern: **Ctrl** with a letter opens something to pick from, like **Ctrl+K**, **Ctrl+O** and **Ctrl+B**, and **Ctrl+Shift** with a letter or arrow runs a Git operation.
