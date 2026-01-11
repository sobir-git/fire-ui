# Fire Notes 🔥

A blazing-fast, native markdown editor for Ubuntu.

## Features

- **Tabs** - Multiple files with `Ctrl+N` (new), `Ctrl+W` (close), `Ctrl+Tab` (switch)
- **File Operations** - `Ctrl+O` (open), `Ctrl+S` (save)
- **GPU-Accelerated** - OpenGL rendering with femtovg
- **Efficient** - Rope data structure for O(log n) edits

## Performance

| Metric | Target | Achieved |
|--------|--------|----------|
| Binary Size | <2MB | 4.0MB |
| Tests | Pass | ✅ 17/17 |

## Build

```bash
# Install dependencies
sudo apt install build-essential pkg-config libfontconfig-dev libxkbcommon-dev libwayland-dev

# Build release
cargo build --release

# Run
./target/release/fire-notes
```

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | New tab |
| `Ctrl+W` | Close tab |
| `Ctrl+Tab` | Next tab |
| `Ctrl+O` | Open file |
| `Ctrl+S` | Save file |
| `Escape` | Quit |
| Arrow keys | Navigate |
| `Backspace` / `Delete` | Delete text |

## Tech Stack

- **Rust** - Memory-safe systems programming
- **winit** - Cross-platform windowing
- **femtovg** - GPU-accelerated 2D rendering
- **cosmic-text** - Fast text layout
- **ropey** - O(log n) text buffer

## Automated Visual Testing

Run visual regression tests:

```bash
# Create/update baseline snapshots
./tests/visual/run_visual_tests.sh --update-snapshots

# Run tests and compare against snapshots
./tests/visual/run_visual_tests.sh
```

Tests use:
- **xdotool** - Simulates keyboard/mouse input
- **scrot** - Captures screenshots
- **ImageMagick** - Compares images (SSIM)

> **Development Guideline**: When facing choices or unexpected issues, always perform a web search to find the latest information and best practices.
