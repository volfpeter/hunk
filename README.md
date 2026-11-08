# diffactive

A simple desktop app for reviewing Git changes, built with [Rune](https://github.com/anomalyco/rune). Review your uncommitted work, commits, branches and stashes, leave comments to hand to a coding agent, and stage, commit and push what's good.

Early: so far it has only been tried on Linux with Wayland.

## Try it

You need [Rust](https://rustup.rs), Git and a GPU with Vulkan support.

```bash
git clone <this repository>
cd diffactive
cargo install --path .
```

Then open a repository with it:

```bash
diffactive path/to/your/repo
```

Without a folder it opens the last repository you had open.

See [how to use it](docs/how-to-use.md).
