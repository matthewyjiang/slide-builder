# Linux installation and qualification

## Build

```sh
cargo build --release --locked -j 8
install -Dm755 target/release/slide-builder ~/.local/bin/slide-builder
```

The lockfile pins `rho-sdk`, `rho-providers`, `rho-agent-tools`, `pptx-handler`, and `handler-common` to audited Git revisions. The rho revision is PR #387 because the extracted crates were not yet present on rho `main` when this lockfile was generated.

## Runtime dependencies

Run inside Kitty or Ghostty for inline previews. The default renderer requires:

- Linux with user namespaces permitted by the host security policy.
- A render-enabled Obscura binary, version 0.2.2 or newer, available as `obscura`
  on PATH or configured through `render.obscura_path`.
- Bubblewrap with `--disable-userns` support, available as `bwrap` on PATH or
  configured through `render.sandbox_path`. Qualification used bubblewrap 0.12.0.

Install these dependencies yourself through your trusted package or build process.
Slide-builder does not install or download renderers. A binary built without render
support cannot capture slides. Missing bubblewrap or denied namespace creation
disables previews with an actionable error. The renderer fails closed rather than
running Obscura without isolation or switching to Chromium.

To use Chromium instead, install a Chromium-family browser (`chromium`,
`chromium-browser`, `google-chrome`, `google-chrome-stable`, `brave-browser`, or
`microsoft-edge`) and explicitly configure:

```toml
[render]
engine = "chromium"
browser_path = "auto"
```

Retain an existing explicit `browser_path` if needed. Chromium discovery never
adds `--no-sandbox`; captures use isolated profiles and offline CSP.

### Existing configurations

An omitted `render.engine` now means `obscura`. Setting `browser_path` alone no
longer selects Chromium. Add `engine = "chromium"` to the existing `[render]`
section to retain that backend, or install Obscura and bubblewrap for the new default.

The default `preview.scale` changes from 2 to 1. Existing explicit values remain
unchanged. Obscura requires scale 1. Set `scale = 1` in the existing
`[preview]` section only if native-resolution output is acceptable, or select
Chromium to preserve higher device-scale rendering. Restart after changing settings.

### Isolation policy

Each Obscura capture gets a separate user, PID, mount and network namespace.
The renderer can read its executable, Linux runtime libraries, system font
configuration, system/user font directories, and the one generated HTML file.
It can write only its bound screenshot file on the host; temporary files stay
inside the namespace. The host project, home directory apart from the selected
font directories, credential stores, sockets and render-cache directory are not
mounted. Inherited environment variables are cleared before launching bubblewrap
and again before the renderer. Timeouts and cancellation kill the renderer tree.

Resources must be embedded in the handler HTML. Relative local assets are not
given access to host directories. Restart after upgrading a renderer so its
installation fingerprint changes the render cache identity.

Provider credentials are isolated under the `slide-builder` OS-keyring service and are never read from rho's credential entries. If a configured API-key provider has no credential, slide-builder opens a masked login form in the TUI and saves the key securely. Environment-variable overrides remain supported for automation.

## Qualification record

- Host: Linux x86_64
- Native handler revision: `acabe4959a37235dd587bbcc788565f19a824bb7`
- Embedded fixture: opens, validates, generates HTML, and round-trips through transactional mutation tests.
- Historical Chromium qualification: found at `/usr/bin/chromium`; browser arguments, offline rejection, CSP injection, cache publication, stale-generation suppression, and timeout behavior are covered by unit tests. This does not qualify the new Obscura backend on your host.
- Isolated Obscura qualification: Obscura 0.2.2 with bubblewrap 0.12.0 passed real host-file/network denial and handler-capture tests on the implementation host. Integrated timings and reproduction commands are in `docs/experiments/obscura-integration.md`.
- Native semantic surface: slide create/copy/delete/reorder, text/image/shape add, element update, inspect/validate, and advanced mutation escape hatch. Raw XML is not required for the starter fixture.
- Rendering expectations: static HTML capture only. Animations, transitions, video, and interactive content are unsupported and replaced or omitted.
- Fonts: output depends on host-installed fonts and the selected renderer's fallback behavior. Verify typography with the intended fonts installed.

Manual PowerPoint, LibreOffice, Kitty, Ghostty, GT-template, and visual-golden checks should be repeated when the handler or selected renderer changes.
