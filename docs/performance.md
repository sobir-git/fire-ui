# Native measurements

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

The native host does not meet Fire Notes' 3,000,000-byte private-memory budget.
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
The current release fails it. A replacement native rendering path must pass this
budget together with native interaction, IME, accessibility and animation checks
before it can be accepted for this consumer.

`tools/lean_probe.py` builds the same draw-only 640×480 window in two independent
consumer workspaces. The minimal variant selects only `x11`; the other enables
accessibility, inspection, clipboard, dialogs and bitmap fonts. Neither loads fonts
or retains a framebuffer. [Raw results](benchmarks/lean.json) record compiler,
features, dependency/lock hashes, binary hashes, memory and build conditions.

| Measurement | Minimal | Extras compiled in |
| --- | ---: | ---: |
| Stripped executable | 2.66 MiB | 6.34 MiB |
| Normal/build dependency packages | 64 | 170 |
| Idle RSS | 106.7 MiB | 112.5 MiB |
| Idle proportional memory (PSS) | 76.1 MiB | 80.9 MiB |
| CPU ticks over two seconds | 0 | 0 |

The minimal executable is 58.1% smaller. These process memory figures include Mesa's
software renderer, which creates its own worker threads; they are not core-only
allocation measurements. The core crate has no dependencies. The native renderer
still compiles its text-shaping dependencies even when no font files are loaded.
Services unused by this workload can incur further costs when activated.
Both variants render the same pixels and resize to 480×360 without stale drawing.
Recorded build times include concurrent compilation and differing cache states;
they do not establish a build-speed improvement.

[Cargo combines enabled features](https://doc.rust-lang.org/cargo/reference/features.html#feature-unification).
A shared workspace build with Studio would therefore invalidate this dependency
comparison. CI checks the native features independently as well as the complete apps.

## CPU and memory

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

## Responsiveness

| Measurement | Pre-change median | Current median | Current p95 |
| --- | ---: | ---: | ---: |
| Resize event received to completed swap | 18.05 ms | 7.89 ms | 11.53 ms |
| Injected pointer command to observed paddle pixels | 37.44 ms | 13.57 ms | 18.40 ms |

Resize uses 63 rendered samples in the current run. Pointer measurements use 20
moves and include xdotool and screenshot overhead. Swap completion is not compositor
presentation. The paddle follows the latest pointer position directly.

## Verification and limits

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
