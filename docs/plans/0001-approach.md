# 0001: Approach

Status: step 4c done, waiting for Peter's UI review. See [Tasks](#tasks).

## Intent

A simple desktop app for reviewing code, mostly written by agents, with the git features that review needs. Not a lazygit replacement: lazygit is a bucket of features, most of which we won't want.

Start simple and iterate. Product scope stays small and uncommon cases wait until they hurt, but the code should be great.

## The model

A **Review** is a comparison between two points. There are five:

| Review             | Compares                                         |
| ------------------ | ------------------------------------------------ |
| Changes            | `HEAD` vs worktree, with untracked files         |
| Staged vs unstaged | index vs worktree, with untracked files          |
| Commit             | first parent vs commit                           |
| Branch             | merge-base with the default branch vs branch tip |
| Stash              | its parent vs the stash                          |

A pull request will be a sixth: same comparison as a branch, with comments going to the forge.

## UI direction

Side-by-side diffs need the width, so nothing sits beside the diff. A bar picks the kind of review, a panel below it holds the list and the changed files, and the diff takes the rest.

```
┌───────────────────────────────────────────────────────────┐
│ repo main  [Changes|Staged vs unstaged|Commits|…]  ⟳ ↓ ↑   │
├─────────────────────────────┬─────────────────────────────┤
│ commit message, or a list   │ changed files          Split │
├─────────────────────────────┴─────────────────────────────┤
│ diff, full width                                           │
└───────────────────────────────────────────────────────────┘
```

## Decisions

- **Rune's own components throughout**, including `rune_kit::diff` and its syntax highlighting. Pierre is DOM-only and Rune has no webview.
- **Shell out to `git`.** User config, hooks and credentials come for free. Blocking `std::process` on workers, no async runtime.
- **One crate** until something forces a split.
- **File-level operations only** for now.
- **`notify` for watching.** Polling git would be dependency-free but spawns processes forever and still needs file stats to see further edits to a changed file.
- **One file order, by path.** The file list's, folders first, and the diff follows it.
- **`dirs` for app folders.** Small, and knows each platform's places.
- **Comments go to agents first.** Each stores path, side, line range and commit, enough to submit to a forge later. No destination abstraction until then.

## Tasks

Work top to bottom. Tasks say what to achieve, not how to build it.

### DONE - Step 1: Review anything

Done. The five reviews, file list, unified and split diff, manual refresh, all read-only.

- [x] Show all five reviews from real repository data
- [x] Unified and split diff, split by default
- [x] Changed files list that jumps to a file in the diff
- [x] Work in a repository without commits

### DONE - Step 2: Comments for agents

Peter reviews agent-written code, comments on what's wrong, hands the comments to an agent, then reviews the follow-up. A typical round: stage what's fine, comment on the rest, and use Staged vs unstaged to see only what the agent changed next. That is one example, not the focus: comments work in every review.

diffle is the behaviour reference.

Tasks:

- [x] Write a comment on a line or a selected range
- [x] Write a comment on a whole file
- [x] Keep comments across restarts
- [x] Keep comments attached as the code around them changes
- [x] List comments whose code is gone as outdated, rather than losing them
- [x] Show open comments from any review, so comments made before staging stay reachable
- [x] Copy all open comments as one prompt for an agent
- [x] Resolve a comment
- [x] Edit a comment
- [x] Delete a comment
- [x] Reopen a resolved comment
- [x] Edit, delete, resolve and reopen from the Comments tab

### DONE - Step 3: Git operations

File-level only: no hunk or line staging for now. Only what normal review work needs. Actions are visible buttons, and the palette is a shortcut to them. Confirm anything destructive.

Keep in mind:

- Agents edit while Peter reads, so a refresh must not yank the view.
- Refreshing also finds comments' code again, reading their files, so a watcher should debounce.
- Stashes go by hash, because `stash@{n}` shifts as stashes come and go.
- Git runs with no stdin, so credential prompts fail. Show git's error rather than handling auth.
- An ssh passphrase prompt may be the exception: launched from a terminal, ssh reads the tty rather than stdin, so the action could hang. Not tried or handled yet.
- Discard means what the review shows: back to `HEAD` in Changes, back to the staged version in Staged vs unstaged.
- The view holds through a refresh, except when a file above the reader joins or leaves the diff: they stay in their file but land on its header (see `docs/rune-quirks.md`).

Tasks:

- [x] Refresh by itself when the repository changes
- [x] Stage and unstage a file
- [x] Commit, with a message
- [x] Discard a file's changes
- [x] Check out a branch and create one
- [x] Stash, and apply, pop or drop a stash
- [x] Pull and push
- [x] Command palette over the same actions

### DONE - Step 4a: Palette and navigation

The palette from step 3 lists what is possible right now. Extend it, but don't overdo it. A file in the diff collapses only from its chevron, which Rune builds: Peter accepts that, so no Rune issue.

