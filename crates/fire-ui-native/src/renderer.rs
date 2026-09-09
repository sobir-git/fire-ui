use fire_ui::{Color, Painter, Rect};
pub use winit::{
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};
/// Creates a native window and its drawing resources on the UI thread.
/// Implementations must release their resources before the returned window is dropped.
pub trait RendererFactory: 'static {
    fn create(
        self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<(std::sync::Arc<Window>, Box<dyn Renderer>), String>;
}
/// A window drawing target. Fonts and text services are supplied by the application.
pub trait Renderer {
    /// Invoke paint with None for the whole window, or a clipped logical damage region.
    fn render(
        &mut self,
        window: &Window,
        background: Color,
        damage: Option<Rect>,
        paint: &mut dyn FnMut(&mut dyn Painter, Option<Rect>),
    ) -> Result<(), String>;
}
