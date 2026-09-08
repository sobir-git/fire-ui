# Native measurements

Measured on 2026-09-08 with release optimization, Xvfb and Mesa software rendering.
[Raw results](benchmarks/native.json) include the final executable's SHA-256 and the
pre-change reference run. These are local process measurements, not hardware-GPU
power measurements or physical-display latency.

```sh
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py --seconds 2 --accessibility --output artifacts/native
```

The probe uses a private X display, D-Bus session, runtime directory and inspection
socket. It leaves the user's desktop and accessibility preferences unchanged.

## Minimal consumer

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

| State | Pre-change CPU | Current CPU | Current RSS |
| --- | ---: | ---: | ---: |
| idle | 0.0% | 0.0% | 159.7 MiB |
| focused caret | 7.5% | 4.0% | 160.0 MiB |
| animation | 196.0% | 118.0% | 157.2 MiB |
| paused again | 0.0% | 0.0% | 157.2 MiB |

Animation CPU was 196.0% in the old reference and 118.0% in this run. This comparison
includes the earlier partial-repaint changes and is not an isolated accessibility
benchmark. Studio explicitly loads installed CJK/emoji font fallbacks for its multilingual
exercises. The previous Linux build recorded 168.4 MiB idle RSS and an 8.14 MiB
executable; this build records 159.7 MiB and 6.53 MiB. Both hashes are recorded in
the raw results. Framework defaults load no fonts; applications supply all font paths.
Shared driver pages and other machine activity can affect RSS comparisons. The retained color image
and stencil buffer cost about five bytes per window pixel, plus driver overhead;
font and text caches also affect RSS. The inspection server adds one sleeping
thread and recorded no idle CPU ticks in the separate Fire Notes probe.

With `partial_repaint: true`, the host repaints damaged regions into a retained image. OpenGL 3+ presents it with
a framebuffer blit; older contexts use a texture draw and have not been benchmarked
here. Geometry changes still repaint the full window. The studio animation damages
about 93,000 pixels of its roughly 1,000,000-pixel window. Fire Notes' decoration
bounds allow small fire frames to invalidate fewer than 1,000 pixels. These are
paint regions, not a claim that presentation copies only that region.

## Responsiveness

| Measurement | Pre-change median | Current median | Current p95 |
| --- | ---: | ---: | ---: |
| Resize event received to completed swap | 18.05 ms | 7.30 ms | 10.35 ms |
| Injected pointer command to observed paddle pixels | 37.44 ms | 9.68 ms | 19.65 ms |

Resize uses 63 rendered samples in the current run. Pointer measurements use 20
moves and include xdotool and screenshot overhead. Swap completion is not compositor
presentation. The paddle follows the latest pointer position directly.

## Verification and limits

The native probe passed independent counters, typing/caret movement, list search and
selection, animation/pause, pointer response, scrolling and wide/narrow resizing.
It reads Unicode through AT-SPI, moves the native caret, exercises all six EditableText
methods and verifies undo and invalid-range rejection. A clipboard owner that ignores
requests leaves the UI responsive and cannot cause a late paste. The UI answered
an inspector edit in 4.6 ms while the native paste was waiting. The counter also
responds to an AT-SPI activation action.

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
