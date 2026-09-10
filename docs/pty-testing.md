# PTY testing

`tools/pty` drives the real slide-builder executable through a Unix pseudo-terminal.
It follows Rho's harness design: a PTY controller, a VT100 screen parser, scripted
input, bounded waits, and failure captures. The controller and key encodings are
adapted from Rho under the MIT notice in `tools/pty/NOTICE`.

The harness is a separate Cargo package with its own lockfile. Its tests do not
build slide-builder's rendering dependencies, and it is not linked into the app.
Linux and macOS CI run the harness checks and application scenarios.

For agent-assisted test work, use `$slide-builder-tests`. The project skill at
[.agents/skills/slide-builder-tests/SKILL.md](../.agents/skills/slide-builder-tests/SKILL.md)
covers choosing a test boundary, adding PTY scenarios, and diagnosing failures.

## Run

From the repository root:

```sh
cargo test --locked --manifest-path tools/pty/Cargo.toml
cargo build --locked --bin slide-builder
cargo run --locked --manifest-path tools/pty/Cargo.toml -- all target/debug/slide-builder
```

Use `picker` or `onboarding` instead of `all` to run one scenario. An optional final
argument selects the capture directory, which defaults to `target/pty-artifacts`.
The binary path is explicit so the runner can also test release builds.

Both scenarios run at 110 columns by 32 rows and 60 columns by 24 rows:

- `picker` checks startup, bracketed paste filtering, backspace, resize to 72 by 28,
  and successful cancellation.
- `onboarding` creates a temporary deck, checks provider selection, opens the
  connection-method picker, returns with Escape, and cancels from the root.
  Root onboarding cancellation currently exits with status 1.

Each launch gets a temporary working directory, HOME, and XDG config, data, and
cache directories. The child environment excludes inherited provider credentials
and terminal integration markers. `SLIDE_BUILDER_FORCE_FIRST_RUN=1` forces setup.
The onboarding scenario stops before authentication; it does not contact providers
or access the OS credential store. Temporary HOME alone does not isolate an OS
keyring, so new authentication scenarios need a separate credential fixture.

## Add a scenario

The library exports `PtyHarness`, `PtySize`, `Key`, and `IsolatedWorkspace`. Keep the
workspace alive until the harness drops. `PtyHarness::spawn` accepts an executable,
arguments, terminal size, complete child environment, working directory, and
artifact directory. The generic harness contains no slide-builder startup policy.

Use `key` for named keys, `send` for raw input, `paste` for bracketed paste, and
`resize` for terminal dimensions. Mouse encodings are available in `keys` and can
be passed to `send`. `screen()` exposes the parsed cells, colors, cursor, and text.

Wait for visible state with `wait_for_text`, `wait_for_text_gone`, or `wait_until`.
Choose a predicate that cannot already match the previous frame. Assertions run
against the current screen rather than historical output, so erased text does not
satisfy later waits. `expect_exit` checks the exit code while draining output.
Dropping the harness kills and reaps its child and signals its process group.

The self-tests cover screen clearing, input, resize propagation, environment
isolation, early exit, timeout diagnostics, and child cleanup. Run formatting and
lint checks separately from the app:

```sh
cargo fmt --manifest-path tools/pty/Cargo.toml -- --check
cargo clippy --locked --manifest-path tools/pty/Cargo.toml --all-targets -- -D warnings
```

## Captures and limits

Successful scenarios print a capture directory. Wait and exit assertion failures
save captures automatically and include the directory in the error. Each capture
contains `screen.txt`, `screen.ansi`, `raw.pty`, `actions.log`, and `report.txt` with
the terminal size and cursor position. Capture directories have unique names for
parallel runs. Input and terminal output are recorded, so use fixture text rather
than real credentials.

The VT100 parser reconstructs terminal cells; it does not render Kitty/Sixel image
protocols or answer terminal capability and palette queries. These scenarios test
the application's fallback terminal behavior. Preview-image correctness and live
agent conversations remain covered by other tests; this suite currently exercises
startup and onboarding only. Windows is not supported.
