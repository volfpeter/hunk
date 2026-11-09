# hunk

A review-first Git desktop app, built in Rust on [Rune](https://github.com/anomalyco/rune). `hunk` is a working name: keep it in one constant.

Kickstart Plan and task list: `docs/plans/01-kickstart.md`. Read it before starting work.

## Product rules

- Everything is a **Review**: a comparison between two points. The diff is the centre of the app.
- Simple and intuitive beats dense. Major actions get a visible control where they have a good place; the rest can live in menus and the palette. lazygit is only a bucket of features, most of which we do not want: never copy its layout or key-driven handling.
- Keep the width for the diff: side-by-side needs it. Lists sit above the diff, not beside it.
- Start simple. Skip uncommon cases until they hurt; iterate from something that runs. Product scope stays small, code quality does not.
- Decisions in these docs and the plans are the best answer so far, not a spec. Use the best tool for what we need: Rune's components first, and a new dependency when it earns its place, with the reason recorded where the decision lives.

## Workflow

- Take the next open task in the plan. Its step's context says what you need to get started.
- Mark a task done in the plan once it works and is verified. Add anything later tasks need to know to that step's context.
- Add what a person can now do to `docs/features.md`, unticked. Only Peter ticks it.
- When a step's last task is done, stop with a runnable app, screenshots and a list of what to try. Peter reviews the UI before the next step starts.
- Fix a problem everywhere it applies, not only where you noticed it. Record Rune traps in `docs/rune-quirks.md`.
- Agent docs hold only what the code can't tell: intent, decisions and traps. Leave out what an agent can find by reading, and anything routine commits would make stale, like names and exact values.
- Never stage or commit unless Peter asks.

## Layout

- `docs/plans/` holds numbered plans and decisions. `docs/how-to-use.md` is the user guide: usage goes there, not in the README. `docs/features.md` lists what the app does, for Peter to try. `docs/rune-quirks.md`: read before UI work.
- `research/`: gitignored, may be missing. Reference checkouts of `anomalyco/rune`, `moritzwilksch/diffle`, `pierrecomputer/pierre` and `jesseduffield/lazygit` (clone what you need), spikes, and `demo-repo/` for trying the app. Read them, never depend on them.

## Commands

- `cargo test`, `cargo clippy --all-targets`
- `cargo run -- .` reviews the repository in the current folder. `cargo run` alone opens the last repository.

## Rune

- Pin a rev of its `dev` branch; it is pre-release.
- Rune takes issues, not pull requests.
- Check UI work headlessly with the built binary, on a repository: `hunk <folder> --script steps.json --snap out.png --at mark:end`, or `--tree out.json` for element keys. Point `XDG_STATE_HOME` at a fresh scratch folder each run, or runs land in Peter's recent repositories, and the review kind left by the last run changes what the script taps. Data loads in the background, so script a wait ending in a `mark` rather than a bare `--snap`.
- Sources of truth: the READMEs and kit examples in `research/rune`.

## Invariants

- One module spawns git, and one spawns `gh`. Explicit argv, NUL-delimited output, pinned `-c` overrides so user config cannot change the output shape (`diff.mnemonicPrefix` does, for one).
- No git, `gh` or file IO on the UI thread. Workers return plain data; `rune_kit::diff` types hold `Rc` and are not `Send`, so they are built on the UI thread.
- Review state lives under `<git-dir>/hunk/`. The only app state, the recent repositories and the settings, lives in `dirs`' state folder, or its data folder where there is none.
