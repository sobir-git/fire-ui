# Fire UI

A Rust UI toolkit built around persistent widgets and typed, owned children.
The framework and reusable widgets are the product. Fire Notes is a consumer.

```sh
cargo run --release -p fire-notes
```

Fire Notes reproduces the original compact black-and-orange interface: inline tab
titles, warm monospaced text, a caret-anchored command palette, and animated fire on
typed and selected text. The app composes the new framework's public widgets and
editor decoration API. The old renderer and widgets are not part of the build.

Notes live in `tmp/fire-notes` by default. Set `FIRE_NOTES_DIR` or pass
`--data-dir PATH` to choose another folder. Files contain raw Markdown or plain text;
titles and session state are separate metadata. Closing a tab keeps its file. The
session remembers tab order, active note, caret, selection, scroll, per-tab wrap,
window size and position. Saves run on one background thread. Closing waits for
outstanding note saves and keeps the window open if a note cannot be saved.

| Shortcut or gesture | Action |
| --- | --- |
| Ctrl N / Ctrl W | New note / close tab; the last saved tab stays open |
| Ctrl Tab / Ctrl Shift Tab / Ctrl 1–9 | Switch tabs |
| Right click tab | Rename, Save, Word wrap, Close and Trash actions |
| Ctrl R / middle click tab | Rename; Enter or blur commits, Escape cancels |
| Drag tab / wheel over tabs | Reorder / scroll tabs |
| Ctrl P | Search saved notes |
| Ctrl Shift T | Open Trash and restore a note |
| Ctrl O / drop a file | Open a Markdown or text file |
| Ctrl Shift O | Open by entering a path |
| Ctrl / | Commands beside the caret |
| Ctrl S / Ctrl Shift S | Save / system Save As dialog |
| Ctrl Z / Ctrl Shift Z / Ctrl Y | Undo / redo |
| Ctrl C / Ctrl X / Ctrl V | Clipboard |
| Alt Z | Toggle this tab's word wrap, off by default |
| Alt Up / Alt Down | Move the current or selected lines |
| Double / triple click, Shift click | Select word / line, extend selection |
| Ctrl Q | Save and quit |

Arrow, word, line, document and page navigation support keyboard selection. Drag
selection scrolls beyond the viewport; the scrollbar also supports dragging. Custom
window controls support minimize, maximize/restore, drag and edge resizing. Fire
animation stops after typing fades, when selection clears, or when the editor loses
focus. It does not keep an idle editor repainting.

Right-click a tab and choose **Move to Trash** to remove its note from the library.
**Open Trash**, also available in the command palette, lists recoverable notes with
their remaining time. Select one to restore its title, text, cursor and wrapping.
Restoring never overwrites a file that has appeared at its original location.
Close tab only closes the tab; it does not trash the note.
New empty Untitled tabs stay in memory and are discarded on close or quit, including
the last tab. They create no note file and are excluded from the saved session.
Typing content, changing the name, or explicitly saving makes a note persistent.

Trash expires after 30 days. Cleanup runs at startup and hourly while the app is
open. The installed Linux user timer also runs hourly while the app is closed and
catches up after login. Install it after installing the app launcher:

```sh
install -Dm644 tools/systemd/fire-notes-trash.service ~/.config/systemd/user/fire-notes-trash.service
install -Dm644 tools/systemd/fire-notes-trash.timer ~/.config/systemd/user/fire-notes-trash.timer
systemctl --user daemon-reload
systemctl --user enable --now fire-notes-trash.timer
```

`fire-notes --purge-trash` performs the same expiry check without opening a window.
Recovery copies and timestamps live in the library's `trash/` directory.

This prototype has no format migration, file watching or concurrent-writer conflict
resolution. Note bodies are limited to 2 MiB and titles to 4 KiB; the framework editor's
byte limit is configurable. There is no app-specific open-tab cap, although framework
node and message budgets still apply. Large-document editing needs further profiling.

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
python3 tools/tab_menu_probe.py --output artifacts/tab-menu
```

The native probe requires Linux, Xvfb, xdotool and Pillow with XCB support. It runs
on an isolated display and, with `--accessibility`, a private accessibility bus.
It exercises real input, verifies an accessibility-triggered counter increment, captures screenshots and measures
idle CPU, memory, resizing and visible paddle response. `FIRE_UI_FONT` selects a
TrueType font if the platform's default font cannot be found.

The notes probe also requires xclip and a native file chooser such as Zenity. It
uses temporary files, isolated chooser settings and its own display. It checks editing,
autosave, independent tabs, pickers, external file loading, narrow layouts,
save-before-close and restart, and records screenshots and process measurements.
Its optional `--telegram` flag sends screenshots using the locally installed
telegram-notify skill; use it only when the user has requested delivery.

See [visual design](DESIGN.md) for Fire Notes’ appearance and interaction rules,
[architecture](docs/architecture.md) for the framework design and implementation limits,
[measurements](docs/performance.md) for native evidence, and [project rules](AGENTS.md).
