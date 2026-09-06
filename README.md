# slide-builder

AI-assisted PowerPoint authoring in your terminal. Linux only.

## Install

You need Rust 1.92+, Kitty or Ghostty for inline previews, and Bubblewrap (`bwrap`) with user namespaces enabled.

From the repository root:

```sh
cargo build --release --locked -j 8
install -Dm755 target/release/slide-builder ~/.local/bin/slide-builder
```

Make sure `~/.local/bin` is on your `PATH`. See `INSTALL.md` for detailed requirements and renderer setup.

## First run

```sh
slide-builder ~/my-deck.pptx
```

Choose a provider, sign in, and select a model when prompted. The path opens an existing deck or creates a new one.

Tell the agent what slides you want. Press `F1` for keyboard help.
