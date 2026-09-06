//! Optional native window and OpenGL host. `fire-ui` can also be embedded elsewhere.
mod render;
mod window;
pub use render::{GlPainter, NativeText};
pub use window::{run, NativeApp, WakeHandle, WindowOptions};
