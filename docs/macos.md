# Experimental macOS support

macOS uses Chromium for previews, design-import screenshots, and PDF export.
Linux still defaults to embedded Obscura. macOS support remains experimental:
native CI checks capture and export, but does not qualify interactive terminal
rendering, provider login, or PowerPoint interoperability on a user's Mac.

## Setup

Build with Rust 1.92 or newer using the commands in `INSTALL.md`. The macOS build
does not compile Obscura or download its V8 library. Install Google Chrome,
Chromium, Brave Browser, or Microsoft Edge through a trusted source. Safari is
not a supported capture backend. Use Kitty or Ghostty for inline previews.

No renderer configuration is needed for a new installation. With
`browser_path = "auto"`, discovery searches executable names on PATH first,
then app bundles in `/Applications`, then `~/Applications`. Within each app
folder the order is Google Chrome, Chromium, Brave Browser, Microsoft Edge.
The browser executable must exist and have executable permissions.

To choose an installation explicitly:

```toml
[render]
engine = "chromium"
browser_path = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
```

Supply an absolute path to the executable inside the bundle, not the `.app`
directory. Explicit paths take priority and an invalid path produces an error,
not a switch to another installation. Restart after changing renderer settings
or upgrading the browser so the render cache uses the new installation identity.

An omitted `render.engine` uses the platform default. An explicit
`engine = "obscura"` copied from Linux is preserved, but preview discovery fails
with instructions to select Chromium. Obscura needs Linux namespaces and
bubblewrap; macOS does not run it without isolation. `sandbox_path` is ignored
by Chromium. Existing preview scale and explicit browser paths are preserved.

Chromium keeps its own sandbox enabled and uses a separate capture profile.
The HTML pipeline rejects remote resources and applies offline CSP. This is
not the Linux Obscura filesystem/network namespace policy; do not treat the two
backends as equivalent isolation boundaries.

## Application files

macOS follows the native paths supplied by the `directories` crate:

- Configuration: `~/Library/Application Support/slide-builder/config.toml`
- Data, sessions, and design packages: `~/Library/Application Support/slide-builder/`
- Render cache: `~/Library/Caches/slide-builder/`

XDG directory overrides apply on Linux, not macOS. CLI integration tests use a
private child-process HOME as well as XDG roots so they do not modify the user's
native application files. Design package publication uses macOS atomic
no-replace rename, so concurrent imports cannot overwrite an existing package.

## Qualification

The `macOS Chromium lint and tests` CI job runs on `macos-15` and records the OS,
architecture, Rust, and Chrome versions. It runs Clippy and the ordinary test
suite, then real-browser checks for scale-one/scale-two capture and a two-page
portrait PDF exported from a native PowerPoint snapshot. PDF checks use Poppler
to inspect page count, dimensions, authored colors, and shape proportions.

To repeat the browser checks on a Mac with Chromium and Poppler installed:

```sh
cargo test --locked -j 8 --test chromium_render -- --ignored
cargo test --locked -j 8 --test export_pdf chromium_exports -- --ignored
```

The implementation host was Linux, not macOS. Passing CI is not a substitute
for these remaining manual checks:

- First-run provider login and credential persistence in the OS keyring.
- Kitty and Ghostty previews, resize behavior, keyboard flows, and cancellation.
- Design import with real templates and local fonts.
- Opening and editing exported decks in PowerPoint and LibreOffice.
- Intel Macs and other macOS releases outside the native CI runner.

Use `SLIDE_BUILDER_FORCE_FIRST_RUN=1` to repeat onboarding with an existing config.
