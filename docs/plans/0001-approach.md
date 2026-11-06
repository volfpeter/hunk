# 0001: Approach

Status: step 2 in Peter's UI review, editing and deleting comments still to do. See [Tasks](#tasks).

## Intent

A simple desktop app for reviewing code, mostly written by agents, with the git features that review needs. Not a lazygit replacement: lazygit is a bucket of features, most of which we won't want.

Start simple and iterate. Product scope stays small and uncommon cases wait until they hurt, but the code should be great.

## The model

A **Review** is a comparison between two points. There are five:

| Review | Compares |
|---|---|
| Changes | `HEAD` vs worktree, with untracked files |
| Staged vs unstaged | index vs worktree, with untracked files |
| Commit | first parent vs commit |
| Branch | merge-base with the default branch vs branch tip |
| Stash | its parent vs the stash |

A pull request will be a sixth: same comparison as a branch, with comments going to the forge.

## UI direction

Side-by-side diffs need the width, so nothing sits beside the diff. A bar picks the kind of review, a panel below it holds the list and the changed files, and the diff takes the rest.

```
┌───────────────────────────────────────────────────────────┐
│ repo main  [Changes|Staged vs unstaged|Commits|…]  Split ⟳ │
├─────────────────────────────┬─────────────────────────────┤
│ commits, branches or stashes│ changed files                │
├─────────────────────────────┴─────────────────────────────┤
│ diff, full width                                           │
└───────────────────────────────────────────────────────────┘
```

## Decisions

- **Rune's own components throughout**, including `rune_kit::diff` and its syntax highlighting. Pierre is DOM-only and Rune has no webview.
- **Shell out to `git`.** User config, hooks and credentials come for free. Blocking `std::process` on workers, no async runtime.
- **One crate** until something forces a split.
- **File-level operations only** for now.
- **Comments go to agents first.** Each stores path, side, line range and commit, enough to submit to a forge later. No destination abstraction until then.

## Tasks

Work top to bottom, skipping sections marked as waiting. Each step opens with what a new session needs to know. Tasks say what to achieve, not how to build it.

- Mark a task `[x]` when it works and is verified.
- If it taught you something later tasks need, add it to that step's context.
- When a step's last task is done, stop for Peter's UI review. Don't start the next step until Peter approves.

### Step 1: Review anything

Done. The five reviews, file list, unified and split diff, manual refresh, all read-only.

- [x] Show all five reviews from real repository data
- [x] Unified and split diff, split by default
- [x] Changed files list that jumps to a file in the diff
- [x] Work in a repository without commits

### Step 2: Comments for agents

Peter reviews agent-written code, comments on what's wrong, hands the comments to an agent, then reviews the follow-up. A typical round: stage what's fine, comment on the rest, and use Staged vs unstaged to see only what the agent changed next. That is one example, not the focus: comments work in every review.

Today comments go to agents as a copied prompt. Eventually they will be submitted to GitHub or Forgejo PRs, so each one stores path, side, line range and commit. Don't build a destination abstraction yet.

Starting points:

- `DiffView` already has `on_comment` (a "+" by the hovered line, or the current selection), `annotation` (app widgets under a line, rebuilt every commit so a composer can read state) and `header_actions` (a per-file slot).
- Typing into a text field inside an annotation row is untested. Check it first.
- diffle does all of this in a browser, so use it as the behaviour reference: `research/diffle/crates/diffle/src/comments/` for anchoring, relocation, storage and prompt format.
- Store state under `<git-dir>/diffactive/`.

How it works:

- Comments live in `src/comments.rs` and `<git-dir>/diffactive/comments.json`, one list per repository. An unreadable file is left alone: the error shows and nothing is saved over it.
- Each comment keeps the review it was written in and quotes its lines. Refresh looks for the quote in that review's side (`Review::file`), nearest the old line. Finding it moves the comment; not finding it marks it outdated, and it recovers if the code comes back.
- Placement in the diff on screen is separate: any review whose hunks hold the quote shows the comment, whichever review it came from.
- Comments is a sixth kind in the bar. Picking one loads its own review and scrolls to it. If that diff can't show it, the card sits above the diff.
- Typing in an annotation works. `TextArea` is 280 points wide until `.width(None)`, and a plain `TextEdit` submits on Enter and takes a new line on Shift+Enter.

Tasks:

- [x] Write a comment on a line or a selected range
- [x] Write a comment on a whole file
- [x] Keep comments across restarts
- [x] Keep comments attached as the code around them changes
- [x] List comments whose code is gone as outdated, rather than losing them
- [x] Show open comments from any review, so comments made before staging stay reachable
- [x] Copy all open comments as one prompt for an agent
- [x] Resolve a comment
- [ ] Edit a comment
- [ ] Delete a comment

### Step 3: Git operations

File-level only: no hunk or line staging for now. Only what normal review work needs. Actions are visible buttons, and the palette is a shortcut to them. Confirm anything destructive.

Starting points:

- Everything is read-only today. Git still goes through `src/git.rs`, and writes stay off the UI thread like reads.
- `DiffState::set_files` collapses folds and clears the selection. Agents edit while Peter reads, so a refresh must not yank the view.
- `State::refresh` also finds comments' code again, so refreshing by itself keeps comments attached for free. It reads every open comment's file, so a watcher should debounce.
- Stash entries are identified by short hash, not `stash@{n}`, which shifts as stashes come and go.
- Git runs with no stdin, so a credential prompt fails instead of hanging. Show git's error rather than handling auth.
- `header_actions` is the natural place for a file's stage button. `rune_kit` has `Dialog`, `Combobox`, `Toast` and `TextArea`.

Tasks:

- [ ] Refresh by itself when the repository changes
- [ ] Stage and unstage a file
- [ ] Commit, with a message
- [ ] Discard a file's changes
- [ ] Check out a branch and create one
- [ ] Stash, and apply, pop or drop a stash
- [ ] Pull and push
- [ ] Command palette over the same actions

### Step 4: File explorer

Lets a reviewer read code outside the diff, then get back without losing their place. Rune has no read-only code view, so that part is ours: `virtual_rows` plus `rune_kit::syntax`. The kit's `Tree` and `FileTree` cover browsing.

Tasks:

- [ ] Browse the tree of the review's target
- [ ] Read a file with syntax highlighting
- [ ] Open the full file from a line in the diff, and return

### UI pass

Waiting: start only when Peter asks. Peter dislikes the always-visible top panel. A left sidebar is doubtful too, because side-by-side diffs need the width, so moving the panel to the side is unlikely to be the fix. lazygit's layout is not a reference.

Tasks:

- [ ] Replace the always-visible top panel
- [ ] Stop new files wasting half of the split view (may need a Rune issue)
- [ ] Make the file list and diff use the same order
- [ ] Decide what picking the default branch should show (it's empty today)

### Later

Order open, plan each when it comes up: PR review on GitHub and Forgejo, worktrees, LSP (diffle's `lsp/` is the reference), commit graph, hunk and line staging, opening folds in the diff.

## Known limits of `rune_kit::diff`

- Folds open only for diffs built from two texts, not parsed patches. Step 1 uses `parse`, so folds stay shut.
- `FileDiff` is `!Send`: patches cross threads as text and are parsed on the UI thread.
- `set_files` collapses folds and clears the selection; matters once refresh is live.
