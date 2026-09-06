# Native measurements

Measured on 2026-09-06 with the replacement framework, release optimization,
Xvfb and Mesa software rendering. These are prototype measurements, not hardware
GPU or physical display latency results. [Raw results](benchmarks/native.json)
include the measured binary's SHA-256. Superseded measurements remain in Git history.

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
| idle | 0.0% | 135.5 MiB |
| focused caret | 7.0% | 130.4 MiB |
| animation | 212.5% | 134.2 MiB |
| paused again | 0.0% | 134.2 MiB |

The stripped release executable is 7.44 MiB, including the
native accessibility bridge. `fire-ui` itself has no dependencies. Software-rendered
animation remains expensive; partial repaint and hardware-GPU measurements are next
performance work. The current host streams a full repaint without a backing texture.
Clipped widget branches and off-screen paragraph lines are skipped.

## Responsiveness

| Measurement | Median | 95th percentile | Maximum |
| --- | ---: | ---: | ---: |
| Resize event received to completed swap, 63 frames | 9.74 ms | 15.39 ms | 21.66 ms |
| Injected mouse command to observed paddle pixels, 20 moves | 22.81 ms | 49.39 ms | 49.39 ms |

Mouse measurements include xdotool execution and screenshot capture. Swap completion
is not compositor presentation. The paddle assigns the latest pointer position
directly; the simulation does not interpolate or smooth pointer movement.

## Verification

The probe verified both counter instances, text insertion and caret movement,
search/filter/select in the virtual list, animation/pause, wide/narrow resize,
scrolling and visible paddle position. It discovered the native AT-SPI button,
invoked its action and checked that the counter increment reached the application.
Normal, editing, animated and narrow-window screenshots were visually inspected.

The workspace has 32 passing tests. These cover lifecycle/removal, input sessions,
modal and anchor geometry, bounded messages/work, stale task results, native text
measurement, editing/undo, appearance invalidation and virtual-list bounds.
A 100,000-item list uses at most 14 mounted rows in the 300-pixel test viewport.
Height-only editor resize reuses its paragraph, and clean pointer motion triggers
no layout or geometry publication.

Full bidi editing, real IME combinations, complete screen-reader text navigation,
large-document partial repaint and target-device resource use remain unverified or
unfinished. They are not established by the native smoke test.
