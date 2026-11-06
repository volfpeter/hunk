# 0001: Approach

Status: step 1 done. The always-visible top panel needs rework in a later UI pass. Revised 2026-11-06.

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

## Steps

Every step ends with a runnable app, screenshots and a list of what to try. Work stops there: Peter reviews the UI, we change what needs changing, Peter stages and commits, and only then does the next step start.

1. **Review anything (read-only).** The five reviews, file list, unified and split diff, manual refresh. *Done.*
2. **Comments for agents.** Comment on a line, selection or file; copy open comments as one prompt; resolve.
3. **Git operations.** File-level stage and unstage, commit, checkout, stash, pull, push; live refresh; a command palette over the same actions.
4. **File explorer.** Browse the tree and read code outside the diff.

Later, order open: PR review on GitHub and Forgejo, worktrees, LSP, commit graph.

## Known limits of `rune_kit::diff`

- Folds open only for diffs built from two texts, not parsed patches. Step 1 uses `parse`, so folds stay shut.
- `FileDiff` is `!Send`: patches cross threads as text and are parsed on the UI thread.
- `set_files` collapses folds and clears the selection; matters once refresh is live.
