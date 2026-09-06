# Native measurements

The studio regression run was measured on 2026-09-07 with release optimization,
Xvfb and Mesa software rendering. These are prototype measurements, not hardware
GPU or physical display latency results. [Raw results](benchmarks/native.json)
include the measured binary's SHA-256. Superseded measurements remain in Git history.

## Fire Notes

The restored original interface was measured on 2026-09-07 using Xvfb and Mesa
software rendering. [Raw app results](benchmarks/notes.json) identify the measured
release binary. The workload includes four tabs, a 100-line document, fire animation,
clipboard edits, native Open/Save As dialogs, and a restart.

After the fire faded, a focused editor used **zero CPU ticks over two seconds**.
Process RSS was 151.0 MiB, including software GL and native libraries.
The release executable was 7.79 MiB. These short samples establish
quiet idle behavior for this run, not hardware GPU power use or an RSS ceiling.

Across 64 rendered resize samples, app-received event to completed swap was
4.48 ms median, 6.48 ms at the 95th percentile, and 6.97 ms maximum.
This excludes compositor presentation latency. The earlier app workload was different,
so these figures are not a controlled speedup or memory regression comparison.

The native check passed initial typing, animated fire, clipboard, undo/redo, inline
rename, tab dragging, search, reopening, command execution, native Open and Save As,
raw Markdown preservation, per-tab wrap, scrollbar drag, 420-pixel layout, saving
immediately before close, and restoration of cursor, scroll, titles, tabs and placement.
Xvfb has no window manager; the probe supplies the ICCCM move notification for that
placement check. Eleven final screenshots were visually inspected against the original
app reference. The workspace has **50 passing tests** and Clippy passes with warnings
as errors. File-drop delivery while blurred/modal, tabs in native text, selection
movement, scroll restoration, and bounded fire scheduling have regression tests.

```sh
cargo build --release -p fire-notes
python3 tools/notes_probe.py --output artifacts/notes
```

## Studio regression run

```sh
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py --seconds 2 --accessibility --output artifacts/native
```

The accessibility option creates a private D-Bus session and temporary runtime
storage. Its settings backend is in-memory. The probe leaves the user's display,
notes and accessibility preferences out of the test.

## CPU and memory

CPU percentages count one core as 100%; Mesa uses several worker threads. Each
sample below lasted about two seconds. Zero CPU ticks over a short sample establishes
quiet idle behavior during that sample, not a universal power-consumption guarantee.
RSS includes native libraries, software GL, font caches and the 100,000-item model.

| State | CPU | Process RSS |
| --- | ---: | ---: |
| idle | 0.0% | 140.4 MiB |
| focused caret | 4.5% | 131.9 MiB |
| animation | 139.5% | 136.3 MiB |
| paused again | 0.0% | 136.3 MiB |

The stripped release executable is 7.48 MiB, including the
native accessibility bridge. `fire-ui` itself has no dependencies. Software-rendered
animation remains expensive; partial repaint and hardware-GPU measurements are next
performance work. The current host streams a full repaint without a backing texture.
Clipped widget branches and off-screen paragraph lines are skipped.

## Responsiveness

| Measurement | Median | 95th percentile | Maximum |
| --- | ---: | ---: | ---: |
| Resize event received to completed swap, 63 frames | 6.36 ms | 7.52 ms | 8.84 ms |
| Injected mouse command to observed paddle pixels, 20 moves | 12.84 ms | 15.32 ms | 15.32 ms |

Mouse measurements include xdotool execution and screenshot capture. Swap completion
is not compositor presentation. The paddle assigns the latest pointer position
directly; the simulation does not interpolate or smooth pointer movement.

## Verification

The probe verified both counter instances, text insertion and caret movement,
search/filter/select in the virtual list, animation/pause, wide/narrow resize,
scrolling and visible paddle position. It discovered the native AT-SPI button,
invoked its action and checked that the counter increment reached the application.
Normal, editing, animated and narrow-window screenshots were visually inspected.

The workspace has 50 passing tests. These cover lifecycle/removal, input sessions,
modal and anchor geometry, bounded messages/work, stale task results, native text
measurement, editing/undo, appearance invalidation and virtual-list bounds.
A 100,000-item list uses at most 14 mounted rows in the 300-pixel test viewport.
Height-only editor resize reuses its paragraph, and clean pointer motion triggers
no layout or geometry publication.

Full bidi editing, real IME combinations, complete screen-reader text navigation,
large-document partial repaint and target-device resource use remain unverified or
unfinished. They are not established by the native smoke test.
