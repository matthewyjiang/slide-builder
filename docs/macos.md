# Experimental macOS rendering

Obscura remains the primary renderer and the default on every platform. The
macOS backend launches the embedded worker through `/usr/bin/sandbox-exec`.
Chromium is available only when explicitly selected; failures never change
engines or run Obscura without isolation.

This backend is not ready for use. Native capture works, but host-process
confidentiality is still failing qualification. This branch remains a draft
pending a fix; the current policy is not an approved security boundary.
The implementation host was Linux. macOS support remains experimental even
after automated qualification, pending the manual checks below.

## Setup

Build with Rust 1.92 or newer using the commands in `INSTALL.md`. Native builds
compile embedded Obscura and download its pinned V8 static library. No separate
Obscura or Chromium installation is needed for the default engine. Use Kitty
or Ghostty for inline terminal previews.

```toml
[render]
engine = "obscura"
sandbox_path = "auto"
```

On macOS, `auto` selects `/usr/bin/sandbox-exec`; on Linux it discovers bubblewrap.
An explicit `sandbox_path` must name the matching platform launcher. A missing
launcher or rejected policy is an error. Restart after changing settings or
upgrading slide-builder so render-cache identity reflects the new installation.

## Isolation and maintenance risk

The macOS launcher applies a deny-default SBPL profile before loading the worker,
including its native library initializers. Paths are separate `-D` arguments,
not interpolated profile source. The environment is cleared before sandbox-exec.

The profile grants reads of the executable, one generated HTML file, system
libraries/frameworks, and random devices. The root directory itself is readable
for dyld startup, but that literal grant does not grant access to its children.
It grants writes only to the pre-created
screenshot file. No home/project directory, credential store, network, Mach
service lookup, or subprocess grants are present. Capture deadlines and
cancellation kill the sandbox process group. The worker's private-mode guard
prevents accidental direct invocation; the external sandbox is the security
boundary, not that guard.

This is not the Linux filesystem/network namespace mechanism. macOS retains the
host filesystem layout and enforces access through Seatbelt policy. Both backends
must fail closed, but their guarantees and operating-system dependencies differ.

SBPL has no official OS-provided documentation. Apple's supported application
sandbox model uses entitlements; this experimental command-line integration
instead depends on sandbox-exec and a custom SBPL profile. OS changes can break
capture, and a permissive compatibility workaround can weaken isolation. Do not
add broad Mach service, user-directory, or network permissions to silence errors.
A supported entitlement-based helper would require a separate packaging/signing
design rather than a drop-in replacement for the source-installed CLI.

The implementation follows Chromium's documented pre-initializer sandbox design
and explicit-resource policy. Its design document describes the compatibility
and security tradeoff and the lack of official SBPL documentation:

- `https://raw.githubusercontent.com/chromium/chromium/main/sandbox/mac/seatbelt_sandbox_design.md`

## Existing Chromium option

Explicit Chromium settings remain supported by the existing adapter; Obscura
never switches to Chromium automatically. Chromium runtime behavior is not
qualified on macOS by this change. Chrome 152 produced PNGs but failed to exit on
the headless macOS CI host, including with GPU rendering disabled. Native macOS
support does not depend on installing a Chromium browser.

## Application files

macOS follows the native paths supplied by the `directories` crate:

- Configuration: `~/Library/Application Support/slide-builder/config.toml`
- Data, sessions, and design packages: `~/Library/Application Support/slide-builder/`
- Render cache: `~/Library/Caches/slide-builder/`

XDG overrides apply on Linux, not macOS. CLI tests use a private child-process HOME
as well as XDG roots. Design package publication uses atomic no-replace rename
on macOS so concurrent imports cannot overwrite a package.

## Qualification

CI has separate Linux CI and macOS CI jobs for project-wide compilation, Clippy,
and tests. Rust formatting runs separately. The macOS CI job uses `macos-15` and
records OS, architecture, Rust, and Poppler versions. Its additional integration
steps check host-file/network/Mach-service denial, real Obscura captures, styled
text, and portrait PDF export using Poppler. macOS CI excludes the two existing
optional Chromium runtime tests; those tests remain available for manual
qualification and are unchanged on Linux.

To repeat the Obscura checks on a Mac with Poppler installed:

```sh
cargo test --locked -j 8 --lib macos_sandbox_blocks_host_files_network_and_services -- --ignored
cargo test --locked -j 8 --test embedded_render -- --include-ignored
cargo test --locked -j 8 --test export_pdf obscura_exports -- --ignored
```

Passing CI does not replace these remaining manual checks:

- First-run provider login and OS-keyring credential persistence.
- Kitty and Ghostty preview rendering, resizing, keyboard flows, and cancellation.
- Design import with real templates and typography.
- Opening and editing decks in PowerPoint and LibreOffice.
- Other macOS releases and architectures outside the native CI runner.

Use `SLIDE_BUILDER_FORCE_FIRST_RUN=1` to repeat onboarding with an existing config.
