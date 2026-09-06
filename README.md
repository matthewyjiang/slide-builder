# slide-builder

AI-assisted PowerPoint authoring in your terminal. Linux only.

## Your decks, your design

Design packages pair PowerPoint templates with a `DESIGN.md` that tells the agent how to use them. Start from a deck you already like, so new slides follow your design instead of a generic theme.

Run `/import-design` and pick a `.pptx`. Slide-builder copies the template and generates its design guide. Use `/design` to select the package, then describe the deck you want. Reuse the same package across decks.

## Install

Early preview. No binary releases yet, so you'll need to build from source.

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

## Slide styling and review

The agent can set text color, font family, size, weight, and alignment when adding
text. Dark slides need an explicit contrasting text color; changing a background
does not automatically recolor existing text.
Text formatting currently requires the standard `p:` and `a:` XML namespace
prefixes; imported files using alternative prefixes return an explicit error.

When the agent calls `render_deck`, the rendered slide images are sent back to the
model automatically for visual review. Use an image-capable model for this workflow.
You can also attach the active slide with `Ctrl+V` to ask about a specific layout.
The preview is an HTML-based rendering, so check the final deck in PowerPoint when
exact Office rendering matters.

## Development checks

GitHub Actions runs on pull requests and pushes to `main`, using the Rust version
in `.mise.toml`. Run the same checks locally:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -j 8 -- -D warnings
cargo test --locked --all-features -j 8
```

A separate workflow runs Actionlint when `.github/workflows/` changes. Both
workflows can also be started manually from GitHub's Actions tab.

CI runs the ordinary test suite, including the direct-worker rejection check.
The ignored renderer and namespace tests require a qualified Bubblewrap
host and are not run in CI. See `INSTALL.md` for the commands to run those checks.
