# Native measurements

The 0.7 host accepts a renderer and text engine explicitly. Cairo
does not initialize OpenGL or retain a full-window client framebuffer. Fonts and
platform services are selected independently. The current Fire Notes comparison
uses the full editor, animation, clipboard history and session restoration.

The original 3,000,000-byte private-dirty target remains a reported target. On
September 9 the product requirement was revised: avoid waste in both memory and
CPU, and reject memory savings that harm responsiveness or correctness. Normal
allocator settings are the default; tuned allocator experiments are
separate diagnostic evidence. Verification does not install the app.

Heaptrack traces identified duplicate paragraph ownership and per-line/glyph
allocations, as well as native XIM and font resources. Changes share immutable
paragraph/source snapshots and store undo text compactly without dropping history.
Heaptrack measures requested heap allocations; the native probes separately record
private dirty/clean pages, RSS, PSS, swap and display-server costs.
The profiler choice and custom allocator hooks follow the upstream
[Heaptrack documentation](https://github.com/KDE/heaptrack) and
[allocator API](https://github.com/KDE/heaptrack/blob/master/src/track/heaptrack_api.h).
The [Massif manual](https://valgrind.org/docs/manual/ms-manual.html) explains why
heap requests alone cannot establish resident memory.

The final full-coverage Notes native gate passes with 3,506,176–3,522,560 private
dirty bytes and 10,153,984–10,186,752 total private resident bytes. App and private
Xvfb swap are zero. Ordinary app CPU is 4.20–4.35 seconds; Xvfb uses a separate
4.56–4.82 seconds. The 3 MB target was not met. Native drawable p95 response is
4.76 ms for idle pointer input, 7.29 ms during fire and 13.60 ms for resizing.
[Consumer measurements and growth limits](../../fire-notes/docs/performance.md)
include the exact binary and acceptance receipt.

The larger editor test exposed redundant whole-document shaping. Callers can now
borrow an existing paragraph through `TextRequest::previous`; unchanged logical
lines are reused only when source and metrics match. No persistent text-service
cache is created. The editor avoids a second scrollbar-width layout when hard
line breaks already require scrolling, and releases superseded wide startup
geometry before shaping narrower lines. Unicode/edit/width tests compare reused
geometry against fresh layout; the complete 64 KiB native workload passes.

The Studio results below are historical 0.4/0.5 measurements. They do not establish
performance of the new Cairo editor. The independent Cairo minimal-consumer
measurements are labeled separately.

Measured on 2026-09-08 with release optimization, Xvfb and Mesa software rendering.
[Raw results](benchmarks/native.json) include the final executable's SHA-256 and the
earlier reference runs, including 0.4.0's under `v0_4_0_reference`. These are local process measurements, not hardware-GPU
power measurements or physical-display latency.

```sh
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py --seconds 2 --accessibility --output artifacts/native
```

The probe uses a private X display, D-Bus session, runtime directory and inspection
socket. It leaves the user's desktop and accessibility preferences unchanged.

## Minimal consumer

### History of the missed memory limit

The released 0.5 native host did not meet Fire Notes' original
3,000,000-byte private-dirty budget.
This is a preexisting renderer constraint, not evidence of a new 0.5.0 renderer
regression. The relevant history is:

| Commit | Change | Memory consequence |
| --- | --- | --- |
| `2431ebd`, 2026-09-06 | First framework prototype stores `Canvas<OpenGl>` and creates a GL context unconditionally in `crates/fire-ui-native/src/window.rs`. | Native applications cannot select a lighter renderer. |
| `0f3087d`, 2026-09-06 | Typed-widget rewrite preserves that native rendering path. | Separating core/widgets from native code does not remove the native renderer's startup cost. |
| `922cfbd`, 2026-09-08, 0.4.0 | Services, font files and retained repainting become explicit choices. | Real dependency reductions, but `glutin` and `femtovg` remain mandatory. |
| `7566422`, 2026-09-08, 0.5.0 | Adds controls, layout policies, theme tokens and brush translations. | Native window, text and presentation implementations, native dependencies and external lockfile versions are unchanged from the 0.4.0 tag. |

The September 6 performance report already recorded roughly 130–140 MiB native
RSS under software GL and left hardware-driver profiling unfinished. The 0.4.0
minimal report below records 106.7 MiB RSS. Neither is proof of low native RAM.
The 2.66 MiB figure is executable size, not process memory.

The missing acceptance check allowed this to remain unresolved: `lean_probe.py`
records RSS/PSS and checks dependency selection, pixels and resize behavior, but
does not fail on a RAM ceiling. `native_probe.py` also records memory without a
budget. CI invokes the studio/input probes and compiles feature combinations;
it does not run the minimal-consumer memory experiment or impose a RAM limit.

A September 8 desktop isolation test on Intel Iris Xe measured 25.6 MiB private
dirty memory for the released no-font/no-service minimal window. A plain GLX
window without Fire UI measured 12.8 MiB. A separate X11/Cairo text-drawing control
measured 1.3 MiB. All had zero swap. These controls identify the current native
path as a blocker; they do not establish that a complete Cairo editor meets the
budget. Source and raw measurements live in the consumer's
[renderer isolation record](../../fire-notes/docs/benchmarks/renderer-isolation.json).

The app now has a failing acceptance command, `python3 tools/memory_probe.py
--output artifacts/memory.json`, run in Fire Notes on the target desktop. It
rejects private dirty memory at or above 3,000,000 bytes or any swap at startup,
after typing, during selected-text fire and after resizing. It writes evidence
before returning failure. This is a sampled local acceptance check, not an
already-integrated framework CI gate or a proof about all documents and frames.
The 0.5 release failed it. The September 9 requirement revision makes CPU and
responsiveness part of the memory tradeoff; native interaction, IME, accessibility
and animation checks remain required.

`tools/lean_probe.py` builds the Cairo `minimal` example as two independent
consumer workspaces, each with its own Cargo target directory. The minimal variant
selects X11; extras also compiles accessibility, inspection, clipboard and dialogs.
Both use `fire-ui-fonts` and `fire-ui-cairo` explicitly, load no fonts and retain no
client framebuffer. The probe rejects GPU dependencies in either graph and rejects
optional service/image, shaping and mmap dependencies in the minimal graph.

Run `python3 tools/lean_probe.py --output artifacts/lean-cairo`. Linux needs Xvfb,
xdotool, dbus-run-session, dbus-update-activation-environment and Python Pillow,
plus the development libraries needed to build Cairo and the X11 host. The probe
creates a private X display and session bus. It verifies pixels at 640×480 and after
resizing to 480×360. `--build-only` skips native checks; `--measure-only` reuses
recorded binaries after checking their hashes.

The JSON records exact-byte private dirty memory using the Fire Notes probe's
`Private_Dirty * 1024` accounting. The executable is fsynced before launch so
unfinished linker writeback does not appear as dirty application allocations.
Total private resident memory, private clean, anonymous pages, RSS, PSS, shared
memory and swap remain separate fields. Per-mapping breakdowns expose file-backed
and anonymous costs. Startup, idle and resized
samples must have zero swap; failure returns nonzero after saving evidence.
Dedicated Xvfb memory is recorded before, during and after the app so server
resource costs remain visible. There is no compositor or GPU renderer in this test.

This draw-only experiment is minimal-consumer evidence, not the Fire Notes memory
acceptance workload. It does not test an editor, fonts, animation or active optional
services, and it does not enforce the app's 3,000,000-byte acceptance threshold.
Its three samples cannot establish a peak between samples. Zero idle CPU ticks
only establishes quiet behavior during the recorded interval.

[Cargo combines enabled features](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).
A shared workspace build with Studio would invalidate this dependency comparison.
The [current raw report](benchmarks/lean.json) records the independent Cairo run.
The earlier OpenGL report remains in Git history.

| Cairo draw-only measurement | Minimal | Extras compiled in |
| --- | ---: | ---: |
| Maximum sampled private dirty bytes | 1,445,888 | 1,708,032 |
| Idle private clean bytes | 1,794,048 | 4,657,152 |
| Idle total private resident bytes | 3,239,936 | 6,365,184 |
| Idle RSS bytes | 8,851,456 | 11,931,648 |
| Idle PSS bytes | 3,548,160 | 6,676,480 |
| Idle shared resident bytes | 5,611,520 | 5,566,464 |
| Swap bytes | 0 | 0 |
| CPU ticks over two seconds | 0 | 0 |

Private dirty matched anonymous bytes in these samples. Dedicated Xvfb private
dirty increased by 1,163,264 bytes for minimal and 1,142,784 bytes for extras;
most of that increase remained resident after the app exited. The JSON includes
those before/during/after values. These server allocations are additional costs.

## Historical Studio CPU and memory

One core is 100%; Mesa uses multiple threads. Samples last about two seconds.
Zero CPU ticks establishes quiet idle behavior during those samples only. The
pre-change reference was the existing release binary; its source commit and hash
were not recorded. Comparisons are indicative, not a controlled benchmark between
pinned source revisions. Other work on the machine can affect timings.

| State | 0.4.0 CPU | 0.5.0 CPU | 0.5.0 RSS |
| --- | ---: | ---: | ---: |
| idle | 0.0% | 0.0% | 158.9 MiB |
| controls page | — | 0.0% | 159.3 MiB |
| focused caret | 4.0% | 5.7% | 161.6 MiB |
| animation | 118.0% | 181.7% | 159.0 MiB |
| paused again | 0.0% | 0.0% | 159.0 MiB |
| light palette | — | 0.0% | 159.0 MiB |

Idle, paused and freshly-navigated states all record zero CPU ticks, including
immediately after a palette swap. Seven pages are mounted at once — navigation hides
rather than removes them — and idle RSS is unchanged against 0.4.0's 159.7 MiB. A run competing with a
concurrent build measured 282.8% for the same animation, so these figures come from
one quiet run rather than an average.

Animation CPU rose from 118.0% to 181.7%, and that is a larger canvas rather than a
slower one: the studio's canvas now damages 280,600 pixels per frame against 93,279
in 0.4.0. Three times the area for 1.54 times the CPU means per-pixel cost fell; the
absolute cost of this demo rose because the demo grew. Studio explicitly loads
installed CJK/emoji font fallbacks for its multilingual exercises. The 0.4.0 build
recorded a 6.53 MiB executable; this build records 6.75 MiB, holding seven pages and
every shipped control. Both hashes are recorded in
the raw results. Framework defaults load no fonts; applications supply all font paths.
Shared driver pages and other machine activity can affect RSS comparisons. The retained color image
and stencil buffer cost about five bytes per window pixel, plus driver overhead;
font and text caches also affect RSS. The inspection server adds one sleeping
thread and recorded no idle CPU ticks in the separate Fire Notes probe.

With `partial_repaint: true`, the host repaints damaged regions into a retained image. OpenGL 3+ presents it with
a framebuffer blit; older contexts use a texture draw and have not been benchmarked
here. Geometry changes still repaint the full window. The studio animation damages
about 281,000 pixels of its roughly 1,000,000-pixel window. Fire Notes' decoration
bounds allow small fire frames to invalidate fewer than 1,000 pixels. These are
paint regions, not a claim that presentation copies only that region.

## Historical Studio responsiveness

| Measurement | Pre-change median | Current median | Current p95 |
| --- | ---: | ---: | ---: |
| Resize event received to completed swap | 18.05 ms | 7.89 ms | 11.53 ms |
| Injected pointer command to observed paddle pixels | 37.44 ms | 13.57 ms | 18.40 ms |

Resize uses 63 rendered samples in the current run. Pointer measurements use 20
moves and include xdotool and screenshot overhead. Swap completion is not compositor
presentation. The paddle follows the latest pointer position directly.

## Historical verification and limits

The native probe passed section navigation, every button style, refusal of a disabled
control, checkbox and switch state, slider movement by pointer, arrow keys and
assistive `set_value` (with a non-numeric value rejected), a dropdown whose list opens
below its field and outside the scrolling panel that holds it, typing and caret
movement, list search and selection, animation/pause, pointer response, a live layout
rule change, a palette swap, scrolling and wide/narrow resizing.
It reads Unicode through AT-SPI, moves the native caret, exercises all six EditableText
methods and verifies undo and invalid-range rejection. A clipboard owner that ignores
requests leaves the UI responsive and cannot cause a late paste. The UI answered
an inspector edit in 4.6 ms while the native paste was waiting. A button also
responds to an AT-SPI activation action, on the page that owns it.

The separate `linux_input_probe.py` runs installed IBus 1.5.29-rc2 with Cangjie5 through
XIM and Orca 46.1 on a private desktop. It verifies composition, candidate commit,
cancellation, selected-text replacement and focus transfer. External edits through
the socket and AT-SPI cancel stale composition. Screenshots verify the CJK glyph and
candidate panel below the text. Orca generates expected content, caret, editing,
selection and button utterances, with keyboard echo disabled. Tab and Enter work
while Orca runs. Speech synthesis/audio output is excluded.

The framework has 66 unit/integration tests and two documentation tests. Added tests
cover transformed/coalesced paint damage, unrelated-sibling paint skipping, mixed
Hebrew/Arabic/Latin geometry, bidi controls, AccessKit selection conversion, IME
composition state, semantic edits/undo and private socket cleanup. The actual crate
packages are built separately by `cargo package`.

Fire Notes checks, screenshots and temporary-data probes live in its own repository.
Its agent probe checks directed selection, Unicode replacement, native keyboard undo,
invalid/stale requests and orderly socket cleanup. The screenshot includes Hebrew,
Arabic, combining characters, color emoji and animated selected text.

Windows/macOS native interaction, hardware-GPU performance and physical-display
latency remain unverified. Linux evidence covers the installed IBus/XIM and Orca
versions above, not every engine or assistive device. Variable-height lists and
large-document incremental layout remain future work. Wayland is deferred.
