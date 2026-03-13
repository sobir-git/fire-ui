# fire-notes

Native GPU-accelerated text editor for Linux. Single binary, no Electron.
This codebase is simultaneously the application **and** the framework being built.
The app is a proving ground — perfecting the framework is the primary goal.

> **Rule**: We do not make pragmatic fixes, minimal patches, or transitional compromises.
> We identify the ideal architecture and implement it radically. The app serves the framework.

## Architecture

```
Platform  (src/platform/)
  Translates OS events (winit) → typed app events
  Owns: GL context, window, event loop

App  (src/app/)
  Routes typed events → AppLogic methods
  Owns: clipboard, file I/O, persistence

AppLogic  (src/logic/)
  Owns all state: tabs, focus, active overlay, inline widget
  render() → Node tree (pure data, no GPU)
  Zero GPU/platform imports. Fully headless-testable.

Components  (src/components/)
  One file per component.
  NotesPicker — search input + filterable list (ActiveOverlay::NotesPicker)
  SlashMenu   — command palette anchored to cursor (ActiveOverlay::SlashMenu)
  TabRename   — inline tab-title editor (ActiveInline::TabRename)
  Event interface: on_event(OverlayEvent) → OverlayResult / InlineResult

Primitives  (src/primitives/)
  TextInput, List<T>, Scrollbar, Button, Label
  Self-contained: own state + render + pointer/keyboard event methods
  No external layout dependencies.

Layout  (src/layout/)
  Column, Row — weight-based rect splitting
  overlay_panel(), floating_rect() — overlay geometry helpers

Renderer  (src/renderer/)
  Single NodeRenderer walks the Node tree → femtovg draw calls
  The ONLY file that touches the GPU API.

Runtime  (src/runtime/)
  Lightweight framework for simple single-window apps (widget_demo)
  App trait + ViewCtx + FocusManager
```

**Absolute invariants** (must never break):
- `src/logic/` never imports `femtovg` or any platform dep — enforced by headless tests
- `AppLogic::render()` is the only path that produces draw data
- `NodeRenderer` is the only caller of femtovg draw primitives
- Components never hardcode colors — all visual values flow from `Theme`

See `UI_FRAMEWORK.md` for the design philosophy, the current gaps, and the target vision.

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
./dev.sh                        # build (debug) + kill old instance + relaunch
cargo test                      # all tests
cargo run --bin widget-demo     # interactive primitive testbed
./dev.sh --watch                # auto-rebuild on src/ change (requires cargo-watch)
./install.sh                    # install release binary + desktop entry
```

## Testing

| Layer | What | Command |
|-------|------|---------|
| L1 | Logic unit tests + TextInput QA | `cargo test` |
| L2 | Headless pixel tests (EGL surfaceless) | `cargo test renderer::tests` |
| L3 | Xvfb smoke tests | `./tests/visual/run_visual_tests.sh` |

## Shortcuts

`Ctrl+N` new tab · `Ctrl+W` close · `Ctrl+Tab` switch · `Ctrl+O` open · `Ctrl+P` notes picker · `Ctrl+/` command palette · `Ctrl+S` save · `Ctrl+Z/Y` undo/redo · `Alt+Z` word wrap · `Escape` cancel/quit
