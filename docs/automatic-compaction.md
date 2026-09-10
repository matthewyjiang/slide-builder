# Automatic conversation compaction

Slide-builder automatically reduces older model context during long conversations.
It checks context before model requests, including between tool steps in a single
run. There is no setting to enable and no manual command to run.

## When it runs

Compaction starts when the SDK's estimated context reaches 85% of the selected
model's usable context window. The estimate includes messages, images, and
advertised tool schemas. The model catalog supplies the window, with usable-input
and provider-specific overrides taking precedence over advertised capacity.
Slide-builder refreshes catalog metadata on startup and model switches; cached
metadata can still be used offline.

The target is 50% of that window. This is a working target, not a hard truncation
limit. The system prompt and the latest complete exchange are never dropped to
meet it. The retained-tail budget accounts for tool schemas and reserves space
for the summary. A large latest exchange or a long summary can leave the result
above the target. The result must still reduce estimated context; otherwise the
run stops with an explanation rather than silently discarding more history.

If the catalog does not know the model's context window, automatic compaction is
disabled for that model. A conversation notice explains this on startup and model
switches. Choose a catalog model for automatic compaction, or start a new session
if an unknown model reaches its provider's limit. Slide-builder does not invent a
context limit for custom models.

## What changes

The conversation shows a short notice while compaction runs, followed by the
estimated token counts before and after. The current run then continues.

Slide-builder asks the selected model for a portable text summary. It does not use
provider-native compaction endpoints, whose opaque context can be bound to a
provider or model and lost when switching models or resuming elsewhere. The summary
request does not advertise tools or execute deck operations.

Compaction preserves the current system prompt and the recent history verbatim.
Tool calls and all matching results stay together, including interleaved results
and image feedback. The summary records goals, deck constraints, decisions,
changed files, tool results, test results, and unfinished work. It includes
portable reasoning summaries when available. Older images become media-type
markers in the summary request, not base64 text.

Compaction only replaces model context. It does not remove messages or tool
activity from the visible transcript, change deck files, or replay tools. Summaries
can omit details, so inspect the original transcript or deck when exact earlier
content matters.

## Cancellation, failures, and saved sessions

Escape cancels an in-flight compaction through the normal run cancellation path.
An empty or non-reducing summary, a summary-provider error, or a context that cannot
be partitioned stops the run with a readable error. Failed and cancelled
compactions do not install replacement history. You can retry, choose a
larger-context model, or start a new session.

Successful compaction becomes part of the SDK session snapshot. Existing session
checkpoints save that reduced context and its compaction counters alongside the
full UI transcript after a completed, failed, or cooperatively cancelled run and
on graceful exit. There is no separate mid-run disk checkpoint: a crash or forced
kill can lose the current run, including its compaction, since the last saved
checkpoint. See [saved sessions](sessions.md) for storage and recovery details.

Resume restores the reduced history and counters without replaying tools, then
rebuilds the system prompt from the current deck and workspace. Switching models
keeps history but replaces both the summarizing provider and the automatic
threshold. Switching from a large-context model to a smaller one can therefore
trigger compaction before the next model request.

## Implementation

`src/agent/compaction.rs` implements the pinned SDK's `Compactor` trait and the
85% token policy. `compaction_history.rs` owns token-budgeted partitioning and
portable summary input. `runtime_builder.rs` captures the actual advertised tool schemas
for budgeting, and idle model switches replace the provider and compaction settings
under the same guard used to start a turn. The SDK commits replacement history and
emits lifecycle events; the TUI maps those events to existing system-message styles.

The implementation adapts Rho's private model compactor rather than depending on
an unexported type. Tests use scripted providers and the real session, event,
SQLite snapshot, and restore paths without credentials or paid model calls.
