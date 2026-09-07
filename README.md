# Fire UI

A Rust UI framework built around persistent widgets and typed, owned children.
Low-level widget, input and drawing interfaces support custom controls, animated
text and games. Reusable composition layers handle common layouts and editing.

## Packages

| Package | Owns |
| --- | --- |
| `fire-ui` | Widget ownership, typed delivery, layout, input, scheduling, drawing and text contracts. No dependencies. |
| `fire-ui-widgets` | Buttons, labels, layouts, themes, document editing, scrolling, menus and virtual lists. |
| `fire-ui-native` | Native windows, OpenGL drawing, text shaping, clipboard, IME and accessibility. |
| `fire-ui-studio` | Public-API examples: counters, editor, searchable 100,000-item list and fire/paddle canvas. |

```sh
cargo run --release -p fire-ui-studio
```

The studio's **Play / pause** button starts animation. The paddle follows the
pointer directly. Narrow windows use a scrolling column. `FIRE_UI_FONT` selects
a TrueType font when the platform default is unavailable.

## Consumers

[Fire Notes](../fire-notes/README.md) is a separate application repository. Keep it
beside this checkout to use its local path dependencies. Fire UI builds and tests
without Fire Notes. App storage, note actions and visual design belong to the app;
framework fixes and reusable controls belong here.

Other apps can depend on the crates directly:

```toml
[dependencies]
fire-ui = { path = "../fire-ui/crates/fire-ui" }
fire-ui-widgets = { path = "../fire-ui/crates/fire-ui-widgets" }
fire-ui-native = { path = "../fire-ui/crates/fire-ui-native" }
```

## Development

```sh
cargo fmt --all --check
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo build --locked --release -p fire-ui-studio
python3 tools/native_probe.py --seconds 2 --accessibility --output artifacts/native
```

The native probe requires Linux, Xvfb, xdotool and Pillow with XCB support.
Accessibility checks use a private D-Bus session and the system AT-SPI tools.
The probe checks real input and accessibility actions, captures screenshots, and
measures idle CPU, memory, resizing and visible paddle response.

See [architecture](docs/architecture.md), [measurements](docs/performance.md) and
[project rules](AGENTS.md). This is prototype development with no backward
compatibility requirement. Superseded implementations remain only in Git history.
