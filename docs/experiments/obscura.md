# Obscura renderer investigation

Initial investigation measured September 5, 2026, before changing production code.
The subsequent isolated-default implementation and its integrated benchmarks
are documented in `obscura-integration.md`. Recommendations below record the
initial decision, before OS isolation was implemented.

## Recommendation

Keep Chromium as the default. Obscura is fast enough to justify further work, but it is not a safe executable substitution. The latest tested release, **Obscura v0.2.2**, ignores the CSP that our capture builder relies on. A probe passed through the actual `build_capture_html` function loaded a prohibited external stylesheet in Obscura; Chromium blocked it.

An experimental backend should run as a separate process inside OS-enforced filesystem and network isolation. Initially support device scale 1 only. Do not embed Obscura in the TUI process or enable it by pointing `render.browser_path` at its binary.

## Versions and baseline limitation

- Obscura v0.2.2, official `obscura-x86_64-linux.tar.gz`, with native rendering enabled. Source commit `a1e09de68c7617b8079fbb1661b0548c501971c1`.
- Chromium Headless Shell `149.0.7827.55`, already installed by Playwright.
- slide-builder production source at `d26e9dab1b4f6aeb96b5677c3cafeda8cdf0350f`, using its pinned OfficeCli handler and capture builder.
- Linux x86_64, AMD Ryzen 5 5600X, six cores and twelve hardware threads.

**The working baseline is Chromium Headless Shell, not the full Chrome executable currently expected by browser discovery.** Installed Chrome for Testing `149.0.7827.55` failed to produce a screenshot within our 60-second timeout with the current capture arguments. It also hung on a minimal static page. Shorter diagnostic runs with GPU disabled, headless Ozone, and sandbox disabled did not resolve it. Sandbox disabling was diagnostic only and was not used in any reported benchmark. The cause remains undiagnosed.

Headless Shell completed captures with the production argument list, including the sandbox-preserving flags. This is a useful Chromium rendering comparison, but not a measurement of a working full-Chrome deployment or a persistent CDP session.

We initially tested v0.2.1 because v0.2.2 had no downloadable assets at the start. Its binaries appeared during the investigation, so both benchmark suites and the sanitizer-path security probe were rerun on v0.2.2. Results below are v0.2.2 only.

## Performance

The primary workload is a generated **12-slide PPTX**, exported through `DeckEngine::snapshot` and `build_capture_html`. Each slide has text, colored shapes, an ellipse, and an embedded raster image. Each capture HTML contains the entire deck, just as it does in the app, with CSS selecting the requested slide. Each HTML file is approximately 54 KB. Twelve slides give three complete scheduling waves at the app's current concurrency of four; this is a defined synthetic workload, not a claim about typical customer decks.

| Measurement, median | Chromium Headless Shell | Obscura v0.2.2 |
| --- | ---: | ---: |
| Capture latency, concurrency 1 | 211.7 ms | 43.4 ms |
| All 12 slides, concurrency 1 | 2.700 s | 0.687 s |
| All 12 slides, concurrency 4 | 0.873 s | 0.191 s |
| Peak summed process-tree RSS, concurrency 1 | 526 MiB | 52 MiB |
| Peak summed process-tree RSS, concurrency 4 | 2,043 MiB | 206 MiB |

For this workload, whole-deck capture was **3.9 times faster serially and 4.6 times faster at concurrency four**. Sampled summed RSS at concurrency four was about one tenth of the Chromium baseline.

A separate six-fixture batch covered a generated single-slide PPTX, the blank starter PPTX, and synthetic typography, SVG, CSS effects, and grid/flex layout. At concurrency four, its median batch time was 0.592 seconds for Chromium and 0.115 seconds for Obscura. These smaller inputs do not replace the whole-deck result above.

### Method and limitations

