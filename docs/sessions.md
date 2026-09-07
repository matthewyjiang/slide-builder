# Saved sessions

Each interactive deck opening starts a fresh conversation.

## Resume inside the TUI

Use `/sessions` or `/resume` to browse saved sessions, most recently updated first.
Type to filter, use Up/Down to choose, and press Enter to resume. Escape closes
the picker without leaving your current session. The current session is marked.
Deck paths and models appear on separate rows. Long details use a leading
ellipsis to keep the filename or model identifier visible; filtering still
matches the full values. Keyboard hints sit inside the picker's bottom border.

Switching saves the current session before restoring the selected conversation
and deck. Finish any active run or design import before switching. Missing or
invalid saved decks show an error in the picker and leave your current session
open.

## CLI commands

```sh
slide-builder sessions list
slide-builder sessions continue                 # most recently updated session
slide-builder sessions continue SESSION_ID
slide-builder sessions new ~/another-deck.pptx
slide-builder sessions rename SESSION_ID "Quarterly review"
slide-builder sessions delete SESSION_ID
```

`list` returns JSON with full IDs, names, absolute deck/workspace paths,
provider/model, Unix timestamps, and checkpoint revisions. `continue` accepts an
exact ID and requires an interactive terminal. Without an ID it chooses the most
recently updated session across all workspaces, including renames. `sessions new`
opens or creates the deck and records a new conversation. Outside a terminal it
prints the new ID without authenticating or starting a model request. The existing
`slide-builder new DECK.pptx` command only creates a deck.

## What resume restores

Resume restores SDK conversation history, the selected provider/model and auth
mode, transcript and tool results, active slide, draft and attachment toggle,
design name, and pending design instructions. Credentials still come from the
credential store, not the session database. Current configuration supplies
permissions and renderer settings.

The application rebuilds its system prompt from the current deck and workspace.
Saving unrelated settings does not make a resumed model the global default;
explicitly switching models does. The application reads the deck from disk and
regenerates previews. Missing saved decks or workspaces produce an error rather
than silently creating replacements. Restoring never replays tools or rolls back
deck files. Historical tool results may describe an older version of a deck that
has since been edited externally.

## Storage and recovery

The application-wide SQLite database is at
`$XDG_DATA_HOME/slide-builder/slide-builder.sqlite3`, normally
`~/.local/share/slide-builder/slide-builder.sqlite3` on Linux. It stores the SDK's
versioned snapshot JSON alongside typed app state and queryable metadata. The
database is owner-readable/writable, but it is not encrypted. Conversations,
tool arguments/results, and attached images can contain sensitive information.
Deleting a session removes its database record, not the deck or other files, and
is not a secure-erasure guarantee. The old, unwired per-project JSON stub has no
automatic migration.

Checkpoints commit snapshot and UI state together after completed, failed, or
cooperatively cancelled turns, and on graceful exit or a deck/session switch.
Graceful shutdown cancels and waits for an active run before saving. A crash or
forced kill can lose work since the last checkpoint; tools may already have
changed the deck on disk. Draft/UI-only changes save on graceful exit or a switch.
Save errors stop the session and are reported after returning to the terminal.

Revision checks reject competing history saves and saves after another process
deletes the session. Renames do not interrupt active work, and subsequent
checkpoints preserve the new name. Metadata updates never rewrite snapshots.
This is optimistic conflict detection, **not an exclusive session or deck lock**.
Do not edit the same deck from two running processes: revision checks cannot undo
tool side effects or protect independent sessions sharing a deck.
