# diffactive

A review-first Git desktop app, built in Rust on [Rune](https://github.com/anomalyco/rune). `diffactive` is a working name: keep it in one constant.

Plan and task list: `docs/plans/0001-approach.md`. Read it before starting work.

## Product rules

- Everything is a **Review**: a comparison between two points. The diff is the centre of the app.
- Simple and intuitive beats dense. Visible controls first; shortcuts are an extra. lazygit is only a bucket of features, most of which we do not want: never copy its layout or key-driven handling.
- Keep the width for the diff: side-by-side needs it. Lists sit above the diff, not beside it.
- Start simple. Skip uncommon cases until they hurt; iterate from something that runs. Product scope stays small, code quality does not.
- Stack stays simple: built-in Rune components and syntax highlighting, no extra frameworks.

## Workflow

- Take the next open task in the plan. Its step's context says what you need to get started.
- Mark a task done in the plan once it works and is verified. Add anything later tasks need to know to that step's context.
- When a step's last task is done, stop with a runnable app, screenshots and a list of what to try. Peter reviews the UI before the next step starts.
- Never stage or commit unless Peter asks.

## Layout

- `src/git.rs`: the only module that spawns git. `src/review.rs`: the comparisons. `src/overview.rs`: commits, branches, stashes. `src/comments.rs`: review comments. `src/app.rs`: state. `src/view.rs`: the window.
- `docs/`: tracked work records. `docs/plans/` holds numbered plans and decisions. `docs/how-to-use.md` is the user guide: usage goes there, not in the README.
- `research/`: gitignored, may be missing. Reference checkouts of `anomalyco/rune`, `moritzwilksch/diffle`, `pierrecomputer/pierre` and `jesseduffield/lazygit` (clone what you need), spikes, and `demo-repo/` for trying the app. Read them, never depend on them.

## Commands

- `cargo test`, `cargo clippy --all-targets`
- `cargo run` reviews the repository in the current folder.

## Rune

- Pin a rev of its `dev` branch; it is pre-release. Vetted so far: `781e2b6d87cd5f32d6f4365956fad4baaf24ee50`.
- Rune takes issues, not pull requests.
- Check UI work headlessly with the built binary, from a repository: `--script steps.json --snap out.png --at mark:end --size 1500x950 --theme dark`, or `--tree out.json` for element keys. Data loads in the background, so wait for it with a script rather than a bare `--snap`: `[{"t":0.4,"type":"tap","key":"#2/segment1"},{"t":1.2,"type":"mark","name":"end"}]`.
- The diff's "+" only takes a press once its row is hovered: `move` to the row in an earlier step, then `tap` the `plus` key.
- Sources of truth: `research/rune/crates/rune/README.md`, `research/rune/crates/rune-kit/README.md`, and `research/rune/crates/rune-kit/examples/`.

## Invariants

- One module spawns git. Explicit argv, NUL-delimited output, pinned `-c` overrides so user config cannot change the output shape (`diff.mnemonicPrefix` does, for one).
- No git or file IO on the UI thread. Workers return plain data; `rune_kit::diff` types hold `Rc` and are not `Send`, so they are built on the UI thread.
- Review state lives under `<git-dir>/diffactive/`.