- Same HTML bytes, fonts available on the host, 1280×720 CSS viewport, and device scale 1 for both engines.
- Five paired measured repetitions per concurrency setting, with alternating engine order. This is exploratory, not a statistical confidence claim.
- One full warmup batch per engine. Every measured capture still starts a new process; Chromium gets a new profile. Filesystem and font caches are warm. This is not an OS cold-boot benchmark.
- Obscura uses `fetch file://... --screenshot ... --wait 0`. A follow-up with its default adaptive settling produced identical PNG hashes on one generated slide and median latency of 46.3 ms versus 45.7 ms with `--wait 0`. This does not establish readiness for arbitrary assets.
- The primary suite has 240 timed captures; the mixed suite has 120. All 360 returned successfully and decoded as 1280×720 PNGs. A successful PNG does not establish fidelity or resource completeness.
- Per-capture timing covers process launch through exit. Batch timing also includes PNG validation, profile cleanup and Python orchestration. It excludes PPTX-to-HTML conversion, staging HTML writes, application retries, checksumming/cache publication and terminal image display. It is not a full TUI refresh benchmark.
- A monitor samples the sum of RSS for renderer processes and descendants every 5 ms. RSS counts shared mappings more than once and sampling can miss brief peaks. These are not unique physical-memory or PSS measurements. The monitor's overhead is included.
- Blocking `wait()` plus a separate deadline timer avoids Python's POSIX `wait(timeout=...)` polling, which otherwise rounds short captures up by tens of milliseconds.
- No browser reuse or persistent Chromium benchmark was performed. That could reduce the startup cost without changing rendering engines.

Raw batch and per-capture timings, versions, fixture hashes, and pixel-difference measurements are in `obscura-results.json`. Full commands, logs, PNGs and diff images remain under `/tmp/slide-obscura-spike/benchmark-deck-v022/` and `/tmp/slide-obscura-spike/benchmark-mixed-v022/` on the investigation machine.

## Rendering compatibility

All tested dimensions were correct. The 12 selected slides produced 12 distinct screenshots with each engine, so this was not a benchmark of repeatedly capturing the same visible slide.

Outputs are not identical:

- On the generated 12-slide deck, approximately 5.39–5.40% of pixels differ. Mean absolute channel error ranges from 2.13 to 2.14 on the 0–255 scale.
- The blank starter image is identical.
- Synthetic typography has 2.18% differing pixels; SVG has 8.29%; CSS effects have 15.44%; grid/flex layout has 0.33%.

Exact pixel disagreement includes harmless antialiasing differences. Conversely, a low average error can conceal an important missing label. These measurements are descriptive, not fidelity acceptance criteria. No representative external deck corpus or human visual approval was available, so this is not enough to approve default replacement.

Other findings:

- `OBSCURA_SHOT_W` and `OBSCURA_SHOT_H` control the CLI viewport. A v0.2.1 probe verified 640×360 output; the v0.2.2 benchmarks verified its 1280×720 default. The source contains the same environment-based sizing path in both releases.
- CLI viewport sizing is not device-pixel-ratio scaling. Our existing `CaptureOptions.scale` changes density independently of CSS layout. The CLI capture path has no equivalent proven switch; non-1 density needs a verified library/CDP path.
- A relative local PNG failed to render in both tested releases under zero wait, default wait, and an explicit one-second wait. The same PNG rendered as a data URI in the initial v0.2.1 control. Our generated handler fixture embeds its image, so its successful PNG does not clear this local-file compatibility issue.
- Embedded webfont probes were inconclusive because valid and invalid font controls produced identical images. Font completeness needs a working positive control.
- Source inspection shows bounded resource warmup, including a 128-candidate cap, a separate resource deadline, and a fallback loader. Capture can succeed without proving that every resource loaded. Do not assume longer settling fixes all missing resources.

## Security probe through the real capture builder

The fixture exporter writes `security/capture.html` through the production `build_capture_html`, plus a harmless sibling `sentinel.css`. The HTML links to `../sentinel.css` in the body, after the injected CSP. That CSS changes the slide background to green.

The offline scanner accepts this relative link. The injected `style-src 'unsafe-inline'` permits inline CSS but forbids this external stylesheet.

| Engine | Pure green pixels out of 921,600 |
| --- | ---: |
| Chromium Headless Shell | 0 |
| Obscura v0.2.2 | 920,186 |

Both commands succeeded. Obscura loaded the prohibited stylesheet outside the capture subdirectory. The resource was deliberately created for this test; no unrelated user files were accessed. Logs and PNGs are in `/tmp/slide-obscura-spike/followups-v022/`.

