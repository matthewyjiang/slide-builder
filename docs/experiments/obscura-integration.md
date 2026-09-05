# Isolated Obscura default

This record describes the external-CLI integration and its measurements. The
current implementation embeds Obscura in a sandboxed worker of the slide-builder
executable. See `INSTALL.md` for current requirements and `embedded-obscura.md`
for its qualification. The timings below describe the external CLI, and the
reproduction commands refer to the earlier implementation.

Implemented September 5, 2026, after the initial renderer investigation. The selected rollout is Obscura by default with mandatory OS filesystem and network isolation, not a trusted-input-only mode.

## What changed

- `render.engine` defaults to `obscura`; `chromium` remains explicitly selectable.
- Obscura runs through bubblewrap. The renderer sees one read-only input HTML file, one writable screenshot file, its executable, and read-only Linux runtime libraries/font directories. Its temporary storage is private. Neither the host render directory nor arbitrary home/project files are mounted.
- User/PID/mount/network namespaces, capability dropping, disabled nested user namespaces, clean environments, and parent-tied process lifetime are mandatory. Missing tools, unsupported flags or denied namespace creation fail closed. There is no unisolated fallback.
- Preview and design-import contact sheets use the same backend configuration. Contact-sheet failures still allow an extraction-only design import, without switching renderers.
- Cache identity includes the capture policy, engine, renderer installation fingerprint, and bubblewrap installation fingerprint. It is applied before cache lookup, at the shared pipeline boundary.
- New preview configs use native scale 1. Obscura rejects other device scales with an actionable error. Existing explicit scale 2 is preserved, not silently downscaled. Increase `preview.width` for a larger native capture or select Chromium for DPR scaling.
- The configuration form exposes engine and executable paths. Unsupported Obscura scale is reported before saving enabled previews; startup dependency errors appear as a preview-unavailable notice rather than terminating the TUI.

Only embedded resources in the generated HTML are intended to load. Relative local assets are not granted access to host directories. System font configuration and system/user font directories are mounted read-only; font substitution remains a rendering-fidelity concern.

## Integrated performance

Measured the actual `BrowserPipeline` at the new default **1600×900, scale 1**, with four concurrent captures. Both engines received the same handler output. These timings include per-slide HTML creation, sandbox/browser startup, PNG decoding, existing blank-capture retries, hashing and cache publication.

| Workload, median of five fresh-cache runs | Chromium Headless Shell | Isolated Obscura |
| --- | ---: | ---: |
| Generated 12-slide deck | 1.339 s | 0.777 s |
| Existing one-slide `testdeck.pptx` | 0.797 s | 0.301 s |

The 12-slide render is **1.72 times faster with isolation enabled**. This is a different measurement from the original native CLI-only result: it includes Rust-side work and uses a larger viewport. It was run in the existing unoptimized development profile, not a release build. The Chromium baseline is still Headless Shell 149.0.7827.55 because the installed full Chrome capture path hung during the initial investigation. Obscura is 0.2.2; bubblewrap is 0.12.0.

Each engine had one warmup followed by five measured runs, with alternating engine order and a new cache directory for every measured render. The helper also requested the next generation against each populated cache and verified reuse of identical images with the updated generation. Timings exclude handler export, one-time renderer discovery, terminal display and the existing configured debounce interval. No new isolated-process memory measurement was made.

The first implementation hashed complete executable files during discovery. That cost 2.15 seconds for Obscura and 3.92 seconds for Chromium in the development build, dwarfing a render. Installation fingerprints now hash canonical path plus device, inode, size, and nanosecond mtime/ctime. Discovery measured approximately 0.08 ms for Obscura and 0.07 ms for Chromium after this change. This is cache invalidation, not an authenticity check. Restart after upgrading renderer binaries.

Raw measured durations are in `obscura-integration-results.json`. Full per-slide manifests and outputs remain in `/tmp/slide-obscura-spike/isolated-pipeline-final/` and `/tmp/slide-obscura-spike/local-deck-pipeline/`.

Reproduce using installed, trusted renderer binaries:

```sh
cargo run -j 8 --example benchmark_pipeline -- \
  /path/to/deck.pptx /tmp/new-pipeline-results \
  /path/to/obscura /path/to/chrome-headless-shell 1600
```

The output directory must not exist. The helper does not modify the supplied deck.

## Verification

The real isolation tests ran bubblewrap on this host rather than merely inspecting command arguments:

- The sandbox reads its allowed input and writes its allowed output.
- A neighboring host sentinel file and `/etc/passwd` are inaccessible.
- A host TCP listener has a successful unsandboxed positive control but cannot be reached inside the renderer namespace.
- The sandbox has a different network namespace and does not inherit an injected test environment variable.
- Additional output files stay in namespace-private storage rather than appearing in the host output directory.
- A handler-generated capture containing a prohibited relative stylesheet does not load that host stylesheet; it produces a correctly sized 640×360 PNG.

Normal behavior tests cover cache separation and executable replacement, missing sandbox failure, unsupported scale, symlink output rejection, bounded failure/diagnostic-pipe lifetime, and killing parent/helper processes when capture is cancelled. Configuration tests cover legacy/default deserialization, explicit Chromium retention, unknown engine rejection and keyboard save behavior.

Run the dependency-dependent tests explicitly:

```sh
SLIDE_BUILDER_TEST_OBSCURA=/path/to/obscura \
  cargo test -j 8 --lib render::browser::tests -- --ignored --test-threads=1
```

These tests additionally require bubblewrap, permitted user namespaces and `/usr/bin/python3`. Normal tests do not require downloaded browser binaries.

This qualifies the implementation's isolation and rendering path on the test host. It does not establish pixel equivalence with Chromium or PowerPoint across arbitrary decks, impose memory/disk quotas, or qualify every Linux distribution's namespace policy. The original fidelity findings still apply.
