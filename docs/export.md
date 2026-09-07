# Export PDF

In the prompt, run:

```text
/export pdf
```

This writes a PDF beside the active PowerPoint file, with the same filename stem.
For example, `/home/matt/talk.pptx` exports to `/home/matt/talk.pdf`.

To choose a destination:

```text
/export pdf output/review copy.pdf
```

Relative destinations resolve from the application's working directory. The
parent directory must already exist. The entire text after `pdf` is the path,
so spaces need no quotes. Shell expansion, including `~`, is not supported.
Explicit destinations must end in `.pdf`.

Existing files, directories, and symlinks are never overwritten. Choose a new
name or move the existing file first. The completed PDF is published without
clobbering a destination created while export was running. Failed exports leave
no partial PDF at the requested path.

Bare `/export` shows usage. Other formats produce a validation message rather
than sending a request to the agent. Export also appears in slash suggestions
and the action palette.

## Progress and snapshots

The status bar shows `Exporting PDF`, and the conversation reports the output
path or an error. Finish an active agent run or design import before starting
an export. Only one export runs at a time. The interface remains available while
rendering and PDF assembly run in the background. Exiting or switching workspaces
waits for an already started export to finish.

Export captures the current committed deck through `DeckEngine`, not the last
preview images. Later edits do not change that snapshot. Slide order, physical
page size, and rendering content all come from the captured version. Portrait,
standard, and custom slide sizes keep their aspect ratio. Every slide fills one
PDF page, without added page margins.

## Rendering and output quality

PDF export uses the configured renderer, Obscura by default or the explicitly
configured Chromium PNG backend. It works when live preview is disabled, but a
working renderer is still required. It never invokes LibreOffice or browser PDF
printing.

Capture width and scale follow the preview configuration; capture height follows
the deck's actual aspect ratio instead of assuming 16:9. Existing renderer size
and timeout checks also apply to export. The export owns a separate temporary
render cache and removes it after completion or failure.

The PDF embeds losslessly compressed slide images using `pdf-writer`. Text is
rasterized, not selectable or searchable. Links, animations, speaker notes, and
editable PowerPoint objects are not included. Rendering has the same font and
PowerPoint fidelity limitations as PNG previews. Keep the `.pptx` as the editable
source.

## Validation

Focused command, publication, and PDF assembly tests run with:

```sh
cargo test -j 8 --lib export
```

The real Obscura export test requires Linux user namespaces, bubblewrap,
`pdfinfo`, and `pdftoppm`:

```sh
cargo test -j 8 --test export_pdf -- --ignored
```

It exports a two-slide portrait snapshot after changing the source deck, checks
page count and physical dimensions with `pdfinfo`, and rasterizes the PDF to
check slide order and full-bleed pixels.
