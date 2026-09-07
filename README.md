# slide-builder

AI-assisted PowerPoint authoring in your terminal. Linux; macOS qualification is in progress.

## Your decks, your design

Design packages pair PowerPoint templates with a `DESIGN.md` that tells the agent how to use them. Start from a deck you already like, so new slides follow your design instead of a generic theme.

Run `/import-design` and pick a `.pptx`. Slide-builder copies the template and generates its design guide. Use `/design` to select the package, then describe the deck you want. Reuse the same package across decks.

## Install

Early preview. No binary releases yet, so you'll need to build from source.

You need Rust 1.92+ and Kitty or Ghostty for inline previews. Linux defaults to
Obscura and needs Bubblewrap (`bwrap`) with user namespaces enabled. Obscura remains
the primary renderer on every platform. See `docs/macos.md` for the pending native
macOS backend and the explicitly selected Chromium compatibility path.

From the repository root:

```sh
cargo build --release --locked -j 8
mkdir -p ~/.local/bin
install -m755 target/release/slide-builder ~/.local/bin/slide-builder
```

Make sure `~/.local/bin` is on your `PATH`. See `INSTALL.md` for detailed requirements and renderer setup.

## First run

```sh
slide-builder ~/my-deck.pptx
```

Choose a provider, sign in, and select a model when prompted. The path opens an existing deck or creates a new one.

Tell the agent what slides you want. Press `F1` for keyboard help.

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

CI runs the ordinary test suite on Linux and macOS, including the direct-worker rejection check.
The macOS job also runs real Obscura isolation, capture, and PDF export checks,
plus explicitly selected Chromium compatibility checks. The ignored Linux
namespace tests require a qualified Bubblewrap host and are not run in CI.
See `INSTALL.md` for the commands to run those checks.
