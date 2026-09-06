# Fire UI

A custom Rust UI framework. The framework and its widgets are the product. Notes, animated text, and a small game are exercises built with it.

This is design and prototype work with **no backward compatibility guarantees**. See
[project rules](AGENTS.md) and the [architecture](docs/architecture.md).

The revised architecture is in [DESIGN.md](docs/architecture.md). The [E2 experiment](skeleton/elegant/README.md)
exercises typed composition, work demand, layout, and appearance with
`cargo test --manifest-path skeleton/elegant/Cargo.toml`. Earlier lifecycle/input experiments
run with `cargo test --manifest-path skeleton/Cargo.toml`. Both are separate from the running
prototype described below; see [experiment results and limits](skeleton/README.md).

The core owns a persistent widget tree, input routing, focus, pointer capture, layout traversal, and frame deadlines. Optional crates add composition, controls, and a native OpenGL host. Application code can implement the same widget trait as the built-in controls.

```sh
cargo run --release -p fire-ui-studio
```

The studio includes independent text fields, a multiline notes editor, a keyboard-controlled list, a modal, animated fire text, and a paddle game. Notes in this exercise are temporary. Click the fire or game to start or pause it.

On Linux, install the native build dependencies first:

```sh
sudo apt install build-essential pkg-config libxkbcommon-dev libwayland-dev libegl1-mesa-dev libgl1-mesa-dev fonts-dejavu-core
```

Set `FIRE_UI_FONT` to a TrueType font path to choose the default font. Native applications can register additional fonts through `NativeText::add_font`.

## Structure

| Package | Owns | Dependencies |
| --- | --- | --- |
| `fire-ui` | Geometry, constraints, open widget and painter traits, persistent nodes, events, focus, capture, modal scope, scheduling | None |
| `fire-ui-widgets` | Row, column, panel, stack, scroll view, label, button, list, modal, single-line and multiline text editing | Core and Unicode segmentation |
| `fire-ui-native` | Window, OS events, clipboard, text measurement, GPU drawing, wake handles | Core plus native bindings |
| `fire-ui-studio` | The notes exercise, custom fire widget, custom game, application composition | The three crates above |

The old application is preserved in `legacy/fire-notes`, outside the workspace. Its runtime and widgets are not dependencies of the new framework.

## Control and composition

Implement `Widget` to provide layout, event handling, and painting. A `Node` keeps that widget instance alive between events. Containers compose ordinary child nodes. Application messages carry typed values and their source widget IDs.

Painting supports rectangles, strokes, paths, curves, text runs, gradients, clipping, and affine transforms. A widget can use the optional `CustomPaint` hook for backend-specific drawing. With the OpenGL host, that hook receives the femtovg canvas. A different host can expose its own backend object or decline the hook.

The core can be embedded in a different event loop, renderer, or game engine. The native host is optional. See [the architecture and extension guide](docs/architecture.md).

## Scheduling and resource use

- An unchanged window has no framework timer and requests no frames.
- A focused text caret schedules its next blink. Losing focus cancels it.
- Animations request their next frame explicitly. Pausing stops those requests.
- Background tasks can use `WakeHandle::post` to wake the native loop directly.
- Layout is invalidated explicitly; painting does not perform hidden layout.
- Paint calls stream to the renderer. The framework does not clone an owned drawing tree every frame.
- Resize events update the pending size; GPU buffers resize at the next rendered frame. Multiple size changes before layout use only the latest constraints.
- Consecutive mouse moves are combined before drawing, so a frame uses the latest received position. Applications can disable this through `WindowOptions::coalesce_pointer_moves`.
- Text undo history is bounded by both operations and bytes.

`WindowOptions::retain_surface` enables an optional backing framebuffer and partial repaint. It adds window-sized GPU storage, so it is disabled by default. Compare it on your hardware with `cargo run --release -p fire-ui-studio -- --retained` and `python3 tools/native_probe.py --retained`.

These are architectural properties. Actual CPU and memory depend on the renderer, fonts, drivers, and application. Run the native probe to measure your machine:

```sh
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py
cargo run --release -p fire-ui-studio --example resize_probe
```

The probe requires Linux, Xvfb, xdotool, and Pillow with XCB support. It creates its own display and terminates its processes afterward. It records process CPU, RSS, screenshots, and sample durations in `artifacts/`.

The resize probe measures CPU layout for a 22,500-byte document using the native font service, without a window or GPU. It separates widths where cached lines still fit from narrower widths that force rewrapping. The native probe records resize-event-to-swap time and observes the paddle following injected mouse moves. The paddle measurement includes command and screenshot overhead. Use `--output artifacts/my-run` to preserve separate runs. See the [measured baseline and remaining performance work](docs/performance.md).

A small application example is available with `cargo run --release -p fire-ui-studio --example counter`.

## Verification

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all -- --check
```

Tests cover focus routing, retained caret state, Unicode grapheme deletion, replacement undo, large text integrity, wrapped-line hit testing, modal focus, list navigation, capture, and timer cancellation. The native probe exercises the actual window separately.

## Current limits

This is the first working framework implementation, not a mature general-purpose toolkit. The host currently uses one window and repaints the window when dirty. Text layout is cached between changes but rebuilt for the edited document after a change. Complete bidirectional caret affinity, advanced IME interactions, accessibility platform integration, touch gestures, and virtualized document layout need further work.

The painter's affine transform changes drawing. A custom widget that applies an additional internal transform must also transform its own interaction coordinates. Containers already handle parent offsets and clipping.

The notes exercise has no persistence or session migration. The old note files and formats have not been changed.
