use crate::{GlPainter, NativeText};
use femtovg::{renderer::OpenGl, Canvas};
use fire_ui::*;
use glutin::{
    config::ConfigTemplateBuilder,
    context::{ContextApi, ContextAttributesBuilder, PossiblyCurrentContext},
    display::GetGlDisplay,
    prelude::*,
    surface::{Surface, SurfaceAttributesBuilder, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use std::{
    num::NonZeroU32,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key as NativeKey, ModifiersState, NamedKey},
    window::{Window, WindowId},
};

/// Application messages are separate from widget-local state and event routing.
pub trait NativeApp {
    fn build(&mut self) -> Node;
    fn mounted(&mut self, _wake: WakeHandle, _ui: &mut Ui, _text: &mut NativeText) {}
    fn message(&mut self, _message: Message, _ui: &mut Ui, _now: Duration, _text: &mut NativeText) {
    }
}
type Task = Box<dyn FnOnce(&mut Ui, Duration, &mut NativeText) + Send>;
/// Background producers wake the OS event loop directly. No polling channel is needed.
#[derive(Clone)]
pub struct WakeHandle(EventLoopProxy<Task>);
impl WakeHandle {
    pub fn post(
        &self,
        task: impl FnOnce(&mut Ui, Duration, &mut NativeText) + Send + 'static,
    ) -> Result<(), String> {
        self.0
            .send_event(Box::new(task))
            .map_err(|_| "The UI window is closed".into())
    }
}
pub struct WindowOptions {
    pub title: String,
    pub size: Size,
    pub background: Color,
    /// Keep a backing image for partial repaint. Costs an extra framebuffer.
    pub retain_surface: bool,
    /// Deliver the latest queued position before a frame or another input event.
    /// Disable for custom widgets that need every mouse sample delivered by the OS.
    pub coalesce_pointer_moves: bool,
}
impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Fire UI".into(),
            size: Size::new(1120., 800.),
            background: Color::hex(0x171917),
            retain_surface: false,
            coalesce_pointer_moves: true,
        }
    }
}

// Declaration order releases renderer resources before the GL context and window.
struct State {
    canvas: Canvas<OpenGl>,
    text: NativeText,
    ui: Ui,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    window: Window,
    occluded: bool,
    backing: Option<(femtovg::ImageId, u32, u32)>,
    resize_received: Option<Instant>,
    surface_size: (u32, u32),
    profile: bool,
    pointer_received: Option<Instant>,
}
struct Host<A> {
    app: A,
    options: WindowOptions,
    state: Option<State>,
    start: Instant,
    modifiers: ModifiersState,
    pointer: Point,
    pending_pointer: bool,
    clipboard: Option<arboard::Clipboard>,
    ime_composing: bool,
    wake: WakeHandle,
    error: Option<String>,
}

