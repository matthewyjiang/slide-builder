# Linux installation and qualification

## Build

```sh
cargo build --release --locked
install -Dm755 target/release/slide-builder ~/.local/bin/slide-builder
```

The lockfile pins `rho-sdk`, `rho-providers`, `rho-agent-tools`, `pptx-handler`, and `handler-common` to audited Git revisions. The rho revision is PR #387 because the extracted crates were not yet present on rho `main` when this lockfile was generated.

## Runtime dependencies

Install one Chromium-family browser (`chromium`, `chromium-browser`, `google-chrome`, `google-chrome-stable`, `brave-browser`, or `microsoft-edge`) and run inside Kitty or Ghostty. Browser discovery never adds `--no-sandbox`; captures use isolated profiles and offline CSP.

Provider credentials are shared with rho. If startup reports missing credentials, run `rho login`.

## Qualification record

- Host: Linux x86_64
- Native handler revision: `acabe4959a37235dd587bbcc788565f19a824bb7`
- Embedded fixture: opens, validates, generates HTML, and round-trips through transactional mutation tests.
- Local browser probe: Chromium found at `/usr/bin/chromium`; browser arguments, offline rejection, CSP injection, cache publication, stale-generation suppression, and timeout behavior are covered by unit tests.
- Native semantic surface: slide create/copy/delete/reorder, text/image/shape add, element update, inspect/validate, and advanced mutation escape hatch. Raw XML is not required for the starter fixture.
- Rendering expectations: static HTML capture only. Animations, transitions, video, and interactive content are unsupported and replaced or omitted.
- Fonts: output depends on host-installed fonts; missing fonts fall back according to Chromium/fontconfig and are surfaced as fidelity diagnostics.

Manual PowerPoint, LibreOffice, Kitty, Ghostty, GT-template, and visual-golden checks should be repeated when either pinned handler/browser qualification revision changes.
