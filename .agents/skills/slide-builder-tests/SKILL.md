---
name: slide-builder-tests
description: Write, run, and debug tests in slide-builder, including Rust behavior tests and real-terminal scenarios using tools/pty. Use for regression coverage, TUI interaction testing, and PTY failure diagnosis in this repository.
---

# Slide-builder tests

Work from the repository root. Read [AGENTS.md](../../../AGENTS.md) for test placement
and Rust conventions. For TUI behavior, read [PRODUCT.md](../../../PRODUCT.md) and
[DESIGN.md](../../../DESIGN.md). Preserve keyboard navigation and narrow-terminal
support; Escape returns from nested views and cancels only at the root.

## Choose the test boundary

Start with the observable behavior or invariant that could regress. Inspect the
nearest existing test before adding coverage.

- Pure logic or application state transitions belong beside the owning module in
  a sibling `*_tests.rs`, declared with `#[path = "..."] mod tests;`.
- Public API, persistence, CLI, rendering, and export behavior belong in `tests/`
  when they need integration coverage. Reuse the relevant fixtures and prerequisite
  checks; consult existing renderer tests before choosing an ignored browser test.
- Actual terminal input decoding, redraw, cursor placement, resize, and navigation
  across screens belong in the PTY suite. State-only tests do not prove that the
  executable handles terminal bytes correctly.
- Harness mechanics belong in `tools/pty/tests/harness.rs`; app scenarios currently
  live in `tools/pty/src/main.rs`. Keep app-specific choices out of the generic
  controller, screen parser, and wait helpers. Extract cohesive scenario modules
  as the runner grows.

Use focused regression coverage that would fail for the defect. Avoid tests that
copy implementation decisions, assert constants, or merely confirm removed code
is absent. Prefer complete expected values and `pretty_assertions::assert_eq` when
already available. Pass dependencies and environment-derived values explicitly;
do not mutate the test process environment. A documentation-only change normally
needs reference and command checks rather than new Rust tests.

## Write and run PTY scenarios

Read [docs/pty-testing.md](../../../docs/pty-testing.md) for commands, supported
scenarios, artifacts, and limitations. Inspect the current
[runner](../../../tools/pty/src/main.rs) and
[harness API](../../../tools/pty/src/harness.rs) before extending them.

Build the current application before drawing conclusions from a PTY run. The
standalone harness build does not rebuild slide-builder:

```sh
cargo build --locked --bin slide-builder
cargo run --locked --manifest-path tools/pty/Cargo.toml -- all target/debug/slide-builder
```

Replace `all` with the relevant scenario when iterating. The harness has its own
manifest and lockfile; root `cargo test`, `cargo fmt --all`, and Clippy do not cover
it automatically.

For a new scenario:

1. Create an `IsolatedWorkspace`, fixture files, and a `PtyHarness`. Keep the
   workspace alive until after the child exits or the harness drops. Use its
   complete child environment, temporary working directory, and an artifact
   directory under `target/pty-artifacts`.
2. Wait for the starting screen, send the intended key or paste event, then wait
   for a state that proves the transition occurred. Use `key`, `paste`, `send`,
   and `resize` according to the behavior being tested.
3. Assert visible output with bounded `wait_for_text`, `wait_for_text_gone`, or
   `wait_until`. A predicate must distinguish the new frame from the old one.
   For example, filter text can also occur in a filename; match the input row or
   changed results. Establish that text was present before waiting for it to go.
   Terminal size changing in the parser alone does not prove the app redrew.
4. Cover a wide and narrow terminal for layout-sensitive changes. Check nested
   Escape navigation and root cancellation where the scenario crosses views.
5. Capture the relevant screen before exiting, then use `expect_exit` with the
   application's actual expected status. The current onboarding root cancel is
   status 1; the deck picker cancel is status 0.

Use screen predicates instead of arbitrary sleeps. `screen()` exposes parsed
cells, styles, cursor position, and visible text. Historical raw bytes cannot
prove text is still visible. Wait and exit assertion failures save diagnostics;
if adding a direct assertion, capture first so a failure leaves evidence.

The launch environment sets `SLIDE_BUILDER_FORCE_FIRST_RUN=1`. Current scenarios
stop before provider authentication. Temporary HOME does not isolate an OS
keyring; authentication and live-agent tests need explicit fixture dependencies.
Use synthetic inputs because captures contain typed bytes and terminal output.
The parser does not render Kitty/Sixel images or answer palette/capability queries.
Use renderer tests for image correctness rather than claiming PTY coverage of it.

## Validate and diagnose

Run the smallest relevant tests first. Examples, with the test target and filter
replaced by those you changed:

```sh
cargo test --locked --lib test_name
cargo test --locked --test sessions_cli test_name
cargo test --locked --manifest-path tools/pty/Cargo.toml
cargo fmt --manifest-path tools/pty/Cargo.toml -- --check
cargo clippy --locked --manifest-path tools/pty/Cargo.toml --all-targets -- -D warnings
```

For application changes, run the applicable root formatting, lint, and integration
checks in [.github/workflows/ci.yml](../../../.github/workflows/ci.yml). After the
required checks pass, broaden testing only for an unresolved concern or new change.

On PTY failure, inspect `report.txt`, `screen.txt`, and `actions.log` at the printed
capture path. Use `raw.pty` and `screen.ansi` for escape-sequence or styling issues.
Distinguish a stale binary, missing fixture, child exit, and wrong wait predicate
from an application regression before changing timeouts or expected output. Rerun
the affected scenario after a fix. Report the commands run, results, and platform
or renderer limits; do not imply unrun macOS or image checks passed.

Update the relevant guide in `docs/` when behavior or test usage changes. Keep
feature-specific testing documentation out of `README.md`.