The palette draws its own rows, since Rune's `List` has no slot for a key and its highlight is barely visible (see `docs/rune-quirks.md`). Commands carry a topic, shown as a prefix and ordered by it: `List`'s group headers repeated the prefix and cost too much height.

Tasks:

- [x] Open the palette with Ctrl+Shift+P too, keeping Ctrl+K
- [x] Navigation in the palette, showing the keys the diff already has: next and previous file and hunk
- [x] Switch the review kind from the palette
- [x] Stage and unstage everything from the palette

### Step 4b: Commit and stash

Staged vs unstaged is the review Peter finds most useful, since staging takes what he has reviewed out of the way. But it hides what is staged, so he can't see what a commit would take. The message box above the diff, with Commit and Stash below it, goes: both become dialogs.

That frees the panel's left half in Changes and Staged vs unstaged, and the panel holds two file trees there: staged on the left, unstaged on the right, lined up with the split diff's sides. The right one is today's tree, now without the staged files. A partly staged file is in both. The other reviews keep their list on the left and their files on the right.

Keep in mind:

- The staged tree is for seeing and unstaging, and must look it: in Staged vs unstaged its files are often not in the diff. If clicking through turns out to be missed, the likely answer is showing what is staged (`HEAD` vs index) in the diff.
- In Changes every file is in the diff, so both trees can jump to it.
- A failed commit keeps its message today. The dialog must too, and a message shouldn't be lost by closing it.
- Stash takes new files too today. The choices map to `git stash push`'s `--include-untracked` and `--staged` (git 2.35 and later), and the name to `-m`.

Built: each tree's header holds its action, Commit over the staged tree and Stash over the unstaged one. Every row has Unstage or Stage, and a folder's acts on everything under it. The diff's order still comes from a tree of all its files, since the unstaged tree no longer holds them all in Changes. The staged tree marks nothing outside Changes, so it never looks like it moved the diff. Not handled: stashing tracked changes when only new files changed does nothing, silently, as git does.

Tasks:

- [x] Show staged and unstaged files apart, in Changes and Staged vs unstaged
- [x] Commit from a dialog, replacing the message box and its buttons
- [x] Stash from a dialog: name the stash and pick what goes: tracked changes, those and new files, or only what's staged

### Step 4c: Open a repository

Switching repositories is the hard part: comments, the watcher, the loads and all the state belong to one repository. Refactor for it first, before more features build on the one-repository shape.

- **One dialog** for recents and the file system, on Ctrl+O and on the repository name in the bar. Empty, it lists recent repositories, newest first, dropping those that no longer exist. Typing a path like `~/dev/p/diffactive` completes folders and marks which are repositories. Listing folders is IO, so it stays off the UI thread.
- **Start as Zed does, never in the dialog.** Only a folder passed as an argument counts (`diffactive .`), never the current one, so a launcher and a terminal behave alike. A folder inside a repository opens that repository. A folder that isn't in one shows a placeholder: initialise it with one click, or open a project. With no folder, the last repository opens if it still exists. Otherwise an empty window offers "Open a repository" and its hotkey, without init, since nobody picked a folder. When this lands, `cargo run` needs `-- .` to review the current folder: update the commands in `AGENTS.md`.
- **The repository name** reads `<user-or-org>/<project>`, from `origin`'s URL, else the first remote's, else the folder's name.
- **Recents are app state**, in `dirs`' state folder, or its data folder where there is none (macOS). The only state outside `<git-dir>/diffactive/`: add it to the invariants when it lands.

Built: `State` is the open repository's state, all of it but its `Shell`, which holds what outlives a switch. Opening rebuilds `State` around the carried `Shell` (keeping the diff's layout), so nothing of the old repository survives: dropping its loads drops their answers, and dropping the watcher stops its thread. Opening finds the repository on a worker, comments included. It is refused while an action runs, whose outcome would go unseen. The start reads its folder and recents on the main thread before the window opens, as before. A path typed without `/` or `~` starts at the home folder. The name is the last two parts of a remote URL with a host, so a local remote gives the folder's name. Step 4e can grow that parser into host, owner and project.

Tasks:

- [x] Let the app switch repositories
- [x] Open dialog with recents and path completion
- [x] Remember recent repositories, forgetting those that are gone
- [x] Start as Zed does: the folder passed, else the last repository, else an empty window
- [x] Name the repository `<user-or-org>/<project>`

### Step 4d: Branches and remotes

The remote side of everyday work, without a terminal: switching to a colleague's or an agent's branch, fetching, and wiring up a new repository's `origin`.

Keep in mind:

