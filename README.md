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
Keyboard input stays in the prompt editor. Click a slide in the slide list to make it
active. Drag across visible conversation text and release to copy it; a brief popup
confirms how many characters were copied. Use the tmux-style `Ctrl+B` slide prefix,
followed by
`h`/`k` to move to the previous slide or `j`/`l` to move to the next, `g`/`G`
to jump to the first/last slide, `r` to refresh the preview, or Enter/`f` to
present. `Ctrl+K` opens the complete action menu, with `F2` as a fallback for
terminal hosts that reserve `Ctrl+K`; `F1` opens keyboard help. Direct shortcuts
include `Ctrl+O` for decks, `Ctrl+P` for designs, `Ctrl+R` for preview refresh,
and `Ctrl+V` to attach the active slide to the next prompt.
`Ctrl+C` clears a non-empty prompt; pressing it again with an empty composer exits.
Type `/` in the prompt editor to browse slash commands, use Up/Down to choose one,
Tab to complete it, and Enter to run it. Available commands cover the action menu,
decks, designs, preview rendering, settings, attachments, presentation, help, and quit.
On smaller terminals, all three status surfaces stack vertically above the prompt.

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

At startup, slide-builder discovers project skills from `.agents/skills`, user
skills from `~/.agents/skills`, and its embedded deck-authoring skills. Matching
skills are advertised to the agent and loaded on demand through the built-in
`load_skill` tool. Project skills take precedence over user and embedded skills
with the same name.

## Design packages

Slide-builder stores imported packages in its managed package directory. On Linux,
managed packages live at:

```text
$XDG_DATA_HOME/slide-builder/design-packages/
```

This normally resolves to `~/.local/share/slide-builder/design-packages/`.
Each package contains a required `DESIGN.md` and one or more PowerPoint templates.

Run `/import-design` to create a managed package from an existing `.pptx` file.
The file picker accepts keyboard navigation or a typed or pasted path.
Slide-builder copies the source into a private staging directory, extracts its
presentation structure, and renders a contact sheet when Chromium is available.
The configured model uses that evidence and the built-in import skill in a fresh,
tool-free importer session to write `DESIGN.md`. Import stages appear beside the
prompt; generated model output is not added to the chat transcript. Slide-builder
validates the result and publishes it atomically with the original presentation
saved as `template.pptx`. Existing packages are never overwritten; repeated names
receive a numeric suffix. Run `/design` to select an imported package before the
next deck prompt.

## Run

```sh
cargo run -- ~/decks/example.pptx
```

All application state is stored beneath XDG config/data directories. The current repository is never modified without approval. Browser rendering is offline, sandboxed, and isolated in the render cache.

See `delegated-doodling-cocke.md` for architecture and qualification requirements.
