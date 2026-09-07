# Experimental macOS rendering

Obscura remains the primary renderer and the default on every platform. The
macOS backend launches the embedded worker through `/usr/bin/sandbox-exec`.
Chromium is available only when explicitly selected; failures never change
engines or run Obscura without isolation.

macOS support remains experimental. Native CI passed capture, export, and
isolation checks on macOS 15.7.9 ARM64, but that does not qualify other releases,
architectures, or interactive terminal behavior. The implementation host was
Linux; the macOS checks ran on GitHub's native runner.

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

Keep the explicit process-inspection denials as well as `deny default`. During
qualification, the default-only profile allowed a sandboxed process to read a
separate host fixture's argument and environment sentinels. Explicit denials for
`process-info*` and the `kern.procargs` sysctl-name prefix blocked that data read
with `PermissionDenied`. The prefix covers PID-suffixed names such as
`kern.procargs2.<pid>`. The test fixture has a cleared environment and only
nonsecret sentinels; it never needs real host credentials.

This is not the Linux filesystem/network namespace mechanism. macOS retains the
host filesystem layout and enforces access through Seatbelt policy. Both backends
must fail closed, but their guarantees and operating-system dependencies differ.

SBPL has no official OS-provided documentation. Apple's supported application
sandbox model uses entitlements; this experimental command-line integration
instead depends on sandbox-exec and a custom SBPL profile. OS changes can break
capture, and a permissive compatibility workaround can weaken isolation. Do not
add broad Mach service, user-directory, or network permissions to silence errors.
The native sentinel probes must stay part of qualification; a deny-default
profile alone is not evidence that every host-data interface is restricted.

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

Sandbox backends return a command and the input/output paths visible inside
their isolation boundary. Both platforms use the same private worker argument
protocol; only sandbox construction and policy are platform-specific. CI failure
diagnostics are best-effort: unavailable sandbox logs or malformed crash reports
do not prevent collecting the remaining reports. Their regression tests run in
Linux CI.

The complete native checks passed in GitHub Actions run `34081510067` on
macOS 15.7.9, build `24G830`, ARM64, using Rust 1.92.0. The process-argument
sentinel probe failed on the preceding profile and passed with the explicit
denials described above. No network, Mach-service, home-directory, or project
read permissions were added to make rendering pass.

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
