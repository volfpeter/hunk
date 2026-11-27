# Features

Everything the app does, to try at your own pace. A tick means Peter tried it and it works: agents add features unticked and never tick them. How to use each one is in [how-to-use.md](how-to-use.md).

## Install

- [ ] `./install.sh` builds the clone and installs the `hunk` command and a launcher entry with its icon
- [ ] The app's window shows the launcher's icon
- [ ] `./install.sh --uninstall` removes them

## Repositories

- [ ] `hunk <folder>` opens the repository the folder is in
- [ ] `hunk` alone opens the last repository, or an empty window offering to open one
- [ ] A folder outside any repository offers to initialise one
- [ ] Ctrl+O, or the repository's name in the bar, opens a repository
- [ ] The open dialog lists recent repositories, newest first, without those that are gone
- [ ] Typing a path completes folders, marking repositories; Tab completes, Enter opens
- [ ] Typing a name in the open dialog lists the recent repositories it matches first
- [ ] Long lists in dialogs (palette, open, branches, merge, conflicts) scroll instead of being cut, following the arrow keys
- [ ] Ctrl+Enter in the open dialog opens the repository in a new window
- [ ] Opening a repository another window shows brings that window to the front
- [ ] The window's title is the open folder's name, or `hunk` with nothing open
- [ ] Switching repositories starts afresh
- [ ] The bar names the repository `owner/project` after its remote, or its folder

## Views

- [ ] The bar lists them by reach: Staging, Changes, Branches, Commits, Stashes
- [ ] Staging opens first, on start and in another repository
- [ ] Changes: everything not committed, new files included
- [ ] Staging: what changed since you last staged
- [ ] Commits: any of the latest 200 on the current branch
- [ ] Branches: what a branch added since it left its parent, the local branch it grew from, else the default branch
- [ ] Branches opens on the current branch
- [ ] Stashes: what a stash holds
- [ ] A repository without commits works

## Diff

- [ ] Unified and split layout
- [ ] Unified or Split holds for every diff, the comments page too, and across restarts
- [ ] The picked view (Staging, Changes, …) holds across repositories and restarts
- [ ] Syntax highlighting
- [ ] Changed files list jumps to a file, and follows as you scroll
- [ ] File list and diff use the same order, by path
- [ ] Collapse a file
- [ ] Show the unchanged lines around changes: 20 more from an arrow or the count, all of them with Shift
- [ ] Ctrl+F, the magnifier in the top bar, or Navigation: Find opens a find field floating in the middle of the top bar; the magnifier closes it again
- [ ] It finds as you type, ignoring case and accents you leave out, and selects the line it found
- [ ] It counts the lines found (`2 of 5`); Enter and Shift+Enter, or its arrows, go to the next and previous one, wrapping round
- [ ] It finds from the selected line, or from the top of the file on screen with nothing selected
- [ ] It finds in the changes shown, not in folded lines, opened or not
- [ ] Escape, or ×, closes it; the line found stays selected, so the full-screen icon opens the file there in Changes and Staging
- [ ] It works on the comments page too

## Files

