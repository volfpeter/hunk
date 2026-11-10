# hunk

A simple desktop app for reviewing Git changes, built with [Rune](https://github.com/anomalyco/rune). Review your uncommitted work, commits, branches and stashes, leave comments to hand to a coding agent, and stage, commit and push what's good.

Early: so far it has only been tried on Linux with Wayland.

## Try it

You need [Rust](https://rustup.rs), Git and a GPU with Vulkan support.

```bash
git clone <this repository>
cd hunk
./install.sh
```

It builds the app and adds `hunk` to `~/.local/bin` and to your app launcher. See [Install](docs/how-to-use.md#install) for updating and removing it.

Then open a repository with it:

```bash
hunk path/to/your/repo
```

Without a folder it opens the last repository you had open.

See [how to use it](docs/how-to-use.md).