pub fn run(app: impl NativeApp + 'static, options: WindowOptions) -> Result<(), String> {
    let event_loop = EventLoop::<Task>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut host = Host {
        app,
        options,
        state: None,
        start: Instant::now(),
        modifiers: ModifiersState::empty(),
        pointer: Point::default(),
        pending_pointer: false,
        clipboard: None,
        ime_composing: false,
        wake: WakeHandle(event_loop.create_proxy()),
        error: None,
    };
    event_loop.run_app(&mut host).map_err(|e| e.to_string())?;
    host.error.map_or(Ok(()), Err)
}
impl<A: NativeApp> Host<A> {
    fn process(&mut self) {
        let Some(s) = &mut self.state else {
            return;
        };
        let now = self.start.elapsed();
        for request in s.ui.platform_requests() {
            if self.clipboard.is_none() {
                self.clipboard = arboard::Clipboard::new().ok();
            }
            match request {
                PlatformRequest::Copy(text) => {
                    if let Some(cb) = &mut self.clipboard {
                        if let Err(e) = cb.set_text(text) {
                            eprintln!("Clipboard write failed: {e}");
                        }
                    }
                }
                PlatformRequest::Paste(id) => {
                    if let Some(cb) = &mut self.clipboard {
                        match cb.get_text() {
                            Ok(value) => {
                                s.ui.send(id, Event::Paste(value), now, &mut s.text);
                            }
                            Err(e) => eprintln!("Clipboard read failed: {e}"),
                        }
                    }
                }
            }
        }
        for message in s.ui.messages() {
            self.app.message(message, &mut s.ui, now, &mut s.text);
        }
    }
    fn dispatch(&mut self, event: Event) {
        if let Some(s) = &mut self.state {
            s.ui.dispatch(event, self.start.elapsed(), &mut s.text);
        }
        self.process();
    }
    fn flush_pointer(&mut self) {
        if std::mem::take(&mut self.pending_pointer) {
            self.dispatch(Event::PointerMove(self.pointer));
            if let Some(s) = &mut self.state {
                if !s.ui.needs_paint() {
                    s.pointer_received = None;
                }
            }
        }
    }
}
impl<A: NativeApp> ApplicationHandler<Task> for Host<A> {
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, task: Task) {
        self.flush_pointer();
        if let Some(s) = &mut self.state {
            task(&mut s.ui, self.start.elapsed(), &mut s.text);
        }
        self.process();
    }
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match create_state(event_loop, &self.options, self.app.build()) {
            Ok(mut state) => {
                self.app
                    .mounted(self.wake.clone(), &mut state.ui, &mut state.text);
                state.window.request_redraw();
                self.state = Some(state);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        // Preserve ordering around clicks and keys, and use the newest queued pointer
        // position before drawing. Intermediate moves need no separate tree traversal.
        if !matches!(event, WindowEvent::CursorMoved { .. }) {
            self.flush_pointer();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Focused(false) => {
                if let Some(s) = &mut self.state {
                    s.ui.window_focus_lost(self.start.elapsed(), &mut s.text);
                }
                self.process();
            }
            WindowEvent::Occluded(value) => {
                if let Some(s) = &mut self.state {
                    s.occluded = value;
                    if !value {
                        s.ui.repaint_all();
                        s.window.request_redraw();
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(s) = &mut self.state {
                    if size.width > 0 && size.height > 0 {
                        // Coalesce events. Resize GPU resources once, at the next rendered frame.
                        s.resize_received = Some(Instant::now());
                        let scale = s.window.scale_factor() as f32;
                        s.ui.resize(Size::new(
                            size.width as f32 / scale,
                            size.height as f32 / scale,
                        ));
                        s.window.request_redraw();
                    }
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(s) = &mut self.state {
                    let size = s.window.inner_size();
                    s.ui.resize(Size::new(
                        size.width as f32 / scale_factor as f32,
                        size.height as f32 / scale_factor as f32,
                    ));
                    s.ui.invalidate();
                    s.window.request_redraw();
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self
                    .state
                    .as_ref()
                    .map(|s| s.window.scale_factor() as f32)
                    .unwrap_or(1.);
                self.pointer = Point::new(position.x as f32 / scale, position.y as f32 / scale);
                self.pending_pointer = true;
                if let Some(s) = &mut self.state {
                    if s.profile {
                        s.pointer_received = Some(Instant::now());
                    }
                }
                if !self.options.coalesce_pointer_moves {
                    self.flush_pointer();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                self.dispatch(if state == ElementState::Pressed {
                    Event::PointerDown(self.pointer)
                } else {
                    Event::PointerUp(self.pointer)
                });
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self
                    .state
                    .as_ref()
                    .map(|s| s.window.scale_factor() as f32)
                    .unwrap_or(1.);
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Point::new(x * 38., y * 38.),
                    MouseScrollDelta::PixelDelta(p) => {
                        Point::new(p.x as f32 / scale, p.y as f32 / scale)
                    }
                };
                self.dispatch(Event::Scroll {
                    position: self.pointer,
                    delta,
                });
            }
            WindowEvent::Ime(Ime::Preedit(value, _)) => {
                self.ime_composing = !value.is_empty();
                self.dispatch(Event::Composition(value));
            }
            WindowEvent::Ime(Ime::Commit(value)) => {
                self.ime_composing = false;
                self.dispatch(Event::Text(value));
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state == ElementState::Pressed => {
                let m = Modifiers {
                    shift: self.modifiers.shift_key(),
                    control: self.modifiers.control_key(),
                    alt: self.modifiers.alt_key(),
                    meta: self.modifiers.super_key(),
                };
                if let Some(key) = key(&event.logical_key) {
                    self.dispatch(Event::Key { key, modifiers: m });
                }
                if !m.control && !m.meta && !self.ime_composing {
                    if let Some(value) = event.text {
                        let filtered: String = value.chars().filter(|c| !c.is_control()).collect();
                        if !filtered.is_empty() {
                            self.dispatch(Event::Text(filtered));
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(state) = &mut self.state {
                    if let Err(error) = render_frame(state, &self.options) {
                        self.error = Some(error);
                        event_loop.exit();
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.flush_pointer();
        if let Some(s) = &mut self.state {
            if !s.occluded {
                s.ui.advance(self.start.elapsed(), &mut s.text);
            }
        }
        self.process();
        if let Some(s) = &self.state {
            if s.ui.needs_paint() && !s.occluded {
                s.window.request_redraw();
            }
        }
        let deadline = self
            .state
            .as_ref()
            .filter(|s| !s.occluded)
            .and_then(|s| s.ui.next_deadline());
        event_loop.set_control_flow(match deadline {
            Some(d) => ControlFlow::WaitUntil(self.start + d),
            None => ControlFlow::Wait,
        });
    }
}
fn key(key: &NativeKey) -> Option<Key> {
    Some(match key {
        NativeKey::Named(k) => match k {
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Space => Key::Character(' '),
            _ => return None,
        },
        NativeKey::Character(s) => Key::Character(s.chars().next()?.to_ascii_lowercase()),
        _ => return None,
    })
}
fn create_state(
    event_loop: &ActiveEventLoop,
    options: &WindowOptions,
    root: Node,
) -> Result<State, String> {
    let attrs = Window::default_attributes()
        .with_title(&options.title)
        .with_inner_size(LogicalSize::new(options.size.width, options.size.height));
    let (window, config) = DisplayBuilder::new()
        .with_window_attributes(Some(attrs))
        .build(
            event_loop,
            ConfigTemplateBuilder::new()
                .with_alpha_size(8)
                .with_depth_size(0),
            |configs| {
                configs
                    .min_by_key(|c| (c.num_samples(), c.depth_size()))
                    .expect("GL config")
            },
        )
        .map_err(|e| e.to_string())?;
    let window = window.ok_or("Window was not created")?;
    let display = config.display();
    let handle = window.window_handle().map_err(|e| e.to_string())?.as_raw();
    let attrs = ContextAttributesBuilder::new().build(Some(handle));
    let fallback = ContextAttributesBuilder::new()
        .with_context_api(ContextApi::Gles(None))
        .build(Some(handle));
    // SAFETY: handles belong to the live window; all GL operations stay on this thread.
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
    let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));
    let gl = unsafe { OpenGl::new_from_function_cstr(|name| display.get_proc_address(name)) }
        .map_err(|e| e.to_string())?;
    let text = NativeText::new()?;
    let canvas =
        Canvas::new_with_text_context(gl, text.context.clone()).map_err(|e| e.to_string())?;
    let scale = window.scale_factor() as f32;
    let ui = Ui::new(
        root,
        Size::new(size.width as f32 / scale, size.height as f32 / scale),
    );
    window.set_ime_allowed(true);
    Ok(State {
        canvas,
        text,
        ui,
        surface,
        context,
        window,
        occluded: false,
        backing: None,
        resize_received: None,
        surface_size: (size.width, size.height),
        profile: std::env::var_os("FIRE_UI_PROFILE").is_some(),
        pointer_received: None,
    })
}

fn render_frame(s: &mut State, options: &WindowOptions) -> Result<(), String> {
    let started = s.profile.then(Instant::now);
    let size = s.window.inner_size();
    if size.width == 0 || size.height == 0 || s.occluded {
        return Ok(());
    }
    let scale = s.window.scale_factor() as f32;
    s.canvas.set_size(size.width, size.height, 1.);
    s.canvas.reset();
    if s.surface_size != (size.width, size.height) {
        s.surface.resize(
            &s.context,
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        s.surface_size = (size.width, size.height);
        s.ui.resize(Size::new(
            size.width as f32 / scale,
            size.height as f32 / scale,
        ));
        s.ui.repaint_all();
    }
    if options.retain_surface
        && s.backing
            .is_none_or(|(_, w, h)| w != size.width || h != size.height)
    {
        if let Some((id, _, _)) = s.backing.take() {
            s.canvas.delete_image(id);
        }
        let id = s
            .canvas
            .create_image_empty(
                size.width as usize,
                size.height as usize,
                femtovg::PixelFormat::Rgba8,
                femtovg::ImageFlags::FLIP_Y,
            )
            .map_err(|e| e.to_string())?;
        s.backing = Some((id, size.width, size.height));
        s.ui.repaint_all();
    }
    s.ui.layout(&mut s.text);
    let c = options.background;
    let color = femtovg::Color::rgbaf(c.0, c.1, c.2, c.3);
    if let Some((backing, _, _)) = s.backing {
        if let Some(damage) = s.ui.damage() {
            s.canvas
                .set_render_target(femtovg::RenderTarget::Image(backing));
            let (x, y, w, h) = physical_damage(damage, scale, size.width, size.height);
            s.canvas.clear_rect(x, y, w, h, color);
            s.canvas.scale(scale, scale);
            s.ui.paint_damage(&mut GlPainter::new(&mut s.canvas, &s.text));
        }
        s.canvas.set_render_target(femtovg::RenderTarget::Screen);
        s.canvas.reset();
        let mut quad = femtovg::Path::new();
        quad.rect(0., 0., size.width as f32, size.height as f32);
        s.canvas.fill_path(
            &quad,
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
    } else {
        s.canvas.clear_rect(0, 0, size.width, size.height, color);
        s.canvas.scale(scale, scale);
        s.ui.paint(&mut GlPainter::new(&mut s.canvas, &s.text));
    }
    s.canvas.flush();
    let submitted = s.profile.then(Instant::now);
    s.surface
        .swap_buffers(&s.context)
        .map_err(|e| e.to_string())?;
    if let (Some(started), Some(submitted)) = (started, submitted) {
        eprintln!(
            "frame_build_us={} swap_us={}",
            submitted.duration_since(started).as_micros(),
            submitted.elapsed().as_micros()
        );
        if let Some(received) = s.pointer_received.take() {
            eprintln!("pointer_frame_us={}", received.elapsed().as_micros());
        }
    }
    if let Some(received) = s.resize_received.take() {
        if s.profile {
            eprintln!("resize_frame_us={}", received.elapsed().as_micros());
        }
    }
    Ok(())
}

fn physical_damage(r: Rect, scale: f32, width: u32, height: u32) -> (u32, u32, u32, u32) {
    let x = (r.x * scale).floor().clamp(0., width as f32) as u32;
    let y = (r.y * scale).floor().clamp(0., height as f32) as u32;
    let right = ((r.x + r.width) * scale)
        .ceil()
        .clamp(x as f32, width as f32) as u32;
    let bottom = ((r.y + r.height) * scale)
        .ceil()
        .clamp(y as f32, height as f32) as u32;
    (x, y, right - x, bottom - y)
}