- [ ] Ctrl+P, or File: Open a file in the palette, opens any working-tree file whole in an overlay over the review
- [ ] Its staged and unstaged changes show in it, together; an unchanged file is just its code, a new one all added
- [ ] Always unified, whatever the review's layout
- [ ] Type to find a file, letters in order but not together (`apprs` finds `src/app.rs`)
- [ ] With nothing typed: the last files opened first, then the changed ones, marked with a dot, then the rest
- [ ] The last 20 files opened are remembered per repository, across restarts
- [ ] Ctrl+P in the overlay, or Open a file in its header, picks another file without closing it
- [ ] Escape on the picker goes back to the file shown in the overlay; on its own, it closes
- [ ] Escape, or ×, closes the overlay, the review untouched; opening it again shows the same file where you were
- [ ] The full-screen icon at the end of a file's header in the diff, or File: Open <name> in the palette, opens that file, at the selected lines in Changes and Staging
- [ ] The file updates when it changes on disk, keeping your place
- [ ] Ignored files aren't offered, but `.ignore` brings them back (`!.env`)
- [ ] j / k and ] / [ move through its changes
- [ ] Alt+← / Alt+→, or the arrows in the header, go back and forward through the files opened, each where you left it
- [ ] Picking a file after going back drops the ones ahead, as a browser does
- [ ] The picker is a card at the top of the overlay, without a title or a second dimming; opened with the overlay closed, it stands alone over the review
- [ ] Ctrl+E, or File: Browse files, opens the overlay on a file tree, without the picker
- [ ] The tree lists every file on offer, folders first, changed files marked with a dot; a click, or moving to it with the arrow keys, opens one
- [ ] The file opened is revealed in the tree, its folders opened
- [ ] Ctrl+E in the overlay, or the panel icon in its header, shows or hides the tree; the choice holds across restarts
- [ ] Ctrl+F in the overlay, or the magnifier in its header, finds in the whole file; the field floats over its toolbar, and Escape closes it, not the overlay

## Comments

- [ ] Comment on a line
- [ ] Comment on a selected range
- [ ] Comment on a whole file
- [ ] Ctrl+Enter saves, Enter starts a new line, Esc cancels
- [ ] Comments survive a restart
- [ ] Comments follow their code as it moves
- [ ] Comments whose code is gone show as outdated, and recover if it returns
- [ ] Comments appear in any view containing their lines
- [ ] Comment on unfolded lines, and Show in diff on a comment in a fold unfolds it
- [ ] Comments, on the right of the bar with the open count, opens a page of every comment, and again goes back
- [ ] The page shows each comment under the code it was written on, with three lines around it, even once that code has changed
- [ ] The page lists open comments, then outdated, then resolved ones, which start closed
- [ ] Opening or closing an entry on the page holds until the comment is resolved or reopened
- [ ] Each entry's header says where the comment is now, the view it was written in and its status
- [ ] Show in diff opens the comment's view at the comment, resolved or not
- [ ] Edit, delete, resolve and reopen, in the diff and on the page
- [ ] Copy one comment, outdated or not, from its entry
- [ ] Copy every open comment that isn't outdated, from the page or the palette, its label counting them
- [ ] The page's top row counts open, outdated and resolved comments, each with icon buttons acting on all of them
- [ ] Resolve every comment, outdated ones too, from the page or the palette
- [ ] Delete every outdated or every resolved comment, from the page or the palette, after a confirm
- [ ] Delete every comment from the palette, after a confirm

## Staying up to date

- [ ] Refreshes by itself when files or git state change
- [ ] Ignored files, like build output, don't trigger a refresh
- [ ] Edits elsewhere leave your place in the diff alone
- [ ] A file joining or leaving above you keeps you in your file, at its start
- [ ] A comment being written survives edits, even if its lines disappear
- [ ] Refresh from the palette

## Git

- [ ] Stage a file
- [ ] Unstage a file
- [ ] Stage and unstage everything, from the palette
- [ ] Stage all above the unstaged list, Unstage all above the staged list
- [ ] Changes and Staging list staged files on the left and unstaged on the right, a partly staged file in both
- [ ] The staged list shows the lines each file changes
- [ ] Stage or unstage a file, or a whole folder, from its row in the lists
- [ ] In Changes, a file in either list jumps to it in the diff
- [ ] Commit from a dialog, with a multi-line message
- [ ] A failed or closed commit dialog keeps its message
- [ ] The commit dialog sums up what the commit takes and what stays uncommitted, marking partly committed files
- [ ] With nothing staged, Commit takes everything, new files included
- [ ] With nothing changed, Commit makes an empty commit, to trigger CI for one
- [ ] Discard in Changes goes back to the last commit
- [ ] Discard in Staging goes back to what you staged
- [ ] Discarding a new file deletes it
- [ ] Discard and Drop ask first
- [ ] Check out a branch
- [ ] Create a branch
- [ ] Ctrl+B, Ctrl+Shift+B or the branch name in the bar opens one switcher: local branches newest first, then those only on a remote
- [ ] Typing filters the switcher, and a name no branch has offers to create it
- [ ] A branch only on a remote checks out as a local branch tracking it
- [ ] Switching branches, from the app or a terminal, moves the Branches view to the new branch
- [ ] Fetch every remote with Ctrl+Shift+F, or from the switcher, dropping branches deleted there
- [ ] Remotes dialog: see each remote's URL, add one (named `origin` by default when there is none), remove one after a confirm
- [ ] Stash from a dialog: changes and new files, changes alone, or only what's staged
- [ ] Name a stash
- [ ] Apply, pop and drop a stash
- [ ] Double-click a stash to apply it
- [ ] Double-click a branch, or press Enter on it, to check it out
- [ ] Every row of a list has a ⋯ menu, also opened by a right-click or the Menu key, acting on that row without showing it
- [ ] A branch's menu: check out, update from the branch it tracks without checking it out (fast-forward only), merge or rebase, copy the name, delete; the current branch's: pull, push, copy the name
- [ ] A stash's menu: apply, pop, drop
- [ ] Delete asks to delete the selected branch or drop the selected stash, then selects the row below, so the next press goes on; their menus show the key
- [ ] A commit's menu copies its hash; the last commit's also undoes it
- [ ] A file's menu: stage or unstage, open it in the file overlay, copy its path, discard
- [ ] Branches, commits and stashes say how long ago they changed, shortly (18h ago), right after their name; the branch dialogs say it the same way
- [ ] Pull (only tried against a local repository so far)
- [ ] Push, publishing a new branch to `origin` (only tried against a local repository so far)
- [ ] Commit, Pull and Push sit at the right of the bar, their keys in their tooltips
- [ ] The right of the bar is icons only, each with a count when there is one, its tooltip saying what it counts
- [ ] Commit shows how many files are staged, and no count when none are
- [ ] Pull and Push show how many commits there are to get or send
- [ ] When either has commits to move, both show a count, 0 included, so they keep the same width
- [ ] Undo the last commit from the palette, keeping its changes staged and its message for the next commit; it asks first once the commit is pushed
- [ ] Discard all changes (Changes) or all unstaged changes (Staging) from the palette, after a confirm counting the new files it deletes
- [ ] Delete the branch selected in Branches from the palette, the confirm saying whether its commits are lost
- [ ] Force push from the palette once the branch and its remote went apart, refused if the remote moved since the last fetch
- [ ] When git fails, its message shows in a dialog
- [ ] A notice says what an operation did, for a few seconds: commit, stash, checkout, discard, pull, push and the rest, but not staging

## Merge and rebase

- [ ] Ctrl+Shift+M, or Merge or rebase in the palette, opens a dialog of the other branches, local then only on a remote, filtered by typing
- [ ] Pick Merge or Rebase there, each saying which branch goes where
- [ ] Rebasing a pushed branch warns that sending it takes Force push
- [ ] A conflict stops it, showing git's message, and the bar says Merging or Rebasing with the conflict count
- [ ] Conflicted files are marked in the unstaged list, and Staging shows them with their conflict markers
- [ ] Ctrl+Shift+M, or the bar, opens what is in progress: the conflicted files, Abort (asks first) and Continue (once none is left)
- [ ] Continue finishes a merge with git's message, and goes on with a rebase, without an editor
- [ ] Commit waits while a merge or rebase is in progress, or a file is conflicted

## Pull requests

- [ ] In Branches, a branch on GitHub shows its pull request: number, title, and open, draft, merged or closed (needs `gh`, logged in)
- [ ] Click it to open the pull request in the browser
- [ ] A branch without one offers Create pull request, opening the forge's form comparing it with its parent, as the diff does
- [ ] Works on Forgejo and Gitea too, which always offer Create pull request
- [ ] Without `gh`, or when it can't look, Create pull request shows, its tooltip saying why
- [ ] The default branch offers neither
- [ ] The palette offers the same, under Git
- [ ] The bar shows the current branch's pull request left of Commit: an icon and its number, the title and status in its tooltip
- [ ] Its icon shows whether it is open, a draft, merged or closed, and only an open one is bright
- [ ] Click it to open the pull request in the browser
- [ ] Ctrl+Shift+R, or the palette, opens the current branch's pull request from any view, or the forge's form for a new one
- [ ] Creating one pushes the branch first when the forge lacks it or some of its commits, then opens the form
- [ ] Coming back to the window looks for pull requests again, so one created or merged in the browser shows
- [ ] Refresh from the palette, or a fetch, looks for pull requests again

## Review a pull request

- [ ] The pull request icon on the comments page, beside the open count, or Git: Review pull request #N in the palette, opens a review of the current branch's open or draft pull request on GitHub
- [ ] It lists the open comments, from any view, those not outdated ticked; click a row to tick or untick it
- [ ] Each row says whether its comment goes on its lines on GitHub or into the summary: on its lines when the pull request's diff shows its code
- [ ] Request changes, Comment and Approve each send the ticked comments and the summary as one review, in one click
- [ ] Comments GitHub can't place, like whole-file ones or ones on code it doesn't have, land in the review's text with their location and quoted code
- [ ] Sent comments are resolved; the dialog closes and a toast says what was sent
- [ ] When GitHub refuses, the dialog stays open and says why, like approving your own pull request
- [ ] Comment and Request changes wait for a ticked comment or a summary; Approve doesn't

## Commands

- [ ] Ctrl+K opens the palette, with what's possible right now
- [ ] Filter by typing, pick with the arrow keys, run with Enter
- [ ] Typing matches letters in order but not together, best match first, in the palette, the branch switcher, the merge dialog and the open dialog
- [ ] Commands are grouped by topic, and typing a topic filters to it
- [ ] File commands act on the file at the top of the diff
- [ ] Next and previous file and hunk from the palette, showing their keys
- [ ] The palette's highlighted command stands out, marked with ›
- [ ] Ctrl+Enter commits, Ctrl+Shift+S stashes, Ctrl+Shift+↓ pulls, Ctrl+Shift+↑ pushes
- [ ] Ctrl+Enter goes ahead in a confirm dialog (delete, discard, drop, force push…), wherever focus is in it
- [ ] The palette shows those keys
- [ ] Switch to any other view from the palette
- [ ] Open a repository from the palette
- [ ] Ctrl+H, or the question mark at the far right of the bar, lists every key, grouped as the palette groups them, Ctrl+K first
- [ ] The list shows keys that do nothing right now too, like Push with nothing to push
- [ ] Ctrl+H again or Escape closes it, and git keys do nothing while it is open
- [ ] It shows a second key where there is one: Ctrl+B / Ctrl+Shift+B
- [ ] The palette offers it too, as Application: Keyboard shortcuts
- [ ] Palette topics: Application, Git, View, File, Navigation; the overlay shows those with keys, in two balanced columns
- [ ] Ctrl+1 to Ctrl+5 switch to Staging, Changes, Branches, Commits and Stashes
- [ ] Ctrl+0 opens the comments page
- [ ] Ctrl+Shift+C copies every open comment that isn't outdated
- [ ] Ctrl+\\ switches the diff between unified and split, also from the palette
