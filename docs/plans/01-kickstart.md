# 0001: Approach

Status: step 4f done, waiting for Peter's UI review. See [Tasks](#tasks).

## Intent

A simple desktop app for reviewing code, mostly written by agents, with the git features that review needs. Not a lazygit replacement: lazygit is a bucket of features, most of which we won't want.

Start simple and iterate. Product scope stays small and uncommon cases wait until they hurt, but the code should be great.

## The model

A **Review** is a comparison between two points. There are five:

| Review             | Compares                                         |
| ------------------ | ------------------------------------------------ |
| Changes            | `HEAD` vs worktree, with untracked files         |
| Staging            | index vs worktree, with untracked files          |
| Commit             | first parent vs commit                           |
| Branch             | merge-base with its parent vs branch tip         |
| Stash              | its parent vs the stash                          |

A pull request will be a sixth: same comparison as a branch, with comments going to the forge.

## UI direction

Side-by-side diffs need the width, so nothing sits beside the diff. A bar picks the kind of review, a panel below it holds the list and the changed files, and the diff takes the rest.

```
┌───────────────────────────────────────────────────────────┐
│ repo main  [Changes|Staging|Commits|…]  ⟳ ↓ ↑   │
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
- **Patches carry each file whole, and Rune folds them.** So every fold opens without reading files, and git still makes the diff. Parsing whole files costs the UI thread, felt only on huge ones.
- **A branch is compared with its parent**, the other local branch it has the fewest commits beyond, or the default branch, which also wins ties. The default branch is the local one named after `origin`'s, so `origin/` shows only when there is no such local branch: it read like a branch prefix. A local default branch that is behind `origin` can pull its missing commits into the diff, which pulling fixes. Every diff's title sets its two points apart with an icon of two opposite arrows, older first, as an arrow from a base to its branch read like a hierarchy. Git doesn't record which branch another grew from, and guessing beats bookkeeping that misses branches made outside the app. Comparing with the default branch mixed a stacked branch's parent into it. The diff's title names the base, so a wrong guess shows.
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

Peter reviews agent-written code, comments on what's wrong, hands the comments to an agent, then reviews the follow-up. A typical round: stage what's fine, comment on the rest, and use Staging to see only what the agent changed next. That is one example, not the focus: comments work in every review.

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
- Discard means what the review shows: back to `HEAD` in Changes, back to the staged version in Staging.
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

### DONE - Step 4b: Commit and stash

Staging is the review Peter finds most useful, since staging takes what he has reviewed out of the way. But it hides what is staged, so he can't see what a commit would take. The message box above the diff, with Commit and Stash below it, goes: both become dialogs.

That frees the panel's left half in Changes and Staging, and the panel holds two file trees there: staged on the left, unstaged on the right, lined up with the split diff's sides. The right one is today's tree, now without the staged files. A partly staged file is in both. The other reviews keep their list on the left and their files on the right.

Keep in mind:

- The staged tree is for seeing and unstaging, and must look it: in Staging its files are often not in the diff. If clicking through turns out to be missed, the likely answer is showing what is staged (`HEAD` vs index) in the diff.
- In Changes every file is in the diff, so both trees can jump to it.
- A failed commit keeps its message today. The dialog must too, and a message shouldn't be lost by closing it.
- Stash takes new files too today. The choices map to `git stash push`'s `--include-untracked` and `--staged` (git 2.35 and later), and the name to `-m`.

Built: each tree's header holds the action matching its rows, Unstage all over the staged tree and Stage all over the unstaged one. Commit and Stash were there first, but didn't match the trees below them: they live in the palette now, and get keys in step 4h. Every row has Unstage or Stage, and a folder's acts on everything under it. The diff's order still comes from a tree of all its files, since the unstaged tree no longer holds them all in Changes. The staged tree marks nothing outside Changes, so it never looks like it moved the diff. Not handled: stashing tracked changes when only new files changed does nothing, silently, as git does.

Tasks:

- [x] Show staged and unstaged files apart, in Changes and Staging
- [x] Commit from a dialog, replacing the message box and its buttons
- [x] Stash from a dialog: name the stash and pick what goes: tracked changes, those and new files, or only what's staged

### DONE - Step 4c: Open a repository

Switching repositories is the hard part: comments, the watcher, the loads and all the state belong to one repository. Refactor for it first, before more features build on the one-repository shape.

- **One dialog** for recents and the file system, on Ctrl+O and on the repository name in the bar. Empty, it lists recent repositories, newest first, dropping those that no longer exist. Typing a path like `~/dev/p/hunk` completes folders and marks which are repositories. Listing folders is IO, so it stays off the UI thread.
- **Start as Zed does, never in the dialog.** Only a folder passed as an argument counts (`hunk .`), never the current one, so a launcher and a terminal behave alike. A folder inside a repository opens that repository. A folder that isn't in one shows a placeholder: initialise it with one click, or open a project. With no folder, the last repository opens if it still exists. Otherwise an empty window offers "Open a repository" and its hotkey, without init, since nobody picked a folder. When this lands, `cargo run` needs `-- .` to review the current folder: update the commands in `AGENTS.md`.
- **The repository name** reads `<user-or-org>/<project>`, from `origin`'s URL, else the first remote's, else the folder's name.
- **Recents are app state**, in `dirs`' state folder, or its data folder where there is none (macOS). The only state outside `<git-dir>/hunk/`: add it to the invariants when it lands.

Built: `State` is the open repository's state, all of it but its `Shell`, which holds what outlives a switch. Opening rebuilds `State` around the carried `Shell` (keeping the diff's layout), so nothing of the old repository survives: dropping its loads drops their answers, and dropping the watcher stops its thread. Opening finds the repository on a worker, comments included. It is refused while an action runs, whose outcome would go unseen. The start reads its folder and recents on the main thread before the window opens, as before. A path typed without `/` or `~` starts at the home folder. The name is the last two parts of a remote URL with a host, so a local remote gives the folder's name. Step 4f can grow that parser into host, owner and project.

Tasks:

- [x] Let the app switch repositories
- [x] Open dialog with recents and path completion
- [x] Remember recent repositories, forgetting those that are gone
- [x] Start as Zed does: the folder passed, else the last repository, else an empty window
- [x] Name the repository `<user-or-org>/<project>`

### DONE - Step 4d: Branches and remotes

The remote side of everyday work, without a terminal: switching to a colleague's or an agent's branch, fetching, and wiring up a new repository's `origin`.

Keep in mind:

- **One branch switcher**, a dialog, from the branch name in the bar and from the palette. Local branches come first, newest first. After a separator come remote branches that have no local branch. Typing filters both. It creates branches too, as Zed's does: a name that no branch has offers to create it. The New branch dialog goes.
- **A remote-only branch checks out as a local branch tracking it.** `git switch <name>` guesses this only when exactly one remote has the branch. Otherwise pass `--track <remote>/<name>`.
- **Fetch gets every remote and prunes**, so the switcher stops offering branches that were deleted on the remote. It fails on credentials the way pull does.
- **The remotes dialog** lists each remote with its URL. It adds a remote from a name and a URL, with the name defaulting to `origin` when there is none. It removes a remote only after a confirm, because git drops that remote's branches with it. Push already sends a new branch to `origin`.

Built: the branch name in the bar opens the switcher, and the Branches review lost its New branch button. A remote-only branch always checks out with `--track` and the full ref, so there is no guessing and no clash with a local branch named `<remote>/<branch>`. The switcher shows ten branches and how many more match, highlights the first match rather than the new branch, and offers no new branch for a name with spaces. The current branch is listed, and picking it only closes the switcher. Fetch and Remotes sit in the switcher's footer, since the bar already overflows (step 4g). Fetch is offered only with a remote. The Branches review selects the current branch when it opens, and moves to the branch switched to, from the app or a terminal. An empty name field means `origin` unless a remote has that name. Adding doesn't fetch. Not handled: a remote named with a leading `-` can be added, and then breaks fetching every remote.

Tasks:

- [x] Switch or create a branch from one dialog: local branches, then those only on a remote, on Ctrl+B and Ctrl+Shift+B
- [x] Check out a branch that only exists on a remote, tracking it
- [x] Fetch, on Ctrl+Shift+F
- [x] Add and remove remotes from a dialog

### DONE - Step 4e: Bar and Comments page

The bar mixed reviews with something that isn't one, and named its actions badly. The Comments tab showed one comment at a time, inside a review's diff, beside a file tree that meant nothing there.

Bar:

- **Reviews in the order of their reach**: Staging, Changes, Branches, Commits, Stashes. Staging is part of Changes, which is part of the branch. Commits splits the branch into pieces, and stashes sit off to the side. Staging is the default, on start and on switching repositories.
- **Comments leaves the segmented control**, since it compares nothing. It becomes a button on the right with the open count. While the page is open no segment is selected; a segment, or the button again, goes back to a review. The control also gets much narrower, as every segment is as wide as the widest label, which was "Comments (n)".
- **Icons on the right, with tooltips and shortcuts**: Commit, Pull and Push. Pull and Push keep their counts beside the icon, the only thing on them worth reading. Commit, the main action of the Staging workflow and so far only in the palette, gets a label too, so it reads as the main action. Our own SVG icons, as Rune has none for git. Reversed in step 4f: the right of the bar is icons and counts only.
- **Refresh leaves the bar.** The watcher refreshes by itself, and the palette keeps Refresh. The watcher's error message must point there.

Comments page:

- **One sheet in place of the panel and the diff**: a `DiffView` with an entry per comment, open first, then outdated, then resolved. Rows are keyed by position, so entries can share a path, and the real path keeps syntax highlighting.
- **Each entry shows the code it was written on**: a snippet of the diff with context above and below, and the comment card under its last line, the same card as in the diff. The header has the location, review and badges, with Copy and Show in diff. Show in diff opens its review at the comment.
- **Snippets are saved with the comment**, when it is written. They need no git and no file reads, and outdated comments still show what they were about. A snippet is what was commented on, so its line numbers can differ from the header's location once the code moves, as on GitHub. Comments written before this have no snippet and show only their lines.
- **Resolved comments start collapsed**, through `DiffView`'s chevron. Rebuilding the entries resets that, so keep what the reader opened or closed.
- **Copy all skips outdated comments**, and its label counts what it copies: "Copy 3 open comments". A single comment, outdated or not, copies from its own header. "Copy prompt" goes everywhere, palette included.
- To check: a whole-file comment has no lines; it needs a sensible entry.

Unified or Split becomes one setting for every diff, kept across restarts. That is app state beside the recent repositories: update the invariant when it lands.

Built: Comments is a page, not a review: the kind under it stays, so the button goes back to it. Copy all left the bar, for the page's header and the palette, as the bar overflows (step 4g) and the page shows what gets copied. Each entry's header holds its lines now, the review it was written in, its status, Copy and Show in diff, so the card under it drops its location and badge and keeps its actions beside its text. Show in diff is offered only while the review is on offer: a commit beyond the list, a dropped stash or a deleted branch has none. A snippet is three rows above and below the lines, a removed line counting as one, within one hunk unless the diff holds the whole text. A whole-file comment keeps the first seven lines of its first hunk, and its card goes under the last. A comment with neither a snippet nor lines, a whole-file one from before this, gets no entry and shows above the entries. Rune opens a snippet that doesn't start at line 1 with a gap row counting the lines before it (see `docs/rune-quirks.md`). The page rebuilds only when a comment comes, goes or changes status, and an entry keeps what the reader opened or closed while it stays resolved or open.

Tasks:

- [x] Reorder the reviews, Staging first and the default
- [x] Comments as a button on the right of the bar, and icons for Commit, Pull and Push; Refresh only in the palette
- [x] Unified or Split as one setting for every diff, kept across restarts
- [x] Save a snippet of the diff with each new comment
- [x] Comments page: every comment with its snippet, open, outdated, then resolved, resolved collapsed
- [x] Copy one comment, and copy every open comment that isn't outdated

### DONE - Step 4f: Pull requests

Match a branch to its pull request, and open one, on GitHub and on Forgejo or Gitea. Reviewing a pull request in the app comes later.

Keep in mind:

- **The forge comes from the remote's URL**, the same URL that step 4c parses for the repository's name. github.com means GitHub. Any other host is taken to be Forgejo or Gitea.
- **Opening a pull request goes to the forge's own form, in the browser**: `<host>/<owner>/<repo>/compare/<base>...<branch>` on all three. The base is the branch's parent, as the Branches review finds it, so a stacked branch's pull request shows only its own commits. It needs no auth, and templates, reviewers and labels work out of the box. An in-app dialog waits until it hurts.
- **On GitHub, `gh` finds a branch's pull request**, with `--json` output and its own auth. That makes it a second spawned program, so it gets its own module, as git has.
- **Forgejo and Gitea have no CLI with machine-readable output.** `fj` prints only for people and can't search by branch. Their HTTP API can (`/api/v1/repos/<owner>/<repo>/pulls`), but private repositories need a token. Decided: on Forgejo and Gitea the app only opens pull requests for now. Showing a branch's pull request there waits for an HTTP client and a token setting.

Built: the pull request sits in the Branches review's header, beside Check out, for the selected branch: its number, title and status, or Create pull request. The repository is the one the bar names, `origin`'s first, as Push publishes there too. Branches go by their local names, and a base that is only on a remote loses its remote's prefix. A base of `HEAD` leaves the forge to pick its default branch. `gh` gets the repository explicitly (`--repo host/owner/repo`), so it never guesses or prompts, and the open pull request wins over newer closed ones; other repositories' branches of the same name don't count. The bar shows the current branch's pull request too, left of Commit: an icon and its number, the rest in the tooltip, and nothing when there is none, as the bar overflows (step 4g). The icon's shape says where the pull request stands, as GitHub's do: open, draft (its line dashed), merged, closed (its arrow crossed out). Shape rather than colour, since green and red mean added and removed lines here, and colour alone fails colour-blind readers. Only an open one is in full ink; a draft, merged or closed one is muted, as it asks for no attention.

That made the right of the bar uniform: every button is an icon, with a count when there is something to count, and its tooltip says what the count is. Comments and Commit lost their labels, reversing step 4e's decision to label Commit so it reads as the main action: its count now does that job, showing up exactly when something is ready to commit. Commit counts staged files only, not what a commit with nothing staged would take: that count would drop as files get staged, which looks broken while staging. With nothing staged it shows no count but still commits everything, and its dialog says so first. The bar now fits a window 900 points wide. Answers are kept by branch, and `gh` runs only for a branch not asked about yet, not on the watcher's refreshes, since agents' edits would call the API all day. The window regaining focus, Refresh from the palette and Fetch ask again for the current and the selected branch, keeping the old answers on screen meanwhile. Until `gh` answers, nothing shows on GitHub. When it can't look, Create pull request shows, its tooltip saying why. Not handled: a branch whose name differs on its remote, an ssh host alias (`git@github-work:`), which reads as Forgejo, and GitHub Enterprise.

Tasks:

- [x] Open a pull request for a branch, in the browser
- [x] Show a branch's pull request on GitHub, and open it in the browser

### Step 4g: UI refinements

Peter dislikes the always-visible top panel. A left sidebar is doubtful too, because side-by-side diffs need the width, so moving the panel to the side is unlikely to be the fix. lazygit's layout is not a reference.

The panel's trouble was a lack of purpose, filled with whatever needed a place, and weak separation from the diff. Step 4b gives it a purpose in Changes and Staging, the staged and unstaged trees, so the fix may be its design rather than its removal.

A first step is done: the panel and the diff are sheets on a darker page, and the diff has a status row of its own, with the summary and Unified/Split. Still open: collapsing the panel, to one line saying what is under review rather than to nothing, with a key.

Tasks:

- [ ] Rework the top panel: a clear purpose in every review, and clear separation from the diff (the bar also overflows narrow windows, which hides Pull and Push; step 4f's icons and counts made it fit 900 points)
- [ ] Stop new files wasting half of the split view (may need a Rune issue)
- [ ] Decide what picking the default branch should show (it's empty today)

### Step 4h: Hotkeys

Hotkeys go in once the layout settles, so they're assigned once. Give the frequent actions a key, then add an overlay that lists every key, grouped as the palette groups its commands.

Decided, so a new key can be guessed from its kind:

- **Ctrl+key opens a picker or switcher**, changing nothing until you pick: K the palette, O a repository, B the branches (step 4d), later P a file in the diff.
- **Ctrl+Shift+key runs a git operation**: C commit, S stash, ↓ pull, ↑ push, B branches as an alias of Ctrl+B, F fetch (step 4d). Ctrl+Shift+P stays free: it was the palette's alias, and a VS Code habit shouldn't land on a git operation.
- **Ctrl+Enter confirms a dialog**, and with none open starts a commit.
- **Bare keys act on what has focus**: moving through the diff, and `s` / `u` to stage and unstage.

Built: a key runs the palette command showing it, so it works exactly when the palette offers it. Git keys do nothing while a dialog or the palette is open. A text field keeps the keys it uses, so Ctrl+Shift+↑ in a comment selects text rather than pushing.

Committing with nothing staged commits everything, as most apps do, new files that aren't ignored included. That made a commit key that did nothing when nothing was staged look broken. The commit dialog shows what will happen, so nothing goes in unseen:

- **Split side by side**: a summary on the left, the message and Commit on the right. The dialog widens so a message still shows about 72 columns.
- **The summary describes the commit, not the index**, so "staged" never appears: "This commit" counts new, changed and deleted files, hiding empty rows. Status runs with `--no-renames`, so a rename counts as one deleted and one new. "New" catches a stray file nobody meant to commit.
- **"Stays uncommitted" shows only when something is staged.** Its "partly committed" row, in the warning colour, counts files the commit takes only part of, as that often means something was forgotten. A partly committed file counts in "This commit" too, as its staged part goes in. "Not included" counts the rest.
- **One Commit button**: with nothing staged it reads "Commit all N files", stages everything, then commits. Git has no single command for that with new files, so a commit that fails, say on a hook, leaves everything staged.
- Later, each row may open into its files.

Tasks:

- [x] Keys for commit, stash, pull and push
- [x] Commit everything when nothing is staged, with the dialog summarising what the commit takes
- [ ] `s` / `u` stage and unstage the focused item: a row in either file tree, else the diff's file
- [ ] Overlay listing every key

### Step 5: File explorer

Lets a reviewer read code outside the diff, then get back without losing their place. Rune has no read-only code view, so that part is ours.

Tasks:

- [ ] Browse the tree of the review's target
- [ ] Read a file with syntax highlighting
- [ ] Open the full file from a line in the diff, and return

### Later

Order open, plan each when it comes up: reviewing pull requests on GitHub and Forgejo/Gitea, worktrees (below), LSP (diffle's `lsp/` is the reference), commit graph, hunk and line staging.

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
