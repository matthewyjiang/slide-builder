# Linux installation and qualification

## Build

Use Rust 1.92 or newer, as required by the pinned dependencies.

```sh
cargo build --release --locked -j 8
install -Dm755 target/release/slide-builder ~/.local/bin/slide-builder
```

The lockfile pins `rho-sdk`, `rho-providers`, `rho-agent-tools`, `pptx-handler`, and `handler-common` to audited Git revisions. The rho revision is PR #387 because the extracted crates were not yet present on rho `main` when this lockfile was generated.

Obscura and its patched layout/font dependencies are pinned to the tested v0.2.2
revision `a1e09de68c7617b8079fbb1661b0548c501971c1`. Rendering is compiled in.
The V8 dependency downloads a prebuilt static library during a normal build;
building V8 from source is not required. Build-time downloads are separate from
runtime: the installed application does not download an Obscura executable.

## Runtime dependencies

Run inside Kitty or Ghostty for inline previews. The default renderer requires:

- Linux with user namespaces permitted by the host security policy.
- Bubblewrap with `--disable-userns` support, available as `bwrap` on PATH or
  configured through `render.sandbox_path`. Qualification used bubblewrap 0.12.0.

Install bubblewrap through your trusted package or build process. Obscura is
compiled into slide-builder with rendering support; no separate Obscura binary
is needed. Captures launch a private worker mode of the slide-builder executable
inside bubblewrap. Missing bubblewrap or denied namespace creation
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
section to retain that backend, or install bubblewrap for the new default.
Legacy `render.obscura_path` entries are ignored; captures always use the embedded
worker when `engine = "obscura"`.

The default `preview.scale` is 1. Existing explicit values remain unchanged,
including `scale = 2`, which both renderers support. Scale controls output pixels
per CSS pixel without enlarging the layout viewport. For example, a 1600×900
viewport at scale 2 produces a 3200×1800 PNG. Restart after changing settings.

Obscura directly rasterizes supported content at the requested resolution. Some
effects still use its native-resolution raster followed by resizing, so increasing
scale does not improve every element's sharpness equally. Its pinned capture API
limits each native or scaled surface to 16,777,216 pixels and each dimension to
32,768 pixels. Oversized requests fail with an error instead of silently reducing
the scale.

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
given access to host directories. Restart after upgrading slide-builder or Chromium so its
installation fingerprint changes the render cache identity.

Provider credentials are isolated under the `slide-builder` OS-keyring service and are never read from rho's credential entries. If a configured API-key provider has no credential, slide-builder opens a masked login form in the TUI and saves the key securely. Environment-variable overrides remain supported for automation.

## Qualification record

Run the embedded capture tests and the filesystem/network namespace checks on
the target host:

```sh
cargo test --locked -j 8 --test embedded_render -- --include-ignored
cargo test --locked -j 8 --test styled_text_render -- --ignored
cargo test --locked -j 8 --lib sandbox_blocks_host_files_and_network -- --ignored
```

These checks require bubblewrap and permitted user namespaces. The namespace
test also requires `/usr/bin/python3`. No external Obscura executable is used.

- Host: Linux x86_64
- Native handler revision: `acabe4959a37235dd587bbcc788565f19a824bb7`
- Embedded fixture: opens, validates, generates HTML, and round-trips through transactional mutation tests.
- Historical Chromium qualification: found at `/usr/bin/chromium`; browser arguments, offline rejection, CSP injection, cache publication, stale-generation suppression, and timeout behavior are covered by unit tests. This does not qualify the new Obscura backend on your host.
- Embedded Obscura qualification: the pinned v0.2.2 library passed real handler-HTML capture, authored-color rendering, host-stylesheet denial, and direct-worker invocation checks. The sandbox also passed host-file/network denial checks on the implementation host. See `docs/experiments/embedded-obscura.md` for fixture comparisons and the build-profile caveat. Historical external-CLI timings are in `docs/experiments/obscura-integration.md`.
- Native semantic surface: slide create/copy/delete/reorder, text/image/shape add, element update, inspect/validate, and advanced mutation escape hatch. Raw XML is not required for the starter fixture.
- Rendering expectations: static HTML capture only. Animations, transitions, video, and interactive content are unsupported and replaced or omitted.
- Fonts: Chromium uses host-installed fonts. The pinned Obscura renderer uses bundled and page-provided fonts, not automatic system-font discovery. Verify typography with the selected renderer; installing a font alone does not make it available to Obscura.

Manual PowerPoint, LibreOffice, Kitty, Ghostty, GT-template, and visual-golden checks should be repeated when the handler or selected renderer changes.
