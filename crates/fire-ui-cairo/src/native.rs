use super::{CairoPainter, FontFaces};
use fire_ui::*;
use fire_ui_fonts::Fonts;
use fire_ui_native::renderer_types::{ActiveEventLoop, Window, WindowAttributes};
use fire_ui_native::{Renderer, RendererFactory};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
/// Direct X11 drawing. It allocates no client framebuffer or retained window image.
pub struct Cairo {
    pub fonts: Fonts,
}
impl RendererFactory for Cairo {
    fn create(
        self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<(std::sync::Arc<Window>, Box<dyn Renderer>), String> {
        let window = event_loop
            .create_window(attributes)
            .map_err(|e| e.to_string())?;
        let RawDisplayHandle::Xlib(display) =
            window.display_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("Cairo requires an Xlib window".into());
        };
        let RawWindowHandle::Xlib(handle) =
            window.window_handle().map_err(|e| e.to_string())?.as_raw()
        else {
            return Err("Cairo requires an Xlib window".into());
        };
        let display = display.display.ok_or("Missing X display")?.as_ptr().cast();
        let size = window.inner_size();
        // The surface is dropped before the window; Xlib handles stay on the window thread.
        let surface = unsafe {
            let mut attrs = std::mem::zeroed();
            if x11::xlib::XGetWindowAttributes(display, handle.window, &mut attrs) == 0 {
                return Err("Cannot query X visual".into());
            }
            cairo::Surface::from_raw_full(cairo::ffi::cairo_xlib_surface_create(
                display,
                handle.window,
                attrs.visual,
                size.width as i32,
                size.height as i32,
            ))
        }
        .map_err(|e| e.to_string())?;
        let window = std::sync::Arc::new(window);
        Ok((
            window.clone(),
            Box::new(Target {
                surface,
                fonts: FontFaces::new(&self.fonts)?,
                size: (size.width, size.height),
                _window: window,
            }),
        ))
    }
}
struct Target {
    surface: cairo::Surface,
    fonts: FontFaces,
    size: (u32, u32),
    _window: std::sync::Arc<Window>,
}
impl Renderer for Target {
    fn render(
        &mut self,
        window: &Window,
        background: Color,
        damage: Option<Rect>,
        paint: &mut dyn FnMut(&mut dyn Painter, Option<Rect>),
    ) -> Result<(), String> {
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let resized = self.size != (size.width, size.height);
        if resized {
            unsafe {
                cairo::ffi::cairo_xlib_surface_set_size(
                    self.surface.to_raw_none(),
                    size.width as i32,
                    size.height as i32,
                )
            }
            self.size = (size.width, size.height);
        }
        let c = cairo::Context::new(&self.surface).map_err(|e| e.to_string())?;
        let scale = window.scale_factor();
        let region = if resized {
            None
        } else {
            damage.map(|d| {
                let x = (d.x as f64 * scale - 2.).floor().max(0.);
                let y = (d.y as f64 * scale - 2.).floor().max(0.);
                let right = ((d.x + d.width) as f64 * scale + 2.)
                    .ceil()
                    .min(size.width as f64);
                let bottom = ((d.y + d.height) as f64 * scale + 2.)
                    .ceil()
                    .min(size.height as f64);
                Rect::new(
                    (x / scale) as f32,
                    (y / scale) as f32,
                    ((right - x) / scale) as f32,
                    ((bottom - y) / scale) as f32,
                )
            })
        };
        if let Some(r) = region {
            c.rectangle(
                r.x as f64 * scale,
                r.y as f64 * scale,
                r.width as f64 * scale,
                r.height as f64 * scale,
            );
            c.clip();
        }
        c.set_source_rgba(
            background.0 as f64,
            background.1 as f64,
            background.2 as f64,
            background.3 as f64,
        );
        c.paint().map_err(|e| e.to_string())?;
        c.scale(scale, scale);
        let mut painter = CairoPainter::new(c, &self.fonts);
        paint(&mut painter, region);
        painter.status()?;
        self.surface.flush();
        unsafe {
            x11::xlib::XFlush(cairo::ffi::cairo_xlib_surface_get_display(
                self.surface.to_raw_none(),
            ));
        }
        Ok(())
    }
}