- **One branch switcher**, a dialog, from the branch name in the bar and from the palette. Local branches come first, newest first. After a separator come remote branches that have no local branch. Typing filters both. It creates branches too, as Zed's does: a name that no branch has offers to create it. The New branch dialog goes.
- **A remote-only branch checks out as a local branch tracking it.** `git switch <name>` guesses this only when exactly one remote has the branch. Otherwise pass `--track <remote>/<name>`.
- **Fetch gets every remote and prunes**, so the switcher stops offering branches that were deleted on the remote. It fails on credentials the way pull does.
- **The remotes dialog** lists each remote with its URL. It adds a remote from a name and a URL, with the name defaulting to `origin` when there is none. It removes a remote only after a confirm, because git drops that remote's branches with it. Push already sends a new branch to `origin`.

Tasks:

- [ ] Switch or create a branch from one dialog: local branches, then those only on a remote
- [ ] Check out a branch that only exists on a remote, tracking it
- [ ] Fetch
- [ ] Add and remove remotes from a dialog

### Step 4e: Pull requests

Match a branch to its pull request, and open one, on GitHub and on Forgejo or Gitea. Reviewing a pull request in the app comes later.

Keep in mind:

- **The forge comes from the remote's URL**, the same URL that step 4c parses for the repository's name. github.com means GitHub. Any other host is taken to be Forgejo or Gitea.
- **Opening a pull request goes to the forge's own form, in the browser**: `<host>/<owner>/<repo>/compare/<base>...<branch>` on all three. It needs no auth, and templates, reviewers and labels work out of the box. An in-app dialog waits until it hurts.
- **On GitHub, `gh` finds a branch's pull request**, with `--json` output and its own auth. That makes it a second spawned program, so it gets its own module, as git has.
- **Forgejo and Gitea have no CLI with machine-readable output.** `fj` prints only for people and can't search by branch. Their HTTP API can (`/api/v1/repos/<owner>/<repo>/pulls`), but private repositories need a token. Decided: on Forgejo and Gitea the app only opens pull requests for now. Showing a branch's pull request there waits for an HTTP client and a token setting.

Tasks:

- [ ] Open a pull request for a branch, in the browser
- [ ] Show a branch's pull request on GitHub, and open it in the browser

### Step 4f: UI refinements

Peter dislikes the always-visible top panel. A left sidebar is doubtful too, because side-by-side diffs need the width, so moving the panel to the side is unlikely to be the fix. lazygit's layout is not a reference.

The panel's trouble was a lack of purpose, filled with whatever needed a place, and weak separation from the diff. Step 4b gives it a purpose in Changes and Staged vs unstaged, the staged and unstaged trees, so the fix may be its design rather than its removal.

A first step is done: the panel and the diff are sheets on a darker page, and the diff has a status row of its own, with the summary and Unified/Split. Still open: collapsing the panel, to one line saying what is under review rather than to nothing, with a key.

Tasks:

- [ ] Rework the top panel: a clear purpose in every review, and clear separation from the diff (the bar also overflows narrow windows, which hides Pull and Push)
- [ ] Stop new files wasting half of the split view (may need a Rune issue)
- [ ] Decide what picking the default branch should show (it's empty today)

### Step 4g: Hotkeys

Hotkeys go in once the layout settles, so they're assigned once. Give the frequent actions a key, then add an overlay that lists every key, grouped as the palette groups its commands. The palette already shows the keys the diff has.

Tasks:

- [ ] Assign keys to the frequent actions
- [ ] Overlay listing every key

### Step 5: File explorer

Lets a reviewer read code outside the diff, then get back without losing their place. Rune has no read-only code view, so that part is ours.

Tasks:

- [ ] Browse the tree of the review's target
- [ ] Read a file with syntax highlighting
- [ ] Open the full file from a line in the diff, and return

### Later

Order open, plan each when it comes up: reviewing pull requests on GitHub and Forgejo/Gitea, worktrees (below), LSP (diffle's `lsp/` is the reference), commit graph, hunk and line staging, opening folds in the diff.

### Later: Worktrees

Not urgent. A feature of its own, with its own dialog: list the worktrees with their branch and path, open one, create one, delete one.

Keep in mind:

- **Opening a worktree is opening a repository**, so this builds on step 4c.
- **Creating takes a path and a branch**, new or existing. A branch can be checked out in only one worktree at a time, and git says so.
- **Deleting is destructive.** A worktree with changes or new files needs `--force`, and the confirm must say what gets lost. The main worktree and the one that is open can't be deleted.
- **A worktree has its own git dir** (`.git/worktrees/<name>`), and review state lives there. Deleting a worktree takes its comments with it, so the confirm says that too.
- **The watcher watches only the open worktree's git dir.** Branches and stashes live in the common dir (`git rev-parse --git-common-dir`). It is likely already blind to changes there when a worktree is open.

Tasks:

- [ ] List worktrees and open one
- [ ] Create a worktree on a new or an existing branch
- [ ] Delete a worktree, after a confirm
