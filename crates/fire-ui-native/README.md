# fire-ui-native

Native desktop host for Fire UI. The default build includes X11 windows,
OpenGL rendering and text shaping. Core semantics and IME handling remain available.
Optional services are selected by the application:

| Cargo feature | Adds |
| --- | --- |
| `x11` (default) | Linux X11 window and GLX backend |
| `accessibility` | Native screen-reader bridge; Linux AT-SPI EditableText |
| `inspection` | Unix JSON inspection socket for agents |
| `clipboard` | System clipboard ownership and transfers |
| `dialogs` | `open_file` and `save_file` system choosers |
| `bitmap-fonts` | PNG decoding for bitmap glyphs, including color emoji |

```toml
fire-ui-native = { git = "https://github.com/sobir-git/fire-ui", tag = "v0.5.0", default-features = false, features = ["x11", "accessibility", "clipboard"] }
```

Call `run` on the main thread. `run_with` additionally receives root outputs and a
`WakeHandle` for background work. `WindowOptions` controls window properties,
resource limits, font files and repaint policy. Windows uses WGL and macOS uses CGL;
Linux builds with defaults disabled must explicitly select `x11`. Native Wayland
support is deferred.

The host loads **no fonts by default**. Draw-only apps need no font files. Apps with
text must set `font: Some(path)`; requesting text without a font returns a native
render error. `system_font()` is an explicit convenience for discovering one common
system font, respecting `FIRE_UI_FONT`. For predictable deployment, supply your own
licensed file. `fallback_fonts` is an empty, ordered list unless the app provides
additional faces. No fonts are bundled. Invalid configured files return an error.
Bitmap-only glyphs require `bitmap-fonts`; without it, use outline fonts. Enabling
that feature does not discover or load fonts. The renderer still compiles its
shaping dependencies even in a draw-only build.

Accessibility does not require clipboard support. Without `clipboard`, native
CopyText, CutText and PasteText return an unsupported-operation error; Set, Insert
and Delete remain available. Built-in keyboard clipboard shortcuts require the
`clipboard` feature; disabled Cut preserves the selection. Custom hosts explicitly
call `Ui::set_clipboard_enabled(true)` when they service clipboard requests. In-process semantics need neither native bridge nor socket.

Linux/X11 requires the system X11, GLX/OpenGL and xkbcommon libraries at runtime.
On Debian/Ubuntu, install `libxkbcommon-x11-0` as well as `libxkbcommon0` and
your OpenGL driver. The X11 keyboard library is loaded dynamically by winit.

Linux X11 is verified with real native interactions on Xvfb. Windows and macOS are
build/test targets in CI; native behavior on those desktops still needs testing.
Browser, mobile and embedded hosts are not implemented. Mixed-direction shaping,
visual selection/caret geometry, IME composition selection and AccessKit text runs
are implemented. Linux probes exercise real IBus Cangjie5/XIM composition and Orca
speech generation/navigation. All six AT-SPI EditableText methods share editor
validation and undo; AT-SPI clipboard reads do not block the UI thread. The private Linux
transport derives from AccessKit Unix 0.23 under its MIT license, with the published
common adapter providing tree translation. Windows/macOS use AccessKit's platform
adapters; their native IME and screen-reader verification is future work.

Enable `inspection` and set `FIRE_UI_INSPECT` to a socket path in a private directory to inspect and operate
widgets on Unix. Requests use the same semantic actions as native accessibility.
See [the inspection protocol](https://github.com/sobir-git/fire-ui/blob/v0.5.0/docs/inspection.md).
No socket exists by default.

The default renderer paints directly to the window, with no retained window image.
Set `WindowOptions::partial_repaint` to `true` to retain pixels and repaint damaged
regions. This trades a window-sized RGBA image and stencil storage for less drawing
work during local animation. OpenGL 3+ blits the retained image; older contexts draw
its texture. Geometry changes invalidate it. Both modes sleep when idle.

Cargo features are additive. A richer app in the same workspace can enable its
features for other consumers during a shared build. Use a separate consumer build
to measure the minimal package; `tools/lean_probe.py` does this in the repository.

See the [quick start](https://github.com/sobir-git/fire-ui#start-an-app).
Experimental 0.x API; MIT licensed.

`WindowOptions::overlay` creates a passive, click-through window above normal
windows. Set `min_size` for small indicators. Linux overlays require X11/XWayland;
native Wayland does not provide this overlay behavior. macOS and Windows overlay
interaction has not been verified.
