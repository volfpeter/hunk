# diffactive

A simple desktop app for reviewing Git changes, built with [Rune](https://github.com/anomalyco/rune). Review your uncommitted work, commits, branches and stashes, and leave comments to hand to a coding agent.

Early: it shows diffs and keeps review comments, but doesn't change your repository yet. So far it has only been tried on Linux with Wayland.

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

See [how to use it](docs/how-to-use.md).
