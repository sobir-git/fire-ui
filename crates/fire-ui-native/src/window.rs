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
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{Key as OsKey, ModifiersState, NamedKey, PhysicalKey},
    window::{Window, WindowId},
};
#[derive(Clone)]
pub struct WindowOptions {
    pub title: String,
    pub size: Size,
    pub background: Color,
    pub limits: Limits,
}
impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Fire UI".into(),
            size: Size::new(1100., 780.),
            background: Color::hex(0x171918),
            limits: Limits::default(),
        }
    }
}
enum HostEvent {
    Command(Posted),
    Accessibility(accesskit_winit::Event),
}
impl From<accesskit_winit::Event> for HostEvent {
    fn from(event: accesskit_winit::Event) -> Self {
        Self::Accessibility(event)
    }
}
struct Posted {
    command: Box<dyn std::any::Any + Send>,
    ticket: Option<Box<dyn std::any::Any + Send>>,
    bytes: usize,
}
struct Pending<W: Widget> {
    command: W::Command,
    ticket: Option<Ticket<W>>,
    bytes: usize,
}
pub struct WakeHandle<W: Widget> {
    proxy: EventLoopProxy<HostEvent>,
    marker: std::marker::PhantomData<fn(W) -> W>,
    count: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
}
impl<W: Widget> Clone for WakeHandle<W> {
    fn clone(&self) -> Self {
        Self {
            proxy: self.proxy.clone(),
            marker: std::marker::PhantomData,
            count: self.count.clone(),
            bytes: self.bytes.clone(),
        }
    }
}
impl<W: Widget<Command: Send>> WakeHandle<W> {
    pub fn post(&self, command: W::Command) -> Result<(), (Error, W::Command)> {
        self.enqueue(None, command)
    }
    pub fn complete(
        &self,
        ticket: Ticket<W>,
        command: W::Command,
    ) -> Result<(), (Error, W::Command)> {
        self.enqueue(Some(ticket), command)
    }
    fn enqueue(
        &self,
        ticket: Option<Ticket<W>>,
        command: W::Command,
    ) -> Result<(), (Error, W::Command)> {
        let bytes = command.bytes();
        if self
            .count
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| {
                (n < 64).then_some(n + 1)
            })
            .is_err()
        {
            return Err((Error::Full, command));
        }
        if self
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Relaxed, |n| {
                n.checked_add(bytes).filter(|n| *n <= 4 * 1024 * 1024)
            })
            .is_err()
        {
            self.count.fetch_sub(1, Ordering::AcqRel);
            return Err((Error::Full, command));
        }
        match self.proxy.send_event(HostEvent::Command(Posted {
            command: Box::new(command),
            ticket: ticket.map(|ticket| Box::new(ticket) as Box<dyn std::any::Any + Send>),
            bytes,
        })) {
            Ok(()) => Ok(()),
            Err(error) => {
                self.count.fetch_sub(1, Ordering::AcqRel);
                self.bytes.fetch_sub(bytes, Ordering::AcqRel);
                let HostEvent::Command(post) = error.0 else {
                    unreachable!()
                };
                Err((
                    Error::Stale,
                    *post
                        .command
                        .downcast::<W::Command>()
                        .expect("private native command type"),
                ))
            }
        }
    }
}
// Drop GPU resources before the context and window.
struct State<W: Widget> {
    accessibility: accesskit_winit::Adapter,
    semantic_revision: Option<u64>,
    canvas: Canvas<OpenGl>,
    text: NativeText,
    ui: Ui<W>,
    surface: Surface<WindowSurface>,
    context: PossiblyCurrentContext,
    window: Window,
    surface_size: (u32, u32),
    occluded: bool,
    resize_at: Option<Instant>,
}
struct Host<W: Widget, F> {
    root: Option<Element<W>>,
    options: WindowOptions,
    state: Option<State<W>>,
    start: Instant,
    pointer: Point,
    pending_pointer: bool,
    next_frame: Instant,
    modifiers: ModifiersState,
    composing: bool,
    ime_session: Option<u64>,
    ime_target: Option<u64>,
    clipboard: Option<arboard::Clipboard>,
    wake: WakeHandle<W>,
    output: F,
    error: Option<String>,
    close_requested: bool,
    profile: bool,
    pending_posts: std::collections::VecDeque<Pending<W>>,
}
pub fn run<W: Widget>(root: Element<W>, options: WindowOptions) -> Result<(), String> {
    run_with(root, options, |_, _| {})
}
pub fn run_with<W: Widget, F: FnMut(W::Output, &WakeHandle<W>) + 'static>(
    root: Element<W>,
    options: WindowOptions,
    output: F,
) -> Result<(), String> {
    let event_loop = EventLoop::<HostEvent>::with_user_event()
        .build()
        .map_err(|e| e.to_string())?;
    let wake = WakeHandle {
        proxy: event_loop.create_proxy(),
        marker: std::marker::PhantomData,
        count: Arc::new(AtomicUsize::new(0)),
        bytes: Arc::new(AtomicUsize::new(0)),
    };
    let mut host = Host {
        root: Some(root),
        options,
        state: None,
        start: Instant::now(),
        pointer: Point::default(),
        pending_pointer: false,
        next_frame: Instant::now(),
        modifiers: ModifiersState::empty(),
        composing: false,
        ime_session: None,
        ime_target: None,
        clipboard: None,
        wake,
        output,
        error: None,
        close_requested: false,
        profile: std::env::var_os("FIRE_UI_PROFILE").is_some(),
        pending_posts: std::collections::VecDeque::new(),
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    event_loop.run_app(&mut host).map_err(|e| e.to_string())?;
    host.error.map_or(Ok(()), Err)
}
impl<W: Widget, F: FnMut(W::Output, &WakeHandle<W>)> Host<W, F> {
    fn service(&mut self) {
        let Some(s) = &mut self.state else { return };
        let now = self.start.elapsed();
        s.ui.advance(now, 128);
        let mut requests = vec![];
        s.ui.pump(
            256,
            |output| (self.output)(output, &self.wake),
            |request| requests.push(request),
        );
        for request in requests {
            if !matches!(request, HostRequest::Close) && self.clipboard.is_none() {
                self.clipboard = arboard::Clipboard::new().ok()
            }
            match request {
                HostRequest::Close => self.close_requested = true,
                HostRequest::Copy(text) => {
                    if let Some(clipboard) = &mut self.clipboard {
                        let _ = clipboard.set_text(text);
                    }
                }
                HostRequest::Paste(token) => {
                    if let Some(clipboard) = &mut self.clipboard {
                        if let Ok(text) = clipboard.get_text() {
                            s.ui.paste(token, text)
                        }
                    }
                }
            }
        }
        while let Some(post) = self.pending_posts.pop_front() {
            let delivered = if let Some(ticket) = post.ticket {
                s.ui.post(ticket, post.command)
            } else {
                s.ui.send(post.command)
            };
            match delivered {
                Ok(()) => {
                    self.wake.count.fetch_sub(1, Ordering::AcqRel);
                    self.wake.bytes.fetch_sub(post.bytes, Ordering::AcqRel);
                }
                Err((Error::Full, command)) => {
                    self.pending_posts.push_front(Pending {
                        command,
                        ticket: post.ticket,
                        bytes: post.bytes,
                    });
                    break;
                }
                Err(_) => {
                    self.wake.count.fetch_sub(1, Ordering::AcqRel);
                    self.wake.bytes.fetch_sub(post.bytes, Ordering::AcqRel);
                }
            }
        }
        s.ui.layout(&mut s.text);
        let revision = s.ui.semantic_revision();
        if s.semantic_revision != Some(revision) {
            s.accessibility.update_if_active(|| {
                crate::accessibility::tree(
                    s.ui.semantics(),
                    &self.options.title,
                    s.window.scale_factor(),
                )
            });
            s.semantic_revision = Some(revision);
        }
        let caret = s.ui.ime_cursor();
        let target = caret.map(|_| s.ui.session());
        if target != self.ime_target {
            s.window.set_ime_allowed(false);
            self.ime_session = None;
            self.composing = false;
            self.ime_target = target;
            if target.is_some() {
                s.window.set_ime_allowed(true);
            }
        }
        if let Some(caret) = caret {
            s.window.set_ime_cursor_area(
                LogicalPosition::new(caret.x as f64, caret.y as f64),
                LogicalSize::new(caret.width as f64, caret.height as f64),
            );
        }
    }

    fn input(&mut self, input: Input) {
        if let Some(s) = &mut self.state {
            s.ui.advance(self.start.elapsed(), 128);
            s.ui.dispatch(input, &mut s.text);
        }
        self.service();
    }
    fn flush_pointer(&mut self) {
        if std::mem::take(&mut self.pending_pointer) {
            self.input(Input::Pointer {
                pointer: 0,
                position: self.pointer,
            })
        }
    }
}
impl<W: Widget, F: FnMut(W::Output, &WakeHandle<W>)> ApplicationHandler<HostEvent> for Host<W, F> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match create(
            event_loop,
            &self.options,
            self.root.take().unwrap(),
            self.wake.proxy.clone(),
        ) {
            Ok(state) => {
                self.state = Some(state);
                self.service();
                if let Some(s) = &self.state {
                    s.window.request_redraw()
                }
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit()
            }
        }
    }
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: HostEvent) {
        match event {
            HostEvent::Command(post) => self.pending_posts.push_back(Pending {
                command: *post
                    .command
                    .downcast::<W::Command>()
                    .expect("private native command type"),
                ticket: post.ticket.map(|ticket| {
                    *ticket
                        .downcast::<Ticket<W>>()
                        .expect("private native task type")
                }),
                bytes: post.bytes,
            }),
            HostEvent::Accessibility(event) => {
                if let Some(s) = &mut self.state {
                    if event.window_id != s.window.id() {
                        return;
                    }
                    match event.window_event {
                        accesskit_winit::WindowEvent::InitialTreeRequested => {
                            s.semantic_revision = None
                        }
                        accesskit_winit::WindowEvent::ActionRequested(request) => {
                            if let Some(action) = crate::accessibility::action(request.action) {
                                s.ui.accessibility(request.target_node.0, action);
                            }
                        }
                        accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                            s.semantic_revision = None
                        }
                    }
                }
            }
        }
        self.service();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let Some(s) = &mut self.state {
            s.accessibility.process_event(&s.window, &event);
        }
        if !matches!(event, WindowEvent::CursorMoved { .. }) {
            self.flush_pointer()
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.state.as_mut().is_none_or(|s| s.ui.request_close()) {
                    event_loop.exit();
                } else {
                    self.service();
                }
            }
            WindowEvent::ModifiersChanged(m) => self.modifiers = m.state(),
            WindowEvent::Focused(focus) => {
                if let Some(s) = &mut self.state {
                    s.ui.window_focus(focus)
                }
                self.service()
            }
            WindowEvent::Occluded(occluded) => {
                if let Some(s) = &mut self.state {
                    s.occluded = occluded;
                    s.ui.window_visible(!occluded);
                    if !occluded {
                        s.ui.repaint();
                        s.window.request_redraw()
                    }
                }
            }
            WindowEvent::Resized(size) => {
                if let Some(s) = &mut self.state {
                    if size.width > 0 && size.height > 0 {
                        let scale = s.window.scale_factor() as f32;
                        s.ui.resize(Size::new(
                            size.width as f32 / scale,
                            size.height as f32 / scale,
                        ));
                        s.resize_at = Some(Instant::now());
                        s.window.request_redraw()
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
                    s.window.request_redraw()
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self
                    .state
                    .as_ref()
                    .map_or(1., |s| s.window.scale_factor() as f32);
                self.pointer = Point::new(position.x as f32 / scale, position.y as f32 / scale);
                self.pending_pointer = true;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 3,
                    MouseButton::Back => 4,
                    MouseButton::Forward => 5,
                    MouseButton::Other(n) => n + 6,
                };
                self.input(Input::Button {
                    pointer: 0,
                    button,
                    down: state == ElementState::Pressed,
                    position: self.pointer,
                })
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self
                    .state
                    .as_ref()
                    .map_or(1., |s| s.window.scale_factor() as f32);
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => Point::new(x * 35., y * 35.),
                    MouseScrollDelta::PixelDelta(p) => {
                        Point::new(p.x as f32 / scale, p.y as f32 / scale)
                    }
                };
                self.input(Input::Scroll {
                    position: self.pointer,
                    delta,
                })
            }
            WindowEvent::Ime(Ime::Enabled) => self.ime_session = self.ime_target,
            WindowEvent::Ime(Ime::Disabled) => {
                self.ime_session = None;
                self.composing = false;
            }
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                if let Some(session) = self.ime_session {
                    self.composing = !text.is_empty();
                    self.input(Input::Preedit { session, text });
                }
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.composing = false;
                if let Some(session) = self.ime_session {
                    self.input(Input::Text { session, text });
                }
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                let modifiers = Modifiers {
                    shift: self.modifiers.shift_key(),
                    control: self.modifiers.control_key(),
                    alt: self.modifiers.alt_key(),
                    meta: self.modifiers.super_key(),
                };
                if let Some(key) = key(&event.logical_key) {
                    let physical = match event.physical_key {
                        PhysicalKey::Code(code) => code as u32,
                        PhysicalKey::Unidentified(_) => u32::MAX,
                    };
                    self.input(Input::Key {
                        key,
                        physical,
                        down: event.state == ElementState::Pressed,
                        repeat: event.repeat,
                        modifiers,
                    });
                }
                if event.state == ElementState::Pressed
                    && !modifiers.command()
                    && !modifiers.alt
                    && !self.composing
                {
                    if let Some(text) = event.text {
                        let text: String = text.chars().filter(|c| !c.is_control()).collect();
                        if !text.is_empty() {
                            let session = self.state.as_ref().map_or(0, |s| s.ui.session());
                            self.input(Input::Text { session, text })
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.service();
                if let Some(s) = &mut self.state {
                    if !s.occluded && Instant::now() >= self.next_frame {
                        let hz = s
                            .window
                            .current_monitor()
                            .and_then(|m| m.refresh_rate_millihertz())
                            .unwrap_or(60_000)
                            .max(1_000);
                        self.next_frame =
                            Instant::now() + Duration::from_secs_f64(1000. / hz as f64);
                        s.ui.frame(self.start.elapsed(), 256);
                    }
                }
                self.service();
                if let Some(s) = &mut self.state {
                    if !s.occluded {
                        if let Err(error) = render(s, self.options.background, self.profile) {
                            self.error = Some(error);
                            event_loop.exit();
                        }
                    }
                }
            }
            _ => {}
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.flush_pointer();
        self.service();
        if std::mem::take(&mut self.close_requested)
            && self.state.as_mut().is_none_or(|s| s.ui.request_close())
        {
            event_loop.exit();
            return;
        }
        let work = self
            .state
            .as_ref()
            .map(|s| s.ui.next_work())
            .unwrap_or_default();
        let visible = self.state.as_ref().is_some_and(|s| !s.occluded);
        if visible && (work.paint || (work.frame && Instant::now() >= self.next_frame)) {
            self.state.as_ref().unwrap().window.request_redraw()
        }
        let mut deadline = work.deadline.map(|d| self.start + d);
        if visible && work.frame {
            deadline = Some(deadline.map_or(self.next_frame, |d| d.min(self.next_frame)));
        }
        event_loop.set_control_flow(if work.ready || !self.pending_posts.is_empty() {
            ControlFlow::Poll
        } else if let Some(deadline) = deadline {
            ControlFlow::WaitUntil(deadline)
        } else {
            ControlFlow::Wait
        });
    }
}
fn key(key: &OsKey) -> Option<Key> {
    Some(match key {
        OsKey::Named(named) => match named {
            NamedKey::ArrowLeft => Key::Left,
            NamedKey::ArrowRight => Key::Right,
            NamedKey::ArrowUp => Key::Up,
            NamedKey::ArrowDown => Key::Down,
            NamedKey::Home => Key::Home,
            NamedKey::End => Key::End,
            NamedKey::PageUp => Key::PageUp,
            NamedKey::PageDown => Key::PageDown,
            NamedKey::Enter => Key::Enter,
            NamedKey::Escape => Key::Escape,
            NamedKey::Tab => Key::Tab,
            NamedKey::Backspace => Key::Backspace,
            NamedKey::Delete => Key::Delete,
            NamedKey::Space => Key::Character(' '),
            _ => return None,
        },
        OsKey::Character(text) => Key::Character(text.chars().next()?.to_ascii_lowercase()),
        _ => return None,
    })
}
fn create<W: Widget>(
    event_loop: &ActiveEventLoop,
    options: &WindowOptions,
    root: Element<W>,
    proxy: EventLoopProxy<HostEvent>,
) -> Result<State<W>, String> {
    let attrs = Window::default_attributes()
        .with_visible(false)
        .with_title(&options.title)
        .with_inner_size(LogicalSize::new(options.size.width, options.size.height))
        .with_min_inner_size(LogicalSize::new(420., 360.));
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
    let window = window.ok_or("Window creation failed")?;
    let accessibility = accesskit_winit::Adapter::with_event_loop_proxy(event_loop, &window, proxy);
    window.set_visible(true);
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
    let _ = surface.set_swap_interval(&context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));
    let renderer = unsafe { OpenGl::new_from_function_cstr(|name| display.get_proc_address(name)) }
        .map_err(|e| e.to_string())?;
    let text = NativeText::new()?;
    let canvas =
        Canvas::new_with_text_context(renderer, text.context.clone()).map_err(|e| e.to_string())?;
    let ui = Ui::new(root, options.size, options.limits)
        .map_err(|e| format!("UI admission failed: {e:?}"))?;
    Ok(State {
        accessibility,
        semantic_revision: None,
        canvas,
        text,
        ui,
        surface,
        context,
        window,
        surface_size: (size.width, size.height),
        occluded: false,
        resize_at: None,
    })
}
fn render<W: Widget>(s: &mut State<W>, background: Color, profile: bool) -> Result<(), String> {
    let size = s.window.inner_size();
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    if s.surface_size != (size.width, size.height) {
        s.surface.resize(
            &s.context,
            NonZeroU32::new(size.width).unwrap(),
            NonZeroU32::new(size.height).unwrap(),
        );
        s.surface_size = (size.width, size.height)
    }
    s.canvas.set_size(size.width, size.height, 1.);
    s.canvas.reset();
    s.canvas.clear_rect(
        0,
        0,
        size.width,
        size.height,
        femtovg::Color::rgbaf(background.0, background.1, background.2, background.3),
    );
    s.canvas.scale(
        s.window.scale_factor() as f32,
        s.window.scale_factor() as f32,
    );
    s.ui.paint(&mut GlPainter::new(
        &mut s.canvas,
        &s.text,
        Rect::new(0., 0., size.width as f32, size.height as f32),
    ));
    s.canvas.flush();
    s.surface
        .swap_buffers(&s.context)
        .map_err(|e| e.to_string())?;
    if let Some(start) = s.resize_at.take() {
        if profile {
            eprintln!("resize_frame_us={}", start.elapsed().as_micros())
        }
    }
    Ok(())
}
