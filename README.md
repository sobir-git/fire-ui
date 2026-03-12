# fire-notes

Native GPU-accelerated text editor for Linux. Single binary, no Electron.

## Architecture

```
App  (owns renderer + clipboard, handles platform events)
 └── AppLogic  (all document and UI state — pure Rust, no platform deps)
      ├── tabs: Vec<Tab>         rope-backed text buffers
      ├── scroll_state           per-tab scroll position
      ├── ui_state               hover, drag, cursor blink
      └── render_frame()  →  RenderFrame  (pure data snapshot)

Renderer  (femtovg/OpenGL, consumes Tab + UiState directly)
 ├── tab_bar/     tab strip + window chrome
 ├── text_content/  editor area, selection highlight, cursor
 └── scrollbar/

Platform
 ├── winit event loop  →  App::handle_event()
 └── glutin/EGL        →  OpenGL context on Wayland or X11
```

**Key design invariant:** `AppLogic` has zero platform dependencies. It can be constructed and driven entirely from test code — no window, no GPU.

## Stack

| Layer | Crate |
|-------|-------|
| Text buffer | `ropey` — O(log n) insert/delete |
| Text layout | `cosmic-text` — shaping + glyph cache |
| 2D rendering | `femtovg` — retained path renderer over OpenGL |
| Windowing | `winit` + `glutin` |

## Build

```bash
sudo apt install build-essential pkg-config libfontconfig-dev libxkbcommon-dev libwayland-dev
cargo build --release
./target/release/fire-notes
```

## Testing

Four layers, ordered by speed:

| Layer | What | Command |
|-------|------|---------|
| L1 | Logic unit tests (pure `AppLogic`) | `cargo test` |
| L2 | `RenderFrame` structural snapshots | `cargo test` |
| L3 | Headless pixel tests (EGL surfaceless + FBO) | `cargo test renderer::tests` |
| L4 | Xvfb smoke tests (5 xdotool scenarios) | `./tests/visual/run_visual_tests.sh` |

L1–L3 require no display server. L3 uses `EGL_PLATFORM_SURFACELESS_MESA`; tests skip gracefully if Mesa is absent.

```bash
# Update L4 baselines
./tests/visual/run_visual_tests.sh --update-snapshots
```

## Shortcuts

`Ctrl+N` new tab · `Ctrl+W` close · `Ctrl+Tab` switch · `Ctrl+O` open · `Ctrl+S` save · `Ctrl+Z/Y` undo/redo · `Escape` quit
