# Fire UI

A Rust UI toolkit built around persistent widgets and typed, owned children.
The framework and reusable widgets are the product. Fire Notes will be a consumer.

```sh
cargo run --release -p fire-ui-studio
```

The studio demonstrates independent counters, editable text, a searchable 100,000-item
virtual list, and a custom fire/paddle canvas. Animations start with **Play / pause**;
the paddle follows the pointer directly. Narrow windows use a scrolling column.

## Packages

- `fire-ui`: platform-independent widget protocol, ownership, typed delivery, layout,
  shared geometry, input, scheduling, drawing and text contracts. No dependencies.
- `fire-ui-widgets`: arbitrary-content buttons, labels, layout helpers, appearance
  scopes, document editing, scrolling and fixed-height virtual lists.
- `fire-ui-native`: winit windows, OpenGL drawing, font shaping, clipboard, IME and native accessibility.
- `examples/studio`: ordinary consumers of the public framework API.

Previous frameworks and executable design skeletons have been removed. Git history
is their archive. There are no compatibility shims or parallel implementations.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py --accessibility --output artifacts/native
```

The native probe requires Linux, Xvfb, xdotool and Pillow with XCB support. It runs
on an isolated display and, with `--accessibility`, a private accessibility bus.
It exercises real input, verifies an accessibility-triggered counter increment, captures screenshots and measures
idle CPU, memory, resizing and visible paddle response. `FIRE_UI_FONT` selects a
TrueType font if the platform's default font cannot be found.

See [architecture](docs/architecture.md) for the design and implementation limits,
[measurements](docs/performance.md) for native evidence, and [project rules](AGENTS.md).
