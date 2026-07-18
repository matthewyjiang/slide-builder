# slide-builder - AI-first PPTX builder TUI on rho-sdk

## Context

Build `slide-builder`, a terminal-first fullscreen ratatui app: a chat-driven AI agent that creates and edits PowerPoint (.pptx) decks. Decks live in a user-chosen decks directory; opening the app inside a repository additionally gives the agent supervised coding-agent access to that repo (reads free, writes/commands approval-gated) so it can generate assets for the deck. Design guidance comes from selectable **design packages** (directories with a `DESIGN.md`, e.g. `~/gt-slides-workspace/`), injected into the agent's context selectively. The agent's pptx "hands" are **native Rust tools** built on the `pptx-handler` crate from [OfficeCli-rust](https://github.com/RainLib/OfficeCli-rust) (see below) - in-process, no shipped binary. Base: the user's own **rho-sdk** (`~/rho/crates/rho-sdk`).

**Locked decisions:** Linux-only v1 · use the extracted rho library crates · native `pptx-handler` crate as the primary pptx engine · optional external OfficeCLI compatibility fallback · per-slide PNG generation from the handler's HTML through a headless Chromium-family browser · inline and fullscreen slide navigation in Kitty-graphics terminals (Kitty and Ghostty are the initially supported terminals) · all state under app data dirs (XDG), repo untouched.

**Verified:** rho-sdk has everything the embedding needs - `Rho::builder()` → `Session` → `Run` with `RunEvent` stream, `Tool` trait, `WorkspacePolicy` + `approval_channel()` (`workspace/approval.rs:154`), `Workspace::with_granted_root` (`workspace/path.rs:106`), `SessionOptions::from_snapshot` (`client.rs:147`), `UserInput::text_and_images` (`session.rs:41`). `ratatui-image 11.0.6` requires `ratatui ^0.30.1` - compatible with the current ratatui 0.30 line. Canonical embedder example: `~/rho/crates/rho-sdk/examples/questionnaire_approval.rs`; reference wiring: `~/rho/crates/rho/src/app/interactive_runtime.rs`.

---

## Completed prerequisite

The rho extraction is merged in [rho PR #387](https://github.com/matthewyjiang/rho/pull/387): `rho-providers` and `rho-agent-tools` are reusable library crates, and slide-builder reimplements the rho-branded prompt and skill-loading semantics.

---

## Part B - slide-builder (this repo)

### Crate & deps
Single Linux binary crate. Key deps: `rho-sdk`, `rho-providers`, `rho-agent-tools` (path deps to `~/rho/...` during dev, crates.io once published), `pptx-handler` and `handler-common` (git deps pinned to the same exact OfficeCli-rust revision), `tokio` (full), `ratatui 0.30`, `ratatui-image 11`, `crossterm 0.29` (event-stream), `image 0.25`, `notify`, `directories`, `sha2`, `serde`/`toml`/`serde_json`, `thiserror`/`anyhow`, `pulldown-cmark`. A system Chromium-family browser is a preview runtime dependency, not a Rust dependency; probe configured `browser_path` first, then common Linux Chrome/Chromium binary names.

Pin both OfficeCli-rust crates to one audited commit so Cargo cannot resolve mismatched workspace revisions:
```toml
pptx-handler = { git = "https://github.com/RainLib/OfficeCli-rust.git", rev = "acabe4959a37235dd587bbcc788565f19a824bb7" }
handler-common = { git = "https://github.com/RainLib/OfficeCli-rust.git", rev = "acabe4959a37235dd587bbcc788565f19a824bb7" }
```
Update this revision deliberately after running deck fixture and render-fidelity tests.

### PPTX engine: native `pptx-handler` crate
[OfficeCli-rust](https://github.com/RainLib/OfficeCli-rust) is an Apache-2.0 Rust rewrite of iOfficeAI's OfficeCLI. Its workspace contains embeddable handler crates in addition to the CLI. Slide-builder takes an **exact-revision git dependency on `pptx-handler`** and uses its `PptxHandler` through the shared `handler-common::DocumentHandler` trait. The handler crates are not yet published on crates.io, so the git revision is part of the reproducible dependency contract.

The verified in-process API covers:

- `PptxHandler::open(path, editable)` and `save()`
- path-based `add`, `set`, `remove`, `move_element`, `copy_from`, and `swap`
- structured `get` and `query`
- text, annotated, outline, stats, issues, HTML, and SVG views
- text extraction with character-offset-to-path mappings
- `raw` and `raw_set` for part-level XML/XPath escape hatches
- `validate` after mutation

`PptxHandler` internally uses `RefCell`, so it is not shared across asynchronous tasks. A small `DeckEngine` adapter owns a per-deck lock and runs every operation in `spawn_blocking`. Mutations copy the deck to a temporary sibling file, open that working copy, apply the operation, validate, save, reopen to verify the package, then atomically rename the sibling over the original using Linux same-filesystem semantics. This keeps automatic rendering and file watchers from observing a half-written package and preserves the original if validation fails. Each committed mutation increments a per-deck generation. Under the deck lock, rendering takes a private snapshot of that generation; browser rendering happens after releasing the lock, and stale generation results are discarded.

New blank decks come from an embedded, validated `.pptx` fixture with known slide dimensions, theme, fonts, master, layouts, relationships, and sensible defaults. Treat the fixture as versioned product source and test it in PowerPoint-compatible and LibreOffice readers. Template-based creation copies the user's selected `.pptx` before opening it with the handler.

**Why native is primary:** deck operations are typed rho-sdk tools, return structured results, require no separately shipped runtime, and are authorized as writes to the deck path rather than as arbitrary processes. Normal deck editing therefore requires no command approval in Supervised mode.

**Optional compatibility fallback:** if the native handler reports an unsupported operation and an `officecli` binary is present, the skill may recommend the equivalent external command through `rho_tools::shell_tool`. This is never invoked silently by a native deck tool. It remains an explicit, approval-gated Process operation and is not required or bundled with slide-builder. If upstream API churn becomes disruptive, pinning gives us time to fork or vendor the Apache-2.0 handler crates.

### Module layout
```
src/
  main.rs  config.rs  paths.rs  design.rs  skills.rs  prompt.rs
  agent/    mod.rs (AgentHandle) runtime.rs (Rho wiring) policy.rs (SlidePolicy)
            deck_engine.rs (safe pptx-handler adapter) deck_tools.rs (rho-sdk tools)
            tools.rs (render_deck, set_active_slide) session_store.rs
  render/   mod.rs (RenderService: queue/debounce/generations) pipeline.rs browser.rs cache.rs
  tui/      mod.rs app.rs event.rs chat.rs outline.rs preview.rs slideshow.rs statusline.rs
            modal/{approval,questionnaire,deck_picker,template_picker,design_picker,setup}.rs
  builtin_skills/  slide-builder-pptx/  deck-design/  deck-assets/
```

### TUI: layout & event loop
Chat transcript (streaming text + in-place-updating tool cards) left; preview pane (ratatui-image using the Kitty graphics protocol) + slide outline right; multiline input below; status bar (deck · design pkg · mode · model · tokens). The preview shows `Slide N / M` and distinct rendering, stale, render-failed, and renderer-unavailable states. Keys: Tab focus-cycle, Enter send, Esc cancel run or leave slideshow mode, Ctrl+P design picker, Ctrl+O deck picker, Ctrl+R re-render, Left/Right or k/j previous/next slide, g/G first/last slide, f fullscreen slideshow mode, **Ctrl+V attach active slide PNG to next message** (`UserInput::text_and_images` - the human-in-loop visual QA path, since `ToolOutput` is text-only). Slideshow mode uses almost the entire terminal for the current static slide; v1 does not play animations, transitions, video, or other PowerPoint runtime behavior.

V1 requires a terminal implementing the Kitty graphics protocol for preview, officially Kitty and Ghostty. Startup probes actual protocol support rather than relying only on `$TERM`. An unsupported terminal or missing browser gets a direct diagnostic, but deck editing and recovery remain available without previews; there is no halfblock fallback in v1.

Single unified event bus - every async source forwards into one `mpsc::UnboundedSender<AppEvent>`; main task drains, mutates `App`, redraws:
```rust
enum AppEvent {
  Input(crossterm::event::Event),        // EventStream pump
  Run(RunEvent), RunHandleReady(Run),    // run pump: session.start() → while run.next_event()
  Approval(PendingApproval),             // approval_channel receiver pump → modal
  RenderDone{generation, manifest} / RenderFailed{generation, error},
                                           // RenderService headless-browser jobs
  DeckFileChanged, Tick,
}
```
One active run at a time; `ToolCallId → transcript index` map updates tool cards through Proposed/Started/Updated/Finished. Text deltas coalesced per frame; markdown parsed on message completion.

### Agent wiring
- **Tools:** semantic native deck tools on `pptx-handler` (below) + `rho_tools::shell_tool` + `coding_tools` (read/write/edit/list); reimplemented `skill` tool (~80 lines, pattern: rho `tools/sdk_registry.rs:218`); native `render_deck` (kicks RenderService, waits via oneshot, returns the generation and per-slide status) and `set_active_slide {index}` (pure UI sync).
- **Native deck tools** (`agent/deck_tools.rs`): normal authoring uses a small semantic surface operating implicitly on the active deck: `slide_create`, `slide_duplicate`, `slide_delete`, `slide_reorder`, `text_add`, `image_add`, `shape_add`, `element_update`, `deck_inspect`, and `deck_validate`. These tools enforce coordinate units, slide bounds, stable element IDs, theme-aware defaults, payload limits, and actionable validation errors. `deck_create {path, from_template?}` and explicit deck selection are the only normal operations accepting a deck path. Advanced path-based `add`/`set`/`remove`/`move`/`copy`/`swap`, query/view/extract, and `raw`/`raw_set` tools remain available for unsupported cases, but the built-in skill directs the agent to semantic tools first. Mutations go through `DeckEngine` transactions, return stable affected IDs plus post-state, and trigger rendering. Native tools authorize only the resolved active deck as a Write and never claim Process capability; reject traversal, symlink escapes, ambiguous selectors, and oversized inputs.
- **Workspace:** `Workspace::new(repo_cwd).with_granted_root(decks_dir).with_granted_root(design_pkg_dir)`.
- **`SlidePolicy` (custom `WorkspacePolicy`), default mode Supervised:** Read/Skill/InstructionDiscovery → Allow · Write under decks_dir or render cache → **Allow** (the deck is the product - this covers the native deck tools) · Write elsewhere (repo) → RequireApproval · agent-requested Process → RequireApproval · agent-tool Network → Deny (v1). Provider transport is outside this tool policy and may use the network. The app-owned renderer may launch only the configured/probed browser binary with hard-coded arguments and paths under its private render directory; it is not exposed as a general agent Process capability. Optional `officecli` fallback commands remain approval-gated. `AllowForSession` remembered approvals make Supervised tolerable. Modes `auto`/`plan` configurable.
- **Approvals:** `approval_channel(16)`; receiver pump → modal → `pending.respond(AllowOnce|AllowForSession|Deny)`.
- **System prompt** (in order): base slide-builder prompt (active deck abs path, decks dir, repo cwd, workflow rules) → active design package `DESIGN.md` full text in `<design_guidelines>` + one-line list of supplementary file paths (read on demand - the "selective context" mechanism) → AGENTS.md from cwd ancestors → `<available_skills>` listing (project > user `~/.agents/skills` > builtin, dedupe by name; format per rho `prompt.rs:92`) → deck state line. On design/deck switch mid-session: rebuild `Rho`, carry history via `SessionOptions::history(...)`, and insert an explicit synthetic transition event naming the old and new deck/design so the model does not apply stale state.

### Design packages
Package = directory containing `DESIGN.md` (H1 = display name). Registry in global config: explicit `[[design_packages]]` entries + `design_scan_dirs` scanned at startup. `~/gt-slides-workspace` registers as-is. Picker on Ctrl+P; selection stored per-project; "None" valid. When creating a deck from a package containing PPTX files, always let the user choose among "blank deck" and every discovered template in a dedicated template picker. Prefer `template.pptx` in ordering but never select it silently; show relative path, file size, and optional package metadata. Copy the chosen template into the decks directory before editing it.

### Config & state (XDG via `directories`)
`~/.config/slide-builder/config.toml`:
```toml
schema_version = 1
decks_dir = "~/decks"
provider = "anthropic"; model = "..."; reasoning = "medium"
permission_mode = "supervised"
preview = { enabled = true, protocol = "kitty", width = 1600, scale = 2 }
render  = { browser_path = "auto", debounce_ms = 1500, timeout_ms = 60000, keep_generations = 5 }
compat  = { officecli_path = "officecli", detect_optional = true }
[[design_packages]]  name = "georgia-tech"; path = "/home/emgym/gt-slides-workspace"
design_scan_dirs = ["~/slide-designs"]
```
`~/.local/share/slide-builder/projects/<sha256(cwd)[..16]>/`: `project.toml` (repo_path, active_deck, design_package, session_id) · `sessions/<id>.json` (`SessionSnapshot` + display-transcript sidecar) · `render-cache/`.

### Built-in skills (embedded `include_str!`, materialized to `~/.local/share/slide-builder/skills/` so relative script paths work; shadowable by user/project skills)
1. **slide-builder-pptx** - how to build decks with semantic native deck tools first: stable IDs, coordinate and sizing conventions, theme-aware layout patterns, transaction and validation behavior, when advanced path or `raw_set` operations are justified, and the render-to-attach-to-fix visual-QA loop. It also documents the optional, explicit external `officecli` fallback for unsupported native operations. No proprietary pptx skill or scripts are distributed.
2. **deck-design** - how to apply a design package: read DESIGN.md first, when to load TEMPLATE-REFERENCE.md, layout/chart heuristics, fallback when no package.
3. **deck-assets** - generating charts/diagrams from the host repo (matplotlib/mermaid → PNG at slide DPI, or mermaid → native shapes via the deck tools), embedding into slides.

### Preview and slide-navigation pipeline
`pptx-handler` renders a substantially richer HTML preview than its current basic SVG view, but it does not expose raster PNG output. V1 implements HTML-to-PNG in slide-builder itself, following the original non-Rust OfficeCLI approach; it does not call OfficeCLI or LibreOffice:

1. Under the deck lock, snapshot the committed generation and call `PptxHandler::view_as_html` in `spawn_blocking`.
2. Write an immutable, self-contained capture HTML file under that generation's private cache directory. Inject capture CSS/JS that selects exactly one `.slide-container[data-slide="N"]`, removes preview chrome and notes, preserves the presentation aspect ratio, and emits a deterministic ready marker after fonts and layout settle.
3. For each slide, launch the probed Chromium-family browser directly with app-controlled arguments such as `--headless=new`, `--hide-scrollbars`, an isolated temporary `--user-data-dir`, exact aspect-matched `--window-size`, `--force-device-scale-factor`, a virtual-time/wall-clock budget, and `--screenshot=<temporary PNG>`. Keep the browser sandbox enabled; do not copy the original OfficeCLI's `--no-sandbox` default. Use a bounded concurrency limit so large decks do not start one browser process per slide at once.
4. Decode and verify each PNG with the `image` crate, then atomically publish a `RenderManifest`. Discard the entire result if a newer deck generation exists; never mix slides from different generations.
5. Display the active slide with `ratatui-image` through the Kitty graphics protocol. Navigation changes only the active PNG and does not rerender.

Rendering is offline. Post-process the generated HTML with a restrictive CSP, reject remote URLs, and bundle any renderer resources needed for supported KaTeX/Three.js content rather than allowing Chromium to fetch CDN assets. Unsupported dynamic content gets a visible placeholder. The browser process receives only the capture HTML and output paths beneath the private render directory. Browser stdout/stderr is bounded and captured for diagnostics, and jobs have a hard timeout and process-tree cleanup.

Triggers: explicit (`Ctrl+R`, `render_deck` tool) plus automatic after a successful native mutation, debounced 1.5 seconds; `notify` watcher later. While a newer generation renders, continue showing the last good PNG with a stale badge. Report mutation failure, validation failure, rendering, stale preview, render failure, and renderer/terminal unavailability as distinct states.

Cache path: `render-cache/<deck-hash>/<renderer-version>-<width>x<height>@<scale>/manifest.json` plus `slide-0001.png`, etc. The key covers deck bytes, handler revision, slide-builder renderer version, dimensions, and scale; retain the last five complete generations and clean abandoned temporary directories on startup. Deck creation and editing do not depend on Chromium or terminal image support.

### Session persistence
On run Completed/Failed/Cancelled: `session.snapshot()` → JSON. On open with stored `session_id`: `SessionOptions::from_snapshot`; corrupt/incompatible → fresh with notice.

---

## Milestones (working app at each boundary)

- [x] **M0 - authoring and rendering feasibility gate.** Build `DeckEngine` plus a small CLI harness before the full TUI. Create the same representative three-slide deck from the embedded fixture and a user-selected GT template using text, images, positioned shapes, theme styling, and slide duplication. Save, reopen, validate, and inspect in PowerPoint-compatible software and LibreOffice. Render `pptx-handler` HTML to one PNG per slide with the custom Chromium pipeline and compare fidelity against the source deck. Record the native operations required, raw-XML frequency, unsupported content, browser latency, and font behavior. This is the go/no-go gate for the semantic tools and handler-based preview.
- [x] **M1 - thin end-to-end agent with navigable slides.** Minimal config, one model, one active deck, built-in skill, prompt assembly, Rho wiring, SlidePolicy + approval modal, streaming chat/cancel, the smallest useful semantic deck-tool set, serialized transactions, PNG RenderService/cache, active-slide state, Kitty-protocol display, previous/next navigation, and all preview status states. No design picker or persistence yet. **Demo: open in a repo → "make a 3-slide intro deck about this repo" → agent builds and validates it without process approvals → each slide appears in the TUI and can be navigated.**
- [x] **M2 - product shell + design packages.** First-run setup; deck, design, and explicit template pickers; DESIGN.md injection + session transition event; complete semantic tool surface and advanced escape hatches; outline pane; `render_deck` + `set_active_slide`; fullscreen static slideshow; Ctrl+V current-slide attachment. Optional `officecli` fallback remains approval-gated.
- [x] **M3 - persistence + external changes.** Snapshots/resume, deck watcher, generation-safe automatic rerendering, cache cleanup/recovery, and usage display.
- [x] **M4 - polish.** Help overlay, robust error surfaces (including credential hints → `rho login`, missing browser, unsupported terminal, and missing fonts), compaction config, and model picker from catalog.
- [x] **M5 - hardening and Linux packaging.** Deterministic PTY smoke tests (reuse `rho-tui-pty` pattern + `ScriptedProvider` env fixture), Linux package/install documentation, browser discovery coverage, and fixture/fidelity qualification.

## Verification
- [x] Per-milestone unit tests: config round-trip, SlidePolicy decision table (pattern: rho `permission_tests.rs`), prompt-assembly snapshots including deck/design transitions, SKILL.md parsing, template discovery/selection ordering, render cache keys, generation supersession, and browser argument construction.
- [x] Deck integration: tests on the embedded blank and multiple template fixtures; round-trip open-mutate-validate-save-reopen; failed validation preserves the original; crash injection before save, during save, before/after rename, and before state persistence; unrelated charts, groups, notes, hyperlinks, custom XML, and relationships survive an edit. Test path traversal, symlink escape, malformed XML, ambiguous selectors, invalid geometry, oversized media, and payload limits.
- [x] Render integration: representative handler HTML → per-slide PNG with correct count, dimensions, aspect ratio, and non-empty decoded pixels; stale render result cannot replace a newer generation; concurrent mutation/render snapshot isolation; browser timeout/process cleanup; offline/CSP enforcement; missing-browser behavior. Skip browser integration only when no supported Chromium-family binary is installed and report the skip clearly.
- [x] Visual fidelity: keep small golden fixture decks covering text, images, theme/master content, tables, groups, charts, math, and backgrounds. Compare PNGs with thresholded perceptual metrics, with deliberate baseline review when the pinned handler or browser qualification version changes.
- [x] Agent integration: `ScriptedProvider` approval loop (pattern: `rho-sdk/examples/questionnaire_approval.rs`) and model-facing semantic-tool errors that identify the invalid field, expected values, selector/ID, and unchanged-deck guarantee.
- [x] Manual smoke each milestone: real run against the GT design package in a real repo; explicit template choice; static slide navigation and fullscreen mode on current Kitty and Ghostty releases; open generated decks in PowerPoint-compatible software and LibreOffice.

## Risks
- OfficeCli-rust is young (v0.1.x, unpublished handler crates) - pin an exact git revision, isolate it behind `DeckEngine`, qualify authoring and HTML fidelity in M0, and fork or vendor if upstream churns. The optional external CLI provides a manual compatibility fallback, not an automatic dependency.
- The pinned handler's SVG view is only a basic textual approximation; v1 intentionally uses its richer HTML view. HTML renderer gaps or regressions directly affect PNG fidelity - maintain representative golden decks and a renderer-versioned cache.
- Headless browser availability, startup latency, font differences, crashes, and hostile document content - Linux-only browser probing, bounded concurrency, debounce, isolated profiles, sandbox enabled, offline CSP, strict time/output limits, and stale-preview behavior. Large templates such as the 18.9 MB GT template are part of M0 performance qualification.
- Kitty graphics protocol quirks across terminals - officially qualify current Kitty and Ghostty releases and show a clear unsupported-terminal state. V1 deliberately has no halfblock fallback.
- Tool results are text-only in rho-sdk - the agent cannot see renders automatically; Ctrl+V attaches the current PNG for visual QA, with a possible future rho-sdk image-tool-output enhancement.
- Active-deck authorization, multiple granted roots, symlinks, app-owned browser execution, and the custom policy need careful decision-table and adversarial tests.
- Static PNG navigation is not a PowerPoint runtime - animations, transitions, video, and interactive content are explicitly out of scope for v1.
