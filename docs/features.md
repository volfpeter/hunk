# Features

Everything the app does, to try at your own pace. A tick means Peter tried it and it works: agents add features unticked and never tick them. How to use each one is in [how-to-use.md](how-to-use.md).

## Repositories

- [ ] `hunk <folder>` opens the repository the folder is in
- [ ] `hunk` alone opens the last repository, or an empty window offering to open one
- [ ] A folder outside any repository offers to initialise one
- [ ] Ctrl+O, or the repository's name in the bar, opens a repository
- [ ] The open dialog lists recent repositories, newest first, without those that are gone
- [ ] Typing a path completes folders, marking repositories; Tab completes, Enter opens
- [ ] Switching repositories starts afresh
- [ ] The bar names the repository `owner/project` after its remote, or its folder

## Reviews

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
- [ ] The picked review (Staging, Changes, …) holds across repositories and restarts
- [ ] Syntax highlighting
- [ ] Changed files list jumps to a file, and follows as you scroll
- [ ] File list and diff use the same order, by path
- [ ] Collapse a file
- [ ] Show the unchanged lines around changes: 20 more above or below, or all of them

## Comments

- [ ] Comment on a line
- [ ] Comment on a selected range
- [ ] Comment on a whole file
- [ ] Ctrl+Enter saves, Enter starts a new line, Esc cancels
- [ ] Comments survive a restart
- [ ] Comments follow their code as it moves
- [ ] Comments whose code is gone show as outdated, and recover if it returns
- [ ] Comments appear in any review containing their lines
- [ ] Comment on unfolded lines, and Show in diff on a comment in a fold unfolds it
- [ ] Comments, on the right of the bar with the open count, opens a page of every comment, and again goes back
- [ ] The page shows each comment under the code it was written on, with three lines around it, even once that code has changed
- [ ] The page lists open comments, then outdated, then resolved ones, which start closed
- [ ] Opening or closing an entry on the page holds until the comment is resolved or reopened
- [ ] Each entry's header says where the comment is now, the review it was written in and its status
- [ ] Show in diff opens the comment's review at the comment, resolved or not
- [ ] Edit, delete, resolve and reopen, in the diff and on the page
- [ ] Copy one comment, outdated or not, from its entry
- [ ] Copy every open comment that isn't outdated, from the page or the palette, its label counting them

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
- [ ] Discard in Changes goes back to the last commit
- [ ] Discard in Staging goes back to what you staged
- [ ] Discarding a new file deletes it
- [ ] Discard and Drop ask first
- [ ] Check out a branch
- [ ] Create a branch
- [ ] Ctrl+B, Ctrl+Shift+B or the branch name in the bar opens one switcher: local branches newest first, then those only on a remote
- [ ] Typing filters the switcher, and a name no branch has offers to create it
- [ ] A branch only on a remote checks out as a local branch tracking it
- [ ] Switching branches, from the app or a terminal, moves the Branches review to the new branch
- [ ] Fetch every remote with Ctrl+Shift+F, or from the switcher, dropping branches deleted there
- [ ] Remotes dialog: see each remote's URL, add one (named `origin` by default when there is none), remove one after a confirm
- [ ] Stash from a dialog: changes and new files, changes alone, or only what's staged
- [ ] Name a stash
- [ ] Apply, pop and drop a stash
- [ ] Double-click a stash to apply it
- [ ] Pull (only tried against a local repository so far)
- [ ] Push, publishing a new branch to `origin` (only tried against a local repository so far)
- [ ] Commit, Pull and Push sit at the right of the bar, their keys in their tooltips
- [ ] The right of the bar is icons only, each with a count when there is one, its tooltip saying what it counts
- [ ] Commit shows how many files are staged, and no count when none are
- [ ] Pull and Push show how many commits there are to get or send
- [ ] When either has commits to move, both show a count, 0 included, so they keep the same width
- [ ] When git fails, its message shows in a dialog
- [ ] A notice says what an operation did, for a few seconds: commit, stash, checkout, discard, pull, push and the rest, but not staging

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
- [ ] Coming back to the window looks for pull requests again, so one created or merged in the browser shows
- [ ] Refresh from the palette, or a fetch, looks for pull requests again

## Commands

- [ ] Ctrl+K opens the palette, with what's possible right now
- [ ] Filter by typing, pick with the arrow keys, run with Enter
- [ ] Commands are grouped by topic, and typing a topic filters to it
- [ ] File commands act on the file at the top of the diff
- [ ] Next and previous file and hunk from the palette, showing their keys
- [ ] The palette's highlighted command stands out, marked with ›
- [ ] Ctrl+Enter or Ctrl+Shift+C commits, Ctrl+Shift+S stashes, Ctrl+Shift+↓ pulls, Ctrl+Shift+↑ pushes
- [ ] The palette shows those keys
- [ ] Switch to any other review from the palette
- [ ] Open a repository from the palette
