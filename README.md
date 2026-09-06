# Fire UI

A Rust UI toolkit built around persistent widgets and typed, owned children.
The framework and reusable widgets are the product. Fire Notes is a consumer.

```sh
cargo run --release -p fire-notes
```

Fire Notes has editable titles, independent editor state per tab, note search,
a command palette, file opening, undo/redo and automatic Markdown saving. It uses
the public framework and widgets throughout. Saves run on one background thread;
closing waits for outstanding note saves and keeps the window open on failure.

Notes live in `tmp/fire-notes` by default. Set `FIRE_NOTES_DIR` or pass
`--data-dir PATH` to choose another folder. A note is a Markdown file with its title
in the first heading. Closing a tab keeps the file. Session state remembers open
tabs and window size. This prototype has no earlier-format migration, file watching,
or concurrent-writer conflict resolution. It supports up to 16 open tabs, a 2 MiB
body and a 4 KiB title per note. Large-document editing still needs performance work.

| Shortcut | Action |
| --- | --- |
| Ctrl N / Ctrl W | New note / close tab |
| Ctrl Tab / Ctrl Shift Tab | Next / previous tab |
| Ctrl P / Ctrl O | Find note / open file path |
| Ctrl / | Command palette |
| Ctrl S | Save or retry a failed save |
| Ctrl Z / Ctrl Shift Z / Ctrl Y | Undo / redo |
| Alt Z | Toggle word wrap |
| Ctrl Q | Save and quit |

The framework studio is a separate consumer:

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
- `apps/fire-notes`: the notes app, composed entirely through public APIs.

Previous frameworks and executable design skeletons have been removed. Git history
is their archive. There are no compatibility shims or parallel implementations.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p fire-ui-studio
python3 tools/native_probe.py --accessibility --output artifacts/native
cargo build --release -p fire-notes
python3 tools/notes_probe.py --output artifacts/notes
```

The native probe requires Linux, Xvfb, xdotool and Pillow with XCB support. It runs
on an isolated display and, with `--accessibility`, a private accessibility bus.
It exercises real input, verifies an accessibility-triggered counter increment, captures screenshots and measures
idle CPU, memory, resizing and visible paddle response. `FIRE_UI_FONT` selects a
TrueType font if the platform's default font cannot be found.

The notes probe uses temporary files and its own display. It checks editing,
autosave, independent tabs, pickers, external file loading, narrow layouts,
save-before-close and restart, and records screenshots and process measurements.
Its optional `--telegram` flag sends screenshots using the locally installed
telegram-notify skill; use it only when the user has requested delivery.

See [architecture](docs/architecture.md) for the design and implementation limits,
[measurements](docs/performance.md) for native evidence, and [project rules](AGENTS.md).
