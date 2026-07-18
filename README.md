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

The workspace keeps the active deck, preview state, and contextual controls visible.
Press `Tab` / `Shift+Tab` to move between the conversation, preview, slide list,
and prompt editor. `Ctrl+K` opens the complete action menu, with `F2` as a
fallback for terminal hosts that reserve `Ctrl+K`; `F1` opens keyboard
help. Direct shortcuts include `Ctrl+O` for decks, `Ctrl+P` for designs, `Ctrl+R`
for preview refresh, and `Ctrl+V` to attach the active slide to the next prompt.
Type `/` in the prompt editor to browse slash commands, use Up/Down to choose one,
Tab to complete it, and Enter to run it. Available commands cover the action menu,
decks, designs, preview rendering, settings, attachments, presentation, help, and quit.
On smaller terminals, the workspace shows one focused panel at a time so controls
and content remain usable.

The configuration can also be edited without leaving the TUI: press `Ctrl+,` or
type `/config` in the message input and press Enter. The responsive configuration
popup groups provider, permissions, preview, renderer, and compatibility settings. Use arrow
keys (or `j`/`k`) to navigate, Left/Right to change choices, Enter to edit text,
`Ctrl+S` to save, and Escape to close without saving. Changes are written to the
configuration file immediately and take effect after restarting the application.

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
