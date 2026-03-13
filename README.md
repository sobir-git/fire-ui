# fire-notes

Native GPU-accelerated text editor for Linux. Single binary, no Electron.

## Architecture

```
App  (src/app/)
  Routes OS events → AppLogic methods
  Calls renderer.render(frame)

AppLogic  (src/logic/)
  Owns all state: tabs, focus, component instances
  render_frame() → delegates to component.snapshot() → RenderFrame
  Zero GPU imports. Fully headless-testable.

Components  (src/components/)
  One file per component. Owns state + geometry + snapshot baking.
  NotesPicker — search input + result list + overlay geometry
  TabBar      — tab strip + window chrome snapshot
  TextEditor  — content area + scrollbar snapshot

Primitives  (src/primitives/)
  Button, Label, TextInput, Scrollbar, List<T>
  One file each. Same state+snapshot contract as components.

Layout  (src/layout/)
  Column, Row — weight-based rect splitting + hit dispatch. No GPU.

Renderer  (src/renderer/)
  Reads RenderFrame (pure data). Calls femtovg. Never computes layout.

ui/  (src/ui/)
  Rect, WindowRect — all coordinate arithmetic lives here.
  UiTree, TabBar, ContentArea — retained geometry widgets.
```

**Key invariants:**
- `AppLogic` has zero platform/GPU dependencies — testable with no window, no GPU.
- `ui/types.rs` has zero widget imports — `Rect` is the only coordinate authority.
- Renderers never compute layout coordinates — they only read pre-baked `RenderFrame` data.
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

Four layers, ordered by speed:

| Layer | What | Command |
|-------|------|---------|
| L1 | Logic unit tests (pure `AppLogic`) | `cargo test` |
| L2 | `RenderFrame` structural snapshots | `cargo test` |
| L3 | Headless pixel tests (EGL surfaceless + FBO) | `cargo test renderer::tests` |
| L4 | Xvfb smoke tests | `./tests/visual/run_visual_tests.sh` |

L1–L3 require no display server. L3 uses `EGL_PLATFORM_SURFACELESS_MESA`; tests skip gracefully if Mesa is absent.

```bash
# Update L4 baselines
./tests/visual/run_visual_tests.sh --update-snapshots
```

## Shortcuts

`Ctrl+N` new tab · `Ctrl+W` close · `Ctrl+Tab` switch · `Ctrl+O` open · `Ctrl+P` notes picker · `Ctrl+S` save · `Ctrl+Z/Y` undo/redo · `Escape` quit
