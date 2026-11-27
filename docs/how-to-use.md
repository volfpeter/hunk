# How to use hunk

## Install

From a clone of the repository, on Linux or macOS, with [Rust](https://rustup.rs) and Git installed:

```bash
./install.sh
```

It builds the app and installs the `hunk` command in `~/.local/bin`, which must be on your `PATH`, and an entry in your app launcher. On macOS the entry is `hunk.app` in `~/Applications`. The app started from the launcher opens the last repository you had open.

To update, `git pull` and run `./install.sh` again. `./install.sh --uninstall` removes everything it installed. Your comments stay in each repository, under `.git/hunk/`.

## Open a repository

`hunk ~/dev/project` opens the repository that folder is in, and `hunk .` the one you're in. `hunk` alone opens the last repository you had open. Without one it shows an empty window, with **Open a repository**.

Open a folder that isn't in a repository, and it offers **Initialise repository**, which turns it into one.

**Ctrl+O**, or clicking the repository's name in the top left, opens another repository. With nothing typed, it lists the ones you opened recently. Type a path, like `~/dev/`, to see the folders there, with repositories marked. A path without `/` or `~` at the start starts in your home folder. Use the arrow keys to pick, **Tab** to complete the picked folder and **Enter** to open it, or **Ctrl+Enter** to open it in a new window, beside this one. A repository already open in a window isn't opened again: that window comes to the front. Nothing can be opened while a Git action runs.

The name in the bar comes from the repository's `origin` remote, like `owner/project`, or from its folder when it has none.

## Pick what to review

The bar at the top picks the view, what you're looking at, from the narrowest to the widest:

- **Staging:** what changed since you last staged. It's what opens the first time.
- **Changes:** everything not committed yet
- **Branches:** what a branch added since it left its parent: the branch it grew from, else the default branch
- **Commits:** any of the latest 200 on the current branch
- **Stashes:** what a stash holds

The pick holds for every repository, and the next time you start the app.

**Unified** and **Split**, in the diff's top row, switch how it is laid out, as does **Ctrl+\\**. The choice holds for every diff and repository, and the next time you start the app. That row also says what the diff compares and how much changed.

The file list and the diff go by path, folders first.

Unchanged lines between changes fold into a bar that says how many. The arrows on its left show 20 more lines next to the change they point at, and clicking the count shows 20 at each end. Shift-click either one to show them all. Folds close again when the diff updates.

**Ctrl+F**, or the magnifier in the top bar, opens a find field floating in the middle of the bar. It finds text in the diff as you type, ignoring case. It looks in the changes and the lines around them, not in folded lines. The line found is selected, and the field says which of how many it is. **Enter** goes to the next one, **Shift+Enter** to the one before, and from the last it goes round to the first. It finds from the line you selected, or from the top of the file you're at. **Escape** closes the field and leaves the line selected, so in Changes and Staging the full-screen button in the file's header opens the file right there.

Everything updates by itself when the repository changes, so you can read while an agent edits. You stay where you are in the diff, and a comment you're writing is kept. If a file above you joins or leaves the diff, you stay in your file but go back to its start. **Git: Refresh** in the [command palette](#commands) is there in case something was missed.

## Read any file

**Ctrl+P**, or **File: Open a file** in the [command palette](#commands), opens a file of the working tree over the review, whole. Its changes since the last commit show in it, staged or not, so a file you haven't touched is just its code. Type to find a file: the letters only need to come in order, so `apprs` finds `src/app.rs`. With nothing typed, the files you opened last come first, then the changed ones, marked with a dot. **Ctrl+P** again picks another file without leaving.

The full-screen button at the end of a file's header in the diff, or **File: Open app.rs** in the palette for the file at the top, opens that file. In Changes and Staging it opens at the lines you selected.

It shows what is on disk now, whatever the review compares, and updates when the file changes. **Escape** closes it, and the review is as you left it. Opening it again brings back the same file, where you were.

To look around instead, **Ctrl+E**, or **File: Browse files**, opens the overlay on a tree of the files, the changed ones marked with a dot. Click a file to open it, or move to it with the arrow keys. The file you open shows selected in the tree. **Ctrl+E** in the overlay, or the panel icon in its header, shows or hides the tree, and it stays that way next time.

**Ctrl+F**, or the magnifier at the top of the overlay, finds in the whole file, as in the diff, with the field floating over the overlay's toolbar. **Escape** closes the field first, then the overlay.

**Alt+←** and **Alt+→**, or the arrows at the top of the overlay, go back and forward through the files you opened, each where you left it.

Files Git ignores aren't offered. To bring some back, like `.env` or generated code, un-ignore them in a `.ignore` file, as ripgrep reads it: a line `!.env`.

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

A comment appears in any view that contains its lines, not only the one you wrote it in. Lines in a fold count: the comment shows once the fold is open, and **Show in diff** on the comments page opens the fold. You can comment on unfolded lines too. An outdated comment has nowhere to sit in the diff, so **Show in diff** shows it above the diff.

## See every comment

**Comments**, the speech bubble on the right of the bar, shows how many comments are open. Press it, or **Ctrl+0**, for a page of every comment, from any view: open ones first, then outdated ones, then resolved ones. Press it again, or pick a view in the bar, to go back.

Each comment sits under the code it was written on, with a few lines around it, as that code was when you wrote it. So an outdated comment still shows what it was about, and its line numbers can differ from where the comment is now, which its header says. The header also names the view it was written in and its status.

Resolved comments start closed: open one with the arrow by its file name. What you open or close stays so until the comment is resolved or reopened.

**Show in diff** in a comment's header opens its view at the comment. A commit, branch or stash that is gone has nothing to open.

## Hand comments to an agent

The copy button beside the open count at the top of the comments page, **Copy 3 open comments** in the [command palette](#commands), or **Ctrl+Shift+C** copies the open comments as one prompt, with the file, lines and code each one refers to. Paste it into your coding agent. Outdated comments are left out, as their code is gone. **Copy** in a comment's header copies that one alone, outdated or not.

Once a comment is dealt with, press **Resolve** on it. Resolved comments leave the diff and future prompts, but stay at the end of the comments page. Press **Reopen** on one to bring it back.

## Clear comments out

The row at the top of the comments page counts open, outdated and resolved comments, with buttons beside each count acting on all of them at once. Beside the open count: copy them, and resolve every comment, outdated ones too. Beside the outdated and the resolved counts: delete them, after a confirm. The [command palette](#commands) has all of these, plus **Delete all comments**.

## Stage, commit and discard

In **Changes** and **Staging**, the panel above the diff lists what is **Staged** on the left and what is **Unstaged** on the right. A partly staged file is in both. In **Changes**, a file in either list jumps to it in the diff. **Staging** leaves staged files out of the diff, so the staged list there is for seeing what a commit would take.

Each row has **Stage** or **Unstage**. On a folder, it acts on every file under it. Its **⋯** button, or a right-click, opens a menu with the rest: **Open file** in the [file overlay](#files), **Copy path**, and **Discard**, which asks first.

Each file in the diff has its buttons in its header:

- **Stage** and **Unstage** work on the whole file.
- **Discard** drops what the diff shows for that file, after asking. In **Changes** it goes back to the last commit, and in **Staging** back to what you staged. Either way a new file is deleted. This can't be undone.

**Stage all** above the unstaged list and **Unstage all** above the staged list act on everything at once. The [command palette](#commands) has them too.

To commit, press **Commit** at the right of the bar, the dot on a line, which shows how many files are staged, **Ctrl+Enter** (or run **Git: Commit** from the [command palette](#commands)), write a message and press the dialog's **Commit**, or **Ctrl+Enter**. On its left, the dialog sums up what the commit takes: new, changed and deleted files. It also lists what stays uncommitted, marking files the commit takes only part of, which often means you forgot to stage something. With nothing staged, the commit takes everything, new files included. **Enter** starts a new line. Your commit hooks run as usual. Closing the dialog keeps the message, and so does a commit that fails.

**Git: Undo last commit** in the [command palette](#commands) takes the last commit back, leaving its changes staged and its message in the commit dialog, unless you've written another since. It asks first when the commit is already pushed, as sending what you commit instead then takes **Force push**. It isn't offered for a merge commit.

**Git: Discard all changes** in **Changes**, or **Git: Discard all unstaged changes** in **Staging**, drops everything the view shows, after a confirm that counts the new files it deletes. This can't be undone.

## Branches and stashes

Selecting a branch, commit or stash shows it. To act on one, open its menu with the **⋯** at the end of its row, a right-click, or the **Menu** key or **Shift+F10** on the highlighted row. Its menu doesn't change what the diff shows. Each row says how long ago it last changed, right after its name.

- **Branches:** the branch you're on is picked when you open it. Double-click another, or press **Enter** on it, to check it out. Its menu also has **Update from** the remote branch it tracks, which brings in what was fetched there without checking it out, but only when your branch has nothing of its own the remote lacks. **Merge or rebase** opens [its dialog](#merge-and-rebase) on that branch. **Delete** says whether commits are lost before it deletes. The **Delete** key does the same to the selected branch, then selects the next one, so you can clear several in a row. The branch you're on has **Pull** and **Push** instead. To switch or start a branch, see [Switch branches](#switch-branches). Whichever way you switch, even from a terminal, the view moves to the branch you're now on.
- **Commits:** the menu copies the hash. The last commit's menu also has **Undo commit**, as in the [command palette](#commands).
- **Ctrl+Shift+S**, or **Git: Stash** in the [command palette](#commands), puts changes aside. Give the stash a name if you like, and pick what it takes: your changes and new files, your changes alone, or only what's staged.
- **Stashes:** double-click one, or press **Enter** on it, to bring it back and keep it. Its menu also has **Pop**, which brings it back and removes it, and **Drop**, which deletes it after asking. The **Delete** key drops the selected stash the same way.

## Merge and rebase

**Ctrl+Shift+M**, or **Git: Merge or rebase** in the [command palette](#commands), lists your other branches, then those only on a remote. Type to filter and pick one with the arrow keys or a click. Then choose **Merge**, which brings its commits into your branch, or **Rebase**, which makes your branch's own commits again on top of it, and press the button or **Enter**. Rebasing a branch you've pushed warns you first: sending it afterwards takes **Force push**.

A conflict stops either, and the bar says **Merging** or **Rebasing** with the number of conflicts until it's done. Conflicted files show up in the unstaged list, marked as such, with their conflict markers in the diff. Settle them in your editor and stage them. Click **Merging** or **Rebasing** in the bar, or press **Ctrl+Shift+M** again, to see what's left: **Continue** finishes the merge with Git's own message, or goes on with the rebase, once no file is conflicted. **Abort** asks first, then puts everything back as it was before. Commit waits until the merge or rebase is over.

## Pull requests

In **Branches**, a branch on GitHub shows its pull request above the list: its number, title and whether it is open, a draft, merged or closed. Click it to open it in the browser. The app asks GitHub through `gh`, so install it and log in with `gh auth login`. Without one, or if `gh` can't look, **Create pull request** opens GitHub's form for a new one, comparing the branch with its parent, as the diff does; hover it to see why `gh` couldn't look. On Forgejo or Gitea, **Create pull request** always shows, as the app can't look there yet.

The branch you're on shows its pull request in the bar too, left of **Commit**: its number, with the title and status in the tooltip. Its icon says where it stands: open, draft (a dashed line), merged (two lines joined) or closed (crossed out). Only an open one is bright. Click it to open it.

**Ctrl+Shift+R**, from any view, opens the pull request of the branch you're on, or the forge's form for a new one. Before the form, it pushes the branch if the forge doesn't have it or some of its commits yet. It does nothing on the default branch.

The pull request goes to the repository named in the bar: the one `origin` points at. The [command palette](#commands) has the same, under **Git**. The app asks GitHub again when you come back to its window, so a pull request you just created or merged in the browser shows then. **Git: Refresh** in the palette and a fetch ask again too.

## Review a pull request

Comments can go to the current branch's pull request on GitHub as one review. On the comments page, press the pull request icon beside the open count, or pick **Git: Review pull request** in the [command palette](#commands). It shows while the branch's pull request is open or a draft.

The dialog lists the open comments, wherever you wrote them. Those whose code is still there start ticked; click a row to change that. Each row says where its comment goes. GitHub only takes a comment on lines its pull request's diff shows, so a comment whose code GitHub has goes **on its lines**, and the rest go **in the summary**, with their location and the code they quote: comments on a whole file, on lines you unfolded, or on changes you haven't pushed.

Write a summary if you want one, then press **Request changes**, **Comment** or **Approve**: that sends the review. The comments sent are resolved. If GitHub refuses, the dialog says why and keeps everything; it won't let you approve or request changes on your own pull request, for one.

## Switch branches

**Ctrl+B**, or clicking the branch name next to the repository's, opens the branch switcher. It lists your branches, the most recently changed first, then those that are only on a remote, like a colleague's or an agent's. Type to filter, use the arrow keys to pick and **Enter** to switch. A branch that is only on a remote becomes a local branch tracking it.

Type a name no branch has, and the list offers **Create branch** with it, starting from where you are.

**Fetch**, at the bottom of the switcher, or **Ctrl+Shift+F**, gets what every remote has. Branches deleted on a remote leave the list.

## Remotes

**Remotes**, at the bottom of the branch switcher or in the [command palette](#commands), lists the repository's remotes with their URLs. To add one, type its URL and press **Add** or **Enter**. Give it a name, or leave the name empty to call it `origin` when there is no `origin` yet. **Remove** asks first, as the remote's branches go with it. Adding a remote doesn't fetch it.

## Pull and push

**Pull** (the arrow down) and **Push** (the arrow up) sit at the right of the bar, each with the number of commits there are to get or send when either has some, so both show a count, even if one is 0. Hover them for their keys. **Ctrl+Shift+↓** pulls and **Ctrl+Shift+↑** pushes. A branch that was never pushed goes to `origin`.

Once your branch and its remote went apart, for instance after a rebase, **Git: Force push** in the [command palette](#commands) replaces the remote's branch with yours, after a confirm saying how many of its commits are lost. If someone pushed there since you last fetched, it refuses instead.

They use your own Git setup, but can't answer prompts: if Git needs a password it fails and shows you why. When anything fails, you see what Git said. When it works, a notice in the bottom right says what happened, for a few seconds; staging and unstaging show in the lists instead.

## Commands

**Ctrl+K** opens a list of everything you can do right now. Each command starts with its topic, such as **Git:** or **View:**. Type to filter, by topic too, with the letters only needing to come in order, the best match first, use the arrow keys to pick and **Enter** to run the command marked with **›**. Actions on a file act on the one at the top of the diff, and those on a branch or stash on the one selected in the list.

The list also moves through the diff, switches to another view and opens another repository. A command with a key shows it, to use without the list: **]** and **[** go to the next and previous file, **j** and **k** to the next and previous change.

Keys follow a pattern: **Ctrl** with a letter opens something to pick from, like **Ctrl+K**, **Ctrl+O**, **Ctrl+B**, **Ctrl+P** and **Ctrl+E**, and **Ctrl+Shift** with a letter or arrow runs a Git operation, or copies the open comments with **Ctrl+Shift+C**. **Ctrl+1** to **Ctrl+5** switch to the views in the bar's order, and **Ctrl+0** opens the comments page. In a dialog asking to confirm, **Ctrl+Enter** goes ahead.

**Ctrl+H**, or the question mark at the far right of the bar, lists every keyboard shortcut, grouped as the palette groups its commands, with a second key where a command has one. It lists them all, including those that do nothing right now, like Push with nothing to send. **Ctrl+H** again, or **Escape**, closes it.
