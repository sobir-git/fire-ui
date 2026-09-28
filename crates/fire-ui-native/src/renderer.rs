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
    /// Paint and present the window. `damage: None` requests a full repaint;
    /// `Some(region)` repaints only that logical region. The host does not call
    /// this method for a frame that has no damage and no OS redraw request.
    /// Invoke `paint` with the same full/region choice after any renderer-side
    /// enlargement needed for a recreated surface or antialiasing.
    fn render(
        &mut self,
        window: &Window,
        background: Color,
        damage: Option<Rect>,
        paint: &mut dyn FnMut(&mut dyn Painter, Option<Rect>),
    ) -> Result<(), String>;
}
