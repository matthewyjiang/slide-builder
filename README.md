# slide-builder

Linux terminal-first, AI-assisted PowerPoint authoring built on `rho-sdk` and the native `pptx-handler` crate.

## Requirements

- Rust 1.92+ to build from source
- Kitty or Ghostty for inline slide images
- Bubblewrap (`bwrap`) for the embedded Obscura renderer on Linux, with user namespaces enabled
- Alternatively, explicitly select Chromium, Chrome, or another Chromium-family browser
- Provider credentials entered through slide-builder's in-TUI login (stored separately from rho)

## Providers

On first launch, slide-builder lists every provider exposed by the pinned
`rho-providers` registry. Choose a provider, authenticate in the terminal, and
then select from the models Rho detects for that account. Providers with more
than one authentication mode open a nested connection picker. The resulting
`provider`, `auth`, and `model` are saved to
`$XDG_CONFIG_HOME/slide-builder/config.toml`. Set `SLIDE_BUILDER_FORCE_FIRST_RUN=1`
to open this flow even when that config already exists.

The workspace keeps the active deck, preview state, and contextual controls visible.
Keyboard input stays in the prompt editor. Click a slide in the slide list to make it
active. Use the mouse wheel over the conversation to scroll through its history. Drag
across visible conversation text and release to copy it; a brief popup
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
slide-builder's isolated OS-keyring service. After authentication, slide-builder
refreshes the provider's live model list through Rho, with Rho's cached or static
catalog as the fallback. The provider-specific environment variables exposed by
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
presentation structure, and renders a contact sheet using the configured renderer.
If rendering is unavailable, import continues with extracted presentation evidence only.
The configured model uses that evidence and the built-in import skill in a fresh,
tool-free importer session to write `DESIGN.md`. Import stages appear beside the
prompt; generated model output is not added to the chat transcript. Slide-builder
validates the result and publishes it atomically with the original presentation
saved as `template.pptx`. Existing packages are never overwritten; repeated names
receive a numeric suffix. Run `/design` to select an imported package before the
next deck prompt.

## Preview renderer

Obscura is the default renderer and is embedded in the slide-builder executable.
Captures run in a sandboxed worker process of that same executable; no separate
Obscura installation is needed. Bubblewrap is still required on Linux with user
namespaces enabled. Missing or unusable isolation disables previews
with an error; it never falls back to unisolated Obscura or another engine.

Renderer settings in `$XDG_CONFIG_HOME/slide-builder/config.toml` default to:

```toml
[preview]
scale = 1

[render]
engine = "obscura"
sandbox_path = "auto"
browser_path = "auto"
```

`auto` discovers executables on PATH. `sandbox_path` selects `bwrap`;
`browser_path` is used only by Chromium. Set explicit executable paths when needed,
or use the Renderer section in `/config`, then restart.

### Migrating an existing configuration

Configs without `render.engine` now select Obscura, even if `browser_path` is set.
The embedded worker replaces the external Obscura executable. Legacy
`render.obscura_path` entries are ignored and can be removed.
To keep Chromium, add `engine = "chromium"` to the existing `[render]` section:

```toml
[render]
engine = "chromium"
browser_path = "auto" # Or retain your existing Chromium executable path.
```

The default `preview.scale` is 1. Both renderers support `scale = 2` without
changing the slide's layout size: a 1600×900 viewport produces a 3200×1800 PNG.
Existing explicit scales are preserved. Obscura renders supported content at the
higher resolution; some effects use an upscaled native-resolution surface instead.
Scaled captures must fit Obscura's capture limits. See `INSTALL.md` for details.

## Run

```sh
cargo run --release -- ~/decks/example.pptx
```

Use the release profile for normal use. Debug builds also leave the embedded
native renderer unoptimized and produce substantially slower previews.

All application state is stored beneath XDG config/data directories. The current repository is never modified without approval. Rendering is offline and sandboxed, with artifacts isolated in the render cache.

See `delegated-doodling-cocke.md` for architecture and qualification requirements.
