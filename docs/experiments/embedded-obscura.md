# Embedded Obscura qualification

The embedded renderer replaces the external Obscura CLI with a private worker
mode of the slide-builder executable. The worker links `obscura-browser` with
native rendering enabled at v0.2.2 revision
`a1e09de68c7617b8079fbb1661b0548c501971c1`. The root Cargo manifest also pins
Obscura's patched `taffy` and `cosmic-text` dependencies to that revision.

## Process boundary

The application starts its own executable inside bubblewrap. Worker dispatch
happens before application configuration, credentials, the TUI, or its Tokio
runtime are initialized. The worker creates a current-thread runtime and calls
Obscura's page navigation, resource preparation, and screenshot APIs directly.
It accepts viewport dimensions, output scale, and a navigation deadline, not
arbitrary URLs or file paths. Its input and output are the fixed sandbox mounts.

The parent still enforces the whole-capture deadline and process-group cleanup.
Filesystem/network namespaces, environment clearing, and fail-closed behavior
are unchanged. Bubblewrap remains a runtime dependency. Chromium remains an
explicit alternative, with no automatic fallback.

### Implementation ownership

`src/render/sandbox.rs` selects the platform launcher behind a concrete `Sandbox`.
Its `Request` contains only the host worker executable, input file, and output
file. The returned `Launch` contains a command and the paths visible to the
worker. Linux setup lives in `sandbox/linux.rs` and uses bubblewrap; macOS setup
lives in `sandbox/macos.rs` with its unchanged `macos.sb` sandbox-exec profile.
Unsupported platforms still fail closed. Platform policy tests live beside
these implementations, including the native macOS child-process probes.

`src/render/executable.rs` shares executable discovery and validation between
the browser and sandbox without making sandbox setup depend on the browser.
`browser.rs` still owns engine selection, capture validation, worker arguments,
and renderer cache identity. Its shared process runner owns capture deadlines,
diagnostics, and process-group cleanup for both engines. Extracting sandbox
setup does not change launcher discovery, permissions, environment handling,
process behavior, or the cache identity format.

## Scaled captures

The worker preserves the CSS viewport and passes output scale to Obscura's
region-capture API. Scale 2 doubles PNG width and height without doubling the
layout viewport. Scale 1 keeps the original screenshot path. Obscura's capture
allocation checks apply before launching the worker as well as before painting.

The native high-resolution raster path supports a subset of content. Other
effects can fall back to a native-resolution surface followed by resizing;
scaled output dimensions do not guarantee sharper rendering of every effect.
The performance and pixel comparisons below were measured at scale 1.

The scaled-capture E2E test runs the actual sandboxed executable at scales 0.25,
1, 2, and 4. It checks PNG dimensions and every pixel of a fixed CSS rectangle,
including its sharp boundaries and percentage-based position. This checks layout
preservation and crisp edges at the output scale. Oversized legacy configurations
can still open the app; enabled-preview configuration saves and capture requests
validate the renderer's allocation limits.

## Verification on September 5, 2026

- Rust 1.92.0 built and tested the integration.
- All-target tests passed: 172 library tests, 11 binary tests, and one ordinary
  integration test. The sandbox-dependent tests were then run explicitly.
- Embedded E2E checks passed for handler-generated HTML, PNG dimensions, authored
  CSS colors, prohibited host stylesheets, and refusal of direct worker invocation.
- The namespace qualification passed real host-file and network-denial checks.
- Existing process timeout/cancellation tests passed. These test the shared
  process runner, not a newly instrumented native-engine hang.
- Clippy with warnings denied, rustfmt, and `git diff --check` passed.
- The benchmark pipeline rendered all 12 generated slides at 1600x900, scale 1,
  with four concurrent captures and verified subsequent-generation cache hits.
- All 12 embedded-renderer PNGs were pixel-identical to the previous external-CLI
  pipeline outputs for that fixture. This does not establish fidelity across
  arbitrary decks, Chromium, or PowerPoint.

Tests and logs are described in `INSTALL.md`. Local verification artifacts are
in `/tmp/slide-embedded-*.log` and `/tmp/slide-embedded-pipeline-qualification/`.
The comparison artifacts are in
`/tmp/slide-obscura-spike/isolated-pipeline-final/`.

## Build-profile caveat

The old integration spawned a release-built Obscura CLI even when slide-builder
itself used the development profile. Embedding puts native rendering code under
the application's Cargo build profile too. The unoptimized development build
measured a median 4.497 seconds for the 12-slide fixture, compared with 0.777
seconds in the historical external-CLI measurement. These are five fresh-cache
samples after a warmup; cache-hit checks still passed.

Do not treat an unoptimized embedded build as comparable to a release-built
external renderer. The installation instructions use `--release`.

The release build measured **0.516 seconds** for embedded Obscura versus
**1.338 seconds** for Chromium in the same run. Both used the same 12-slide
fixture, 1600x900 output, scale 1, four concurrent captures, and five fresh-cache
samples after a warmup. All 12 release PNGs were also pixel-identical to the
previous CLI outputs. Median cache-hit time was 0.070 ms for Obscura.

Release artifacts and raw per-sample timings are in
`/tmp/slide-embedded-release-pipeline-qualification/`; the build log is
`/tmp/slide-embedded-release-build.log`. The historical 0.777-second result also
includes unoptimized application-side work, so it is not a controlled measure
of the library-versus-CLI overhead alone.

To reproduce with a fresh output directory:

```sh
cargo run --release --locked -j 8 --example benchmark_pipeline -- \
  /path/to/deck.pptx /tmp/new-embedded-results \
  /path/to/chrome-headless-shell 1600
```
