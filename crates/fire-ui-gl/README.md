# Fire UI OpenGL

Explicit GPU drawing and font resources for the native host. The default renderer
draws font outlines without a rasterizer. Enable `raster-text` to opt into Swash's
cached glyph masks for ordinary solid UI text; the core, widgets and other renderers
do not acquire it. FemtoVG falls back to outlines for text with unsupported paint or
transforms and for very large effective sizes.
