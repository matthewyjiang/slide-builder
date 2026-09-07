# Native Herdr integration

Run `slide-builder path/to/deck.pptx` in a Herdr pane as usual. Slide Builder detects the host and reports its own state directly over Herdr's socket. No plugin, hooks, or additional installation is required.

The integration activates on Unix when `HERDR_ENV=1`, `HERDR_SOCKET_PATH`, and `HERDR_PANE_ID` are present. Herdr supplies these variables. Outside Herdr, and on unsupported platforms, reporting is disabled and ordinary terminal preview detection is unchanged. Command-line session management and the initial onboarding/deck-selection screens do not register an agent; registration begins when the deck workspace opens.

## Activity and attention

Slide Builder reports as `slide-builder`, with the source `herdr:slide-builder`.

| Application state | Reported state |
| --- | --- |
| Composer ready | `idle` |
| Agent run, design import, or PDF export active | `working` |
| Tool approval or question open | `blocked` |
| Other dialog open or slideshow active | `blocked` |
| Work completes, fails, or is cancelled | `idle`, once other work is finished |
| Workspace closes or switches deck/session | Release the registration after shutdown |

A preview refresh alone does not make the agent busy. Messages describe the operation and its outcome without sending prompt text, tool arguments, provider errors, or deck contents. Herdr decides whether an idle agent represents unseen completion and should appear as `done`.

Reports run sequentially in a background task. Pending state updates coalesce to the latest state when the host is slow, so this is a current-status integration, not a lossless event log. Unchanged reports are suppressed, including after failures: the next state or message change retries reporting rather than every UI tick. Socket failures do not stop deck editing. If reporting failed during the session, the latest error is printed after the terminal is restored on exit, even if a later retry succeeded.

## Slide previews

With the default `preview.protocol = "auto"`, Slide Builder queries `pane.graphics.info` before opening the deck workspace:

- Positive host cell pixel dimensions select Kitty graphics using those dimensions.
- Missing dimensions, disabled host graphics, a disconnected socket, or an invalid reply select half-block previews instead of potentially blank image placements.
- Outside Herdr, Slide Builder uses its usual terminal capability query.

An explicit `preview.protocol` override still wins, including `halfblocks`, `kitty`, `sixel`, and `iterm2`. Older configurations may explicitly select `kitty`; change that value to `auto` to enable host-aware fallback. New configurations default to `auto`. Forcing a protocol does not make an unsupported host support it. Use `auto` or `halfblocks` if previews are blank. Capability discovery happens when opening a workspace; reopen the deck after changing the attached host's graphics configuration.

## Session identity and limits

Reports include the active SDK session ID, including after restoring a saved session. Herdr 0.8.2 accepts custom agent labels for state reporting, but its native session-reference allowlist does not include Slide Builder. That version may acknowledge reports while ignoring their session reference. Herdr needs separate first-class support before it can retain and resume Slide Builder sessions.

This integration does not add `herdr agent start --kind slide-builder`, automatic resume, pane creation, workspace rearrangement, or tools for controlling other agents. State reporting alone is not a guarantee of reliable remote prompting through every input mode.

## Implementation

- `src/integrations/herdr.rs` owns environment discovery, newline-delimited JSON socket transport, graphics probing, and the reporting worker.
- `src/herdr_status.rs` maps application state to reports and host graphics capabilities to a terminal image picker.
- `src/main.rs` connects the adapter to the workspace lifetime and event loop.
- `PreviewImage::with_picker` consumes a generic terminal image picker without knowing about Herdr.

The transport follows Rho's native Herdr adapter. Request deadlines and the response-size tripwire reuse Rho's existing budgets rather than introducing a deck-specific limit. Unit tests use isolated Unix sockets; application-state tests drive the same events and keyboard actions as the TUI.
