# slide-builder

Linux terminal-first, AI-assisted PowerPoint authoring built on `rho-sdk` and the native `pptx-handler` crate.

## Requirements

- Rust 1.85+
- Kitty or Ghostty for inline slide images
- Chromium, Chrome, or another Chromium-family browser for previews
- Provider credentials configured through `rho login`

## Run

```sh
cargo run -- ~/decks/example.pptx
```

All application state is stored beneath XDG config/data directories. The current repository is never modified without approval. Browser rendering is offline, sandboxed, and isolated in the render cache.

See `delegated-doodling-cocke.md` for architecture and qualification requirements.
