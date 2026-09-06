//! Optional composition and controls. Applications can implement Widget directly.
mod controls;
mod layout;
mod text;
pub use controls::*;
pub use layout::*;
pub use text::*;

use fire_ui::Color;
#[derive(Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub panel: Color,
    pub raised: Color,
    pub border: Color,
    pub foreground: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub radius: f32,
    pub padding: f32,
    pub font_size: f32,
}
impl Default for Theme {
    fn default() -> Self {
        Self {
            background: Color::hex(0x171917),
            panel: Color::hex(0x20231f),
            raised: Color::hex(0x2b2f29),
            border: Color::hex(0x3c4238),
            foreground: Color::hex(0xeeeedd),
            muted: Color::hex(0xa0a795),
            accent: Color::hex(0xefaa63),
            selection: Color::hex(0xefaa63).alpha(0.22),
            radius: 6.,
            padding: 12.,
            font_size: 15.,
        }
    }
}
