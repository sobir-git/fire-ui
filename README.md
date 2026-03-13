# fire-notes

Native GPU-accelerated text editor for Linux. Single binary, no Electron.

## Architecture

```
App  (src/app/)
  Routes OS events → AppLogic methods
  Calls renderer.render(&node, width, height)

AppLogic  (src/logic/)
  Owns all state: tabs, focus, overlay, inline widget
  render() → Node tree (pure data, no GPU)
  Zero GPU imports. Fully headless-testable.

Components  (src/components/)
  One file per component. Implements Overlay or InlineWidget.
  NotesPicker — search input + result list (Overlay)
  SlashMenu   — command palette anchored to cursor (Overlay)
  TabRename   — inline tab-title editor (InlineWidget)

Primitives  (src/primitives/)
  Button, Label, TextInput, Scrollbar, List<T>
  One file each. Render via render_at(rect, scale) → Node.

Layout  (src/layout/)
  Column, Row — weight-based rect splitting.
  floating_rect() — anchored overlay positioning.
  Overlay / InlineWidget / Component traits.

Renderer  (src/renderer/node_renderer.rs)
  Single NodeRenderer walks the Node tree and calls femtovg.
  Never computes layout. Zero coupling to AppLogic.

ui/  (src/ui/)
  Rect, WindowRect — all coordinate arithmetic lives here.
  UiTree — retained geometry for hit-testing and hover state.
```

**Key invariants:**
- `AppLogic` has zero platform/GPU dependencies — testable with no window, no GPU.
- `AppLogic::render()` is the only path that produces draw data — no intermediate snapshots.
- `NodeRenderer` is the only file that calls femtovg — no per-subsystem renderers.
- See `UI_FRAMEWORK.md` for the full design guide.

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

## Dev Workflow

```bash
# Build (debug) + kill old instance + relaunch
./dev.sh

# Run tests
cargo test

# Auto-rebuild + relaunch on every src/ change (requires cargo-watch)
cargo install cargo-watch
./dev.sh --watch

# Install release binary to ~/.local/bin + desktop entry
./install.sh
```

## Testing

Three layers, ordered by speed:

| Layer | What | Command |
|-------|------|---------|
| L1 | Logic unit tests (pure `AppLogic` state) | `cargo test` |
| L2 | Headless pixel tests (EGL surfaceless + FBO) | `cargo test renderer::tests` |
| L3 | Xvfb smoke tests | `./tests/visual/run_visual_tests.sh` |

L1–L2 require no display server. L2 uses `EGL_PLATFORM_SURFACELESS_MESA`; tests skip gracefully if Mesa is absent.

```bash
# Update L3 baselines
./tests/visual/run_visual_tests.sh --update-snapshots
```

## Shortcuts

`Ctrl+N` new tab · `Ctrl+W` close · `Ctrl+Tab` switch · `Ctrl+O` open · `Ctrl+P` notes picker · `Ctrl+/` command palette · `Ctrl+S` save · `Ctrl+Z/Y` undo/redo · `Alt+Z` word wrap · `Escape` quit
