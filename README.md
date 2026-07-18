# slide-builder

Linux terminal-first, AI-assisted PowerPoint authoring built on `rho-sdk` and the native `pptx-handler` crate.

## Requirements

- Rust 1.85+
- Kitty or Ghostty for inline slide images
- Chromium, Chrome, or another Chromium-family browser for previews
- Provider credentials entered through slide-builder's in-TUI login (stored separately from rho)

## Providers

Set `provider` and, optionally, `model` in
`$XDG_CONFIG_HOME/slide-builder/config.toml`. Every provider exposed by the
pinned `rho-providers` registry is supported:

- `openai`
- `openai-codex`
- `anthropic`
- `github-copilot`
- `moonshot`
- `openrouter`
- `kimi-code`
- `xai`
- `xai-oauth`

API-key providers show a masked key prompt. OAuth and device-login providers
show the authorization URL and code, then store the resulting tokens in
slide-builder's isolated OS-keyring service. When `model` is empty,
slide-builder opens model setup on launch and preselects Rho's default or cached
model when available. The provider-specific environment variables exposed by
Rho can also be used for automation.

## Run

```sh
cargo run -- ~/decks/example.pptx
```

All application state is stored beneath XDG config/data directories. The current repository is never modified without approval. Browser rendering is offline, sandboxed, and isolated in the render cache.

See `delegated-doodling-cocke.md` for architecture and qualification requirements.
