# Fire UI native

Desktop windows, input routing and optional platform services. The host accepts
an explicit `TextEngine` and `RendererFactory`; it does not initialize a renderer,
load fonts, or discover a font collection itself.

| Feature | Adds |
| --- | --- |
| `x11` (default) | Linux X11 windows and keyboard input |
| `accessibility` | Native screen-reader bridge and Linux AT-SPI editing |
| `inspection` | Optional Unix inspection socket |
| `clipboard` | System clipboard ownership and transfers |
| `dialogs` | Explicitly invoked native file choosers |

Call `run(root, options, text, renderer)` on the main thread. `run_with` also
receives root outputs and a `WakeHandle` for application background work.
`WindowOptions` controls window properties and runtime limits. Rendering policy
and text resources belong to their respective implementations.

`fire-ui-cairo` provides direct X11 drawing. `fire-ui-gl` provides optional OpenGL
drawing and retained repainting. Both consume the same public `Painter` and
positioned-glyph paragraph contract. Custom renderers implement the same
`RendererFactory` and `Renderer` interfaces as these implementations.

Linux requires X11 and xkbcommon libraries, including `libxkbcommon-x11-0` on
Debian/Ubuntu. GPU libraries are required only by the OpenGL renderer. Native
Wayland support is not implemented. Windows and macOS retain the OpenGL host
path and CI build checks; their native interaction parity needs verification.

Core semantics and IME routing do not require accessibility or inspection.
Accessibility can be built without clipboard support; AT-SPI clipboard methods
then report unsupported operations while set, insert and delete remain available.
Editor validation and undo also apply to native accessibility edits.

With `inspection`, set `FIRE_UI_INSPECT` to a socket path in a private directory.
No socket exists by default. See the repository's `docs/inspection.md` for its
semantic query/action protocol.

`WindowOptions::overlay` creates a passive, click-through window. Linux overlays
require X11/XWayland; macOS and Windows overlay interaction remains unverified.

Cargo features are additive across a shared build. `tools/lean_probe.py` builds
independent consumers to expose the actual cost of optional integrations.
