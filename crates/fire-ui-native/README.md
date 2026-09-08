# fire-ui-native

Optional native desktop host for Fire UI. Uses winit for windows, glutin/OpenGL and
femtovg for rendering and text shaping, and AccessKit for accessibility. It also
provides clipboard, IME events, file dialogs and a worker-to-UI wake handle.

Call `run(root, WindowOptions::default())` on the main thread. `run_with` additionally
receives root outputs and a `WakeHandle` for background work. `WindowOptions`
controls title, size, native decorations, background, resource limits and font path.

The default font search supports common Linux, macOS and Windows locations.
For predictable deployment, provide your own licensed font using
`WindowOptions::font`, or set `FIRE_UI_FONT` to a TrueType font path. No font is
bundled, and failure to find one is returned as an error.

Linux X11 is verified with real native interactions on Xvfb. Windows and macOS are
build/test targets in CI; native behavior on those desktops still needs testing.
Browser, mobile and embedded hosts are not implemented. Full editable-text
accessibility, bidi editing and platform IME coverage remain incomplete.

See the [quick start](https://github.com/sobir-git/fire-ui#start-an-app).
Experimental 0.x API; MIT licensed.

`WindowOptions::overlay` creates a passive, click-through window above normal
windows. Set `min_size` for small indicators. Linux overlays require X11/XWayland;
native Wayland does not provide this overlay behavior. macOS and Windows overlay
interaction has not been verified.
