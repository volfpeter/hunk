# Features

Everything the app does, to try at your own pace. A tick means Peter tried it and it works: agents add features unticked and never tick them. How to use each one is in [how-to-use.md](how-to-use.md).

## Repositories

- [ ] `diffactive <folder>` opens the repository the folder is in
- [ ] `diffactive` alone opens the last repository, or an empty window offering to open one
- [ ] A folder outside any repository offers to initialise one
- [ ] Ctrl+O, or the repository's name in the bar, opens a repository
- [ ] The open dialog lists recent repositories, newest first, without those that are gone
- [ ] Typing a path completes folders, marking repositories; Tab completes, Enter opens
- [ ] Switching repositories starts afresh, keeping Unified or Split
- [ ] The bar names the repository `owner/project` after its remote, or its folder

## Reviews

- [ ] Changes: everything not committed, new files included
- [ ] Staging: what changed since you last staged
- [ ] Commits: any of the latest 200 on the current branch
- [ ] Branches: what a branch added since it left the default branch
- [ ] Stashes: what a stash holds
- [ ] A repository without commits works

## Diff

- [ ] Unified and split layout
- [ ] Syntax highlighting
- [ ] Changed files list jumps to a file, and follows as you scroll
- [ ] File list and diff use the same order, by path
- [ ] Collapse a file

## Comments

- [ ] Comment on a line
- [ ] Comment on a selected range
- [ ] Comment on a whole file
- [ ] Ctrl+Enter saves, Enter starts a new line, Esc cancels
- [ ] Comments survive a restart
- [ ] Comments follow their code as it moves
- [ ] Comments whose code is gone show as outdated, and recover if it returns
- [ ] Comments appear in any review containing their lines
- [ ] Comments tab lists everything, open first
- [ ] Edit, delete, resolve and reopen, in the diff and from the Comments tab
- [ ] Copy all open comments as one prompt

## Staying up to date

- [ ] Refreshes by itself when files or git state change
- [ ] Ignored files, like build output, don't trigger a refresh
- [ ] Edits elsewhere leave your place in the diff alone
- [ ] A file joining or leaving above you keeps you in your file, at its start
- [ ] A comment being written survives edits, even if its lines disappear
- [ ] Manual Refresh button

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
- [ ] Stash from a dialog: changes and new files, changes alone, or only what's staged
- [ ] Name a stash
- [ ] Apply, pop and drop a stash
- [ ] Pull (only tried against a local repository so far)
- [ ] Push, publishing a new branch to `origin` (only tried against a local repository so far)
- [ ] Pull and Push show how many commits there are to get or send
- [ ] When git fails, its message shows in a dialog
- [ ] A notice says what an operation did, for a few seconds: commit, stash, checkout, discard, pull, push and the rest, but not staging

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
