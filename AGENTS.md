# diffactive

A review-first Git desktop app, built in Rust on [Rune](https://github.com/anomalyco/rune). `diffactive` is a working name: keep it in one constant.

Plan and step status: `docs/plans/0001-approach.md`.

## Product rules

- Everything is a **Review**: a comparison between two points. The diff is the centre of the app.
- Simple and intuitive beats dense. Visible controls first; shortcuts are an extra. lazygit is only a bucket of features, most of which we do not want: never copy its layout or key-driven handling.
- Keep the width for the diff: side-by-side needs it. Lists sit above the diff, not beside it.
- Start simple. Skip uncommon cases until they hurt; iterate from something that runs. Product scope stays small, code quality does not.
- Stack stays simple: built-in Rune components and syntax highlighting, no extra frameworks.

## Workflow

- Work one plan step at a time. Each ends with a runnable app and screenshots, then stop for UI review.
- Never stage or commit. The reviewer does that after approving a step.

## Layout

- `src/git.rs`: the only module that spawns git. `src/review.rs`: the comparisons. `src/overview.rs`: commits, branches, stashes. `src/app.rs`: state. `src/view.rs`: the window.
- `docs/`: tracked work records. `docs/plans/` holds numbered plans and decisions.
- `research/`: gitignored reference checkouts (rune, diffle, pierre, lazygit), spikes and a demo repo. Read them, never depend on them.

## Commands

- `cargo test`, `cargo clippy --all-targets`
- `cargo run` reviews the repository in the current folder.

## Rune

- Pin a rev of its `dev` branch; it is pre-release. Vetted so far: `781e2b6d87cd5f32d6f4365956fad4baaf24ee50`.
- Rune takes issues, not pull requests.
- Check UI work headlessly: `--snap out.png --size WxH --theme dark`, `--script keys.json --at mark:end`, `--tree`.
- Sources of truth: `research/rune/crates/rune/README.md`, `research/rune/crates/rune-kit/README.md`, and `research/rune/crates/rune-kit/examples/`.

## Invariants

- One module spawns git. Explicit argv, NUL-delimited output, pinned `-c` overrides so user config cannot change the output shape (`diff.mnemonicPrefix` does, for one).
- No git or file IO on the UI thread. Workers return plain data; `rune_kit::diff` types hold `Rc` and are not `Send`, so they are built on the UI thread.
- Review state lives under `<git-dir>/diffactive/`.
