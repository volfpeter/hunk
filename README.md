# diffactive

A simple desktop app for reviewing Git changes, built with [Rune](https://github.com/anomalyco/rune).

Early and read-only: it shows diffs but doesn't change your repository yet. So far it has only been tried on Linux with Wayland.

## What you can review

- **Changes:** everything not committed yet
- **Staged vs unstaged:** what changed since you last staged
- **Commits:** any of the latest 200 on the current branch
- **Branches:** what a branch added since it left the default branch
- **Stashes:** what a stash holds

## Try it

You need [Rust](https://rustup.rs), Git and a GPU with Vulkan support.

```bash
git clone <this repository>
cd diffactive
cargo install --path .
```

Then run it from any repository:

```bash
cd path/to/your/repo
diffactive
```

The file list and diff don't update on their own yet: press **Refresh** to see new changes.