Additional raw-browser probes on v0.2.1 showed inline script execution despite `script-src 'none'`, inline and external styles despite `style-src 'none'`, and a data image despite `img-src 'none'`. These isolate browser behavior; script tags normally get stripped by slide-builder, so they are not themselves an end-to-end app exploit. The external stylesheet probe above is the directly demonstrated app-path failure on v0.2.2.

Source findings explain why a new adapter needs its own isolation:

- Resource policy allows HTTP, HTTPS and data URLs, plus file URLs for file documents. File reads do not constrain paths to a capture directory.
- The CLI has a private-network opt-in, not an offline switch. Private-address blocking is not an egress ban. A separate synchronous resource loader also needs consideration before trusting transport interception.
- The upstream security policy states that V8 runs in-process and that watchdogs/panic guards are not OS isolation. A killable subprocess is useful for cancellation, but does not replace a filesystem or network sandbox.
- Our string-based scanner permits relative links and is not a complete HTML/CSS sanitizer. It cannot compensate for missing CSP. Also, the existing Chromium path should not be described as a filesystem jail or independently proven full offline sandbox; its CSP is inserted after original head content. That deserves separate review rather than assuming the current wrapper is perfect.

Relevant source references at Obscura commit `a1e09de68c7617b8079fbb1661b0548c501971c1`:

- `crates/obscura-cli/src/main.rs:740–752,791–845,917–927`: viewport, deadlines, settling and capture.
- `crates/obscura-browser/src/page.rs:122–140,1903–2005,3578–3745`: allowed schemes, stylesheet loading and screenshot resource warmup.
- `crates/obscura-net/src/client.rs:757–774`: local file reads.
- `crates/obscura-render/src/paint.rs:7394–7446`: synchronous resource loader.
- `SECURITY.md:83–104`: lack of OS isolation and recommended containment.

The full source investigation, including v0.2.1 references and raw probe commands, remains in `/tmp/slide-obscura-spike/compatibility.md`.

## Reproduce

Prerequisites: the project's working Rust toolchain, Python with Pillow and psutil, a Chromium-family capture executable, and an Obscura release **with rendering enabled**. No global toolchain or package installation was performed for this experiment. The shell's default Rust was too old for existing dependencies; the harness's existing newer toolchain built the example successfully.

```sh
# Output directories must not already exist.
cargo run -j 8 --example renderer_fixtures -- /tmp/renderer-fixtures --slides 12

python3 scripts/benchmark_renderers.py \
  --chromium /absolute/path/to/chrome-headless-shell \
  --obscura /absolute/path/to/obscura \
  --fixtures /tmp/renderer-fixtures \
  --fixture-prefix deck-0- \
  --output /tmp/renderer-benchmark
```

Omit `--fixture-prefix` to include the blank starter and synthetic probes. Omit `--slides 12` for the smaller generated fixture set. The initial mixed-suite title predates numbered slide titles in the exporter; its exact inputs are identified by recorded hashes. To investigate actual decks instead, supply PPTX paths after the output directory:

```sh
cargo run -j 8 --example renderer_fixtures -- /tmp/real-deck-fixtures ./example.pptx
```

The exporter reads supplied decks without mutating them. Do not run untrusted HTML/decks through Obscura outside external isolation.

To reproduce the policy probe on the generated harmless input:

```sh
/path/to/obscura fetch file:///tmp/renderer-fixtures/security/capture.html \
  --screenshot /tmp/obscura-policy.png --wait 0

/path/to/chrome-headless-shell --headless=new --hide-scrollbars \
  --window-size=1280,720 --force-device-scale-factor=1 \
  --virtual-time-budget=60000 --user-data-dir=/tmp/chromium-policy-profile \
  --screenshot=/tmp/chromium-policy.png \
  file:///tmp/renderer-fixtures/security/capture.html
```

## Next decision

The performance case is strong for these inputs. The next useful work is **sandboxed experimental integration and broader fidelity checks**, not more repetitions of this small benchmark. Require verified network/filesystem denial, asset readiness and scale handling before enabling Obscura for imported decks. Keep renderer identity in cache keys distinct, and retain Chromium until real-deck comparisons pass.
