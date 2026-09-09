# Fire UI Cairo

Cairo implementation of the public `fire_ui::Painter` interface. The optional
`x11` feature adds native presentation without an OpenGL context. Choose
`Cairo::direct(fonts)` for the lowest memory use, or `Cairo::retained(fonts)` to
copy completed damage from a four-byte-per-pixel client image and prevent visible
clear-then-draw updates.

`CairoPainter::new(context, &faces)` also works with application-owned Cairo
contexts, including image surfaces. `FontFaces::new(&fonts)` keeps the selected
font bytes alive. Pass the same ordered `fire_ui_fonts::Fonts` collection to the
renderer and your text engine; glyph indices are meaningful only for those faces.
An empty collection does not initialize FreeType. Shaping, font discovery and
background services are not renderer dependencies.

Solid, linear, radial and rounded-box brushes, paths, clipping and positioned
text use the same public drawing interface as custom widgets. Box gradients use
small temporary tiles. Renderer errors propagate to the host; custom context
users should check `CairoPainter::status()` after painting.

Linux needs Cairo, FreeType and, for native windows, Xlib development libraries.
The X11 renderer does not provide Wayland presentation. Other renderers can be
selected through `fire_ui_native::RendererFactory`.
