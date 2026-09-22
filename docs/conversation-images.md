# Conversation images

A standalone Markdown image such as `![diagram](images/diagram.png)` can appear directly in the conversation. Images mixed into prose keep their text fallback. The conversation uses the same terminal graphics capabilities and protocol choice as the slide preview, including halfblocks when native graphics are unavailable.

Relative paths resolve against the session working directory. Absolute paths, `~/` paths, and local `file://` URLs are supported. Remote URLs, data URLs, remote file URL hosts, and non-regular files are not loaded. Images never open a desktop viewer or trigger a network request.

The Markdown fallback stays visible while an image loads or if it cannot be decoded. File reads, image decoding, resizing, and terminal protocol encoding run on a background worker. Scrolling clips image rows rather than squeezing the image into the remaining viewport space.

## Cache and integration

`ConversationImages` owns a session-local cache keyed by local path and available cell dimensions. Its picker is supplied by the existing preview setup, so rendering does not query the terminal. A disabled picker leaves Markdown fallbacks unchanged.

Call `poll` to collect results. `conversation_media::place_images` uses `cached_image` to replace ready images' placeholder rows without requesting offscreen files. It wraps load-error diagnostics and adjusts following code-panel copy targets. Request missing images with `request_visible` only for source rows in the viewport. The `image` convenience method combines lookup with an initial nonblocking request, but does not retry evicted images. Source indices preserve the association between Markdown references and loaded images when another image fails. Offset returned placement row ranges when concatenating messages, then render them against the conversation viewport and scroll offset.

Call `end_frame` after every draw, including frames where another view or modal hides the conversation. Placements record their visibility during rendering. Kitty images swap to a pre-encoded standby when they return after being hidden, because compatible hosts can discard hidden image data. The worker replenishes the standby using retained resized pixels, without rereading the file or encoding on the draw thread. If the standby is still pending, the completed replacement retransmits on the next draw.

The decoder and cache inherit the slide preview's 256 MiB allocation budget. Cache accounting reserves the resized RGBA footprint, not the original decoded image size. Kitty also reserves space for standby and retained refresh pixels. Terminal encodings use additional protocol-dependent memory. A single worker and one-slot request/result channels bound in-flight work. Eviction is not a load failure: an evicted image can load again when its fallback enters the viewport, without rereading offscreen transcript images on every frame. `load_error` exposes failed-load reasons, including caught decoder or encoder panics. A panic fails that request rather than stranding the worker queue.

Use terminal height, rather than composer-dependent viewport height, for the available image size so typing does not resize cached images. Clear or replace the cache when changing sessions, and clear it after an accepted render completes to refresh generated files at the same paths. Clear it to retry failed loads. Drawing performs no file metadata checks.

Tests in `src/tui/conversation_images_tests.rs` cover local path handling, asynchronous cache loading, queue saturation, stale results after clearing, fallback-row preservation, cropped halfblock rendering, resized accounting, eviction retries, panic diagnostics, and Kitty retransmission sequences after reactivation. They do not verify a real terminal's Kitty, Sixel, or iTerm2 implementation.
