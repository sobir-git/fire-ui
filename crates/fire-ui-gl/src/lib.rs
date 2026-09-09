#![doc = include_str!("../README.md")]
mod painter;
mod present;
use femtovg::{renderer::OpenGl as Gl, Canvas};
use fire_ui::*;
use fire_ui_fonts::Fonts;
use fire_ui_native::{Renderer, RendererFactory};
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::GetGlDisplay,
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use painter::GlPainter;
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use winit::{
    event_loop::ActiveEventLoop,
    window::{Window, WindowAttributes},
};
/// GPU resources are selected explicitly and share their font context with the text engine.
pub struct OpenGl {
    pub fonts: Fonts,
    pub partial_repaint: bool,
}
impl RendererFactory for OpenGl {
    fn create(
        self,
        event_loop: &ActiveEventLoop,
        attributes: WindowAttributes,
    ) -> Result<(std::sync::Arc<Window>, Box<dyn Renderer>), String> {
        let (window, config) = DisplayBuilder::new()
            .with_window_attributes(Some(attributes))
            .build(
                event_loop,
                ConfigTemplateBuilder::new()
                    .with_alpha_size(8)
                    .with_stencil_size(8)
                    .with_depth_size(0),
                |configs| {
                    configs
                        .min_by_key(|c| (c.num_samples(), c.depth_size()))
                        .expect("GL config")
                },
            )
            .map_err(|e| e.to_string())?;
        let window = window.ok_or("Window creation failed")?;
        let display = config.display();
        let handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
        let attrs = ContextAttributesBuilder::new().build(Some(handle));
        let fallback = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(None))
            .build(Some(handle));
        // SAFETY: these handles belong to the live window; the GL context stays on this UI thread.
        let context = unsafe {
            display
                .create_context(&config, &attrs)
                .or_else(|_| display.create_context(&config, &fallback))
        }
        .map_err(|e| e.to_string())?;
        let size = window.inner_size();
        let attrs = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            handle,
            NonZeroU32::new(size.width.max(1)).unwrap(),
            NonZeroU32::new(size.height.max(1)).unwrap(),
        );
        let surface =
            unsafe { display.create_window_surface(&config, &attrs) }.map_err(|e| e.to_string())?;
        let context = context.make_current(&surface).map_err(|e| e.to_string())?;
        let _ =
            surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));
        let renderer = unsafe { Gl::new_from_function_cstr(|name| display.get_proc_address(name)) }
            .map_err(|e| e.to_string())?;
        #[cfg(feature = "raster-text")]
        let (canvas, font_ids) = {
            let text = femtovg::TextContext::default();
            let font_ids = self
                .fonts
                .iter()
                .map(|font| {
                    text.add_shared_font_with_index(font.bytes.clone(), font.face_index)
                        .map_err(|error| error.to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            (
                Canvas::new_with_text_context(renderer, text).map_err(|e| e.to_string())?,
                font_ids,
            )
        };
        #[cfg(not(feature = "raster-text"))]
        let canvas = Canvas::new(renderer).map_err(|e| e.to_string())?;
        let presenter = if self.partial_repaint {
            unsafe { crate::present::Presenter::new(|name| display.get_proc_address(name)) }
        } else {
            None
        };
        let window = std::sync::Arc::new(window);
        Ok((
            window.clone(),
            Box::new(Target {
                _window: window,
                canvas,
                backing: None,
                presenter,
                #[cfg(not(feature = "raster-text"))]
                fonts: self.fonts,
                #[cfg(feature = "raster-text")]
                font_ids,
                surface,
                context,
                partial_repaint: self.partial_repaint,
                surface_size: (size.width, size.height),
            }),
        ))
    }
}
struct Target {
    canvas: Canvas<Gl>,
    backing: Option<femtovg::ImageId>,
    presenter: Option<present::Presenter>,
    #[cfg(not(feature = "raster-text"))]
    fonts: Fonts,
    #[cfg(feature = "raster-text")]
    font_ids: Vec<femtovg::FontId>,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    partial_repaint: bool,
    surface_size: (u32, u32),
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
        let s = self;
        let mut full = false;
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        if s.surface_size != (size.width, size.height) {
            s.surface.resize(
                &s.context,
                NonZeroU32::new(size.width).unwrap(),
                NonZeroU32::new(size.height).unwrap(),
            );
            s.surface_size = (size.width, size.height);
            if let Some(image) = s.backing.take() {
                s.canvas.delete_image(image);
            }
            full = true;
        }
        s.canvas.set_size(size.width, size.height, 1.);
        if s.partial_repaint {
            if s.backing.is_none() {
                s.backing = Some(
                    s.canvas
                        .create_image_empty(
                            size.width as usize,
                            size.height as usize,
                            femtovg::PixelFormat::Rgba8,
                            femtovg::ImageFlags::FLIP_Y
                                | femtovg::ImageFlags::PREMULTIPLIED
                                | femtovg::ImageFlags::NEAREST,
                        )
                        .map_err(|e| e.to_string())?,
                );
                full = true;
            }
            let backing = s.backing.unwrap();
            let scale = window.scale_factor() as f32;
            if let Some(damage) = if full {
                Some(Rect::new(
                    0.,
                    0.,
                    size.width as f32 / scale,
                    size.height as f32 / scale,
                ))
            } else {
                damage
            } {
                // Round outwards and include antialiasing at the damage edge.
                let x = (damage.x * scale - 2.).floor().max(0.);
                let y = (damage.y * scale - 2.).floor().max(0.);
                let right = ((damage.x + damage.width) * scale + 2.)
                    .ceil()
                    .min(size.width as f32);
                let bottom = ((damage.y + damage.height) * scale + 2.)
                    .ceil()
                    .min(size.height as f32);
                let region = Rect::new(
                    x / scale,
                    y / scale,
                    (right - x) / scale,
                    (bottom - y) / scale,
                );
                s.canvas
                    .set_render_target(femtovg::RenderTarget::Image(backing));
                s.canvas.reset();
                s.canvas.clear_rect(
                    x as u32,
                    y as u32,
                    (right - x) as u32,
                    (bottom - y) as u32,
                    femtovg::Color::rgbaf(background.0, background.1, background.2, background.3),
                );
                s.canvas.scale(scale, scale);
                {
                    let mut painter = GlPainter::new(
                        &mut s.canvas,
                        #[cfg(not(feature = "raster-text"))]
                        &s.fonts,
                        #[cfg(feature = "raster-text")]
                        &s.font_ids,
                        Rect::new(0., 0., size.width as f32, size.height as f32),
                    );
                    paint(&mut painter, Some(region));
                    painter.status()?;
                }
                if std::env::var_os("FIRE_UI_PROFILE").is_some() {
                    eprintln!("paint_area_pixels={}", (right - x) * (bottom - y));
                }
            }
            s.canvas.set_render_target(femtovg::RenderTarget::Screen);
            if let Some(presenter) = &s.presenter {
                s.canvas.flush();
                presenter.copy(
                    s.canvas
                        .get_native_texture(backing)
                        .map_err(|e| e.to_string())?,
                    size.width,
                    size.height,
                )?;
            } else {
                s.canvas.reset();
                s.canvas.clear_rect(
                    0,
                    0,
                    size.width,
                    size.height,
                    femtovg::Color::rgba(0, 0, 0, 0),
                );
                s.canvas
                    .global_composite_operation(femtovg::CompositeOperation::Copy);
                let mut path = femtovg::Path::new();
                path.rect(0., 0., size.width as f32, size.height as f32);
                s.canvas.fill_path(
                    &path,
                    &femtovg::Paint::image(
                        backing,
                        0.,
                        0.,
                        size.width as f32,
                        size.height as f32,
                        0.,
                        1.,
                    ),
                );
                s.canvas.flush();
            }
        } else {
            s.canvas.set_render_target(femtovg::RenderTarget::Screen);
            s.canvas.reset();
            s.canvas.clear_rect(
                0,
                0,
                size.width,
                size.height,
                femtovg::Color::rgbaf(background.0, background.1, background.2, background.3),
            );
            let scale = window.scale_factor() as f32;
            s.canvas.scale(scale, scale);
            {
                let mut painter = GlPainter::new(
                    &mut s.canvas,
                    #[cfg(not(feature = "raster-text"))]
                    &s.fonts,
                    #[cfg(feature = "raster-text")]
                    &s.font_ids,
                    Rect::new(0., 0., size.width as f32, size.height as f32),
                );
                paint(&mut painter, None);
                painter.status()?;
            }
            s.canvas.flush();
            if std::env::var_os("FIRE_UI_PROFILE").is_some() {
                eprintln!(
                    "paint_area_pixels={}",
                    size.width as u64 * size.height as u64
                );
            }
        }
        s.surface
            .swap_buffers(&s.context)
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
