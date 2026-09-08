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
        atomic::{AtomicBool, AtomicUsize, Ordering},
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
    pub decorations: bool,
    /// Passive, click-through window above normal windows. On Linux this requires X11/XWayland.
    pub overlay: bool,
    pub min_size: Size,
    pub position: Option<(i32, i32)>,
    /// Primary font file. None loads no font; text requires an explicit face.
    pub font: Option<std::path::PathBuf>,
    /// Ordered fallback faces for characters missing from the primary font.
    pub fallback_fonts: Vec<std::path::PathBuf>,
    /// Retain a window image to repaint only damage, trading graphics memory for CPU.
    pub partial_repaint: bool,
    pub size: Size,
    pub background: Color,
    pub limits: Limits,
}
impl Default for WindowOptions {
    fn default() -> Self {
        Self {
            title: "Fire UI".into(),
            decorations: true,
            overlay: false,
            min_size: Size::new(420., 360.),
            position: None,
            font: None,
            fallback_fonts: vec![],
            partial_repaint: false,
            size: Size::new(1100., 780.),
            background: Color::hex(0x171918),
            limits: Limits::default(),
        }
    }
}
enum HostEvent {
    #[cfg(all(unix, feature = "inspection"))]
    Inspect(crate::inspection::Pending),
    Command(Posted),
    #[cfg(feature = "accessibility")]
    Accessibility(accesskit_winit::Event),
    #[cfg(all(target_os = "linux", feature = "accessibility"))]
    LinuxEdit(crate::linux_atspi::PendingEdit),
    #[cfg(all(target_os = "linux", feature = "accessibility", feature = "clipboard"))]
    LinuxPaste {
        pending: crate::linux_atspi::PendingEdit,
        text: Result<String, String>,
        expected: String,
    },
}
#[cfg(all(target_os = "linux", feature = "accessibility"))]
impl From<crate::linux_atspi::PendingEdit> for HostEvent {
    fn from(pending: crate::linux_atspi::PendingEdit) -> Self {
        Self::LinuxEdit(pending)
    }
}
#[cfg(feature = "accessibility")]
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
    alive: Arc<AtomicBool>,
    count: Arc<AtomicUsize>,
    bytes: Arc<AtomicUsize>,
}
impl<W: Widget> Clone for WakeHandle<W> {
    fn clone(&self) -> Self {
        Self {
            proxy: self.proxy.clone(),
            marker: std::marker::PhantomData,
            alive: self.alive.clone(),
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
        if !self.alive.load(Ordering::Acquire) {
            return Err((Error::Stale, command));
        }
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
                #[cfg(any(feature = "accessibility", all(unix, feature = "inspection")))]
                let HostEvent::Command(post) = error.0
                else {
                    unreachable!()
                };
                #[cfg(not(any(feature = "accessibility", all(unix, feature = "inspection"))))]
                let HostEvent::Command(post) = error.0;
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
    #[cfg(feature = "accessibility")]
    accessibility: crate::platform_accessibility::Adapter,
    #[cfg(feature = "accessibility")]
    accessibility_tree: crate::accessibility::AccessibilityTree,
    #[cfg(feature = "accessibility")]
    semantic_revision: Option<u64>,
    canvas: Canvas<OpenGl>,
    backing: Option<femtovg::ImageId>,
    partial_repaint: bool,
    presenter: Option<crate::present::Presenter>,
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
    #[cfg(feature = "clipboard")]
    clipboard: Option<arboard::Clipboard>,
    cursor: Option<CursorIcon>,
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
        alive: Arc::new(AtomicBool::new(true)),
        count: Arc::new(AtomicUsize::new(0)),
        bytes: Arc::new(AtomicUsize::new(0)),
    };
    #[cfg(all(unix, feature = "inspection"))]
    let _inspection = if let Some(path) = std::env::var_os("FIRE_UI_INSPECT") {
        let proxy = wake.proxy.clone();
        Some(crate::inspection::Server::start(
            path.into(),
            move |request| proxy.send_event(HostEvent::Inspect(request)).is_ok(),
        )?)
    } else {
        None
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
        #[cfg(feature = "clipboard")]
        clipboard: None,
        cursor: None,
        wake,
        output,
        error: None,
        close_requested: false,
        profile: std::env::var_os("FIRE_UI_PROFILE").is_some(),
        pending_posts: std::collections::VecDeque::new(),
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let result = event_loop.run_app(&mut host);
    host.wake.alive.store(false, Ordering::Release);
    result.map_err(|e| e.to_string())?;
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
            #[cfg(feature = "clipboard")]
            if matches!(request, HostRequest::Copy(_) | HostRequest::Paste(_))
                && self.clipboard.is_none()
            {
                self.clipboard = arboard::Clipboard::new().ok()
            }
            match request {
                HostRequest::Close => self.close_requested = true,
                HostRequest::Window(action) => match action {
                    WindowAction::Focus => s.window.focus_window(),
                    WindowAction::Restore => {
                        s.window.set_minimized(false);
                        s.window.set_maximized(false);
                    }
                    WindowAction::Fullscreen(enabled) => s.window.set_fullscreen(
                        enabled.then_some(winit::window::Fullscreen::Borderless(None)),
                    ),
                    WindowAction::AlwaysOnTop(enabled) => s.window.set_window_level(if enabled {
                        winit::window::WindowLevel::AlwaysOnTop
                    } else {
                        winit::window::WindowLevel::Normal
                    }),
                    WindowAction::Minimize => s.window.set_minimized(true),
                    WindowAction::ToggleMaximized => {
                        s.window.set_maximized(!s.window.is_maximized())
                    }
                    WindowAction::Drag => {
                        let _ = s.window.drag_window();
                    }
                    WindowAction::Resize(edge) => {
                        use winit::window::ResizeDirection as R;
                        let direction = match edge {
                            ResizeEdge::North => R::North,
                            ResizeEdge::South => R::South,
                            ResizeEdge::East => R::East,
                            ResizeEdge::West => R::West,
                            ResizeEdge::NorthEast => R::NorthEast,
                            ResizeEdge::NorthWest => R::NorthWest,
                            ResizeEdge::SouthEast => R::SouthEast,
                            ResizeEdge::SouthWest => R::SouthWest,
                        };
                        let _ = s.window.drag_resize_window(direction);
                    }
                },
                #[cfg(feature = "clipboard")]
                HostRequest::Copy(text) => {
                    if let Some(clipboard) = &mut self.clipboard {
                        let _ = clipboard.set_text(text);
                    }
                }
                #[cfg(feature = "clipboard")]
                HostRequest::Paste(token) => {
                    if let Some(clipboard) = &mut self.clipboard {
                        if let Ok(text) = clipboard.get_text() {
                            s.ui.paste(token, text)
                        }
                    }
                }
                #[cfg(not(feature = "clipboard"))]
                HostRequest::Copy(_) | HostRequest::Paste(_) => {}
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
        let edge = if !self.options.decorations {
            resize_edge(self.pointer, s.ui.size())
        } else {
            None
        };
        let cursor = edge
            .map(CursorIcon::Resize)
            .unwrap_or_else(|| s.ui.cursor(self.pointer));
        if self.cursor != Some(cursor) {
            use winit::window::CursorIcon as NativeCursor;
            s.window.set_cursor(match cursor {
                CursorIcon::Arrow => NativeCursor::Default,
                CursorIcon::Text => NativeCursor::Text,
                CursorIcon::Pointer => NativeCursor::Pointer,
                CursorIcon::Resize(edge) => match edge {
                    ResizeEdge::North | ResizeEdge::South => NativeCursor::NsResize,
                    ResizeEdge::East | ResizeEdge::West => NativeCursor::EwResize,
                    ResizeEdge::NorthEast | ResizeEdge::SouthWest => NativeCursor::NeswResize,
                    ResizeEdge::NorthWest | ResizeEdge::SouthEast => NativeCursor::NwseResize,
                },
            });
            self.cursor = Some(cursor);
        }

        #[cfg(feature = "accessibility")]
        {
            let revision = s.ui.semantic_revision();
            if s.semantic_revision != Some(revision) {
                s.accessibility.update_if_active(|| {
                    s.accessibility_tree.tree(
                        s.ui.semantics(),
                        &self.options.title,
                        s.window.scale_factor(),
                    )
                });
                s.semantic_revision = Some(revision);
            }
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
            // winit 0.30's X11 backend ignores the area size and forwards only
            // the spot. XIM needs the bottom of the line to avoid covering preedit.
            let y = if matches!(
                s.window.window_handle().map(|h| h.as_raw()),
                Ok(raw_window_handle::RawWindowHandle::Xlib(_)
                    | raw_window_handle::RawWindowHandle::Xcb(_))
            ) {
                caret.y + caret.height
            } else {
                caret.y
            };
            s.window.set_ime_cursor_area(
                LogicalPosition::new(caret.x as f64, y as f64),
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
            #[cfg(feature = "accessibility")]
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
            #[cfg(all(unix, feature = "inspection"))]
            HostEvent::Inspect(pending) => {
                if Instant::now() > pending.deadline {
                    let _ = pending
                        .reply
                        .send(serde_json::json!({"error": "request expired"}));
                    return;
                }
                self.service();
                let result = self
                    .state
                    .as_mut()
                    .ok_or("window unavailable".to_string())
                    .and_then(|s| crate::inspection::apply(&mut s.ui, &pending.request));
                for _ in 0..16 {
                    self.service();
                    if self.state.as_ref().is_none_or(|s| !s.ui.next_work().ready) {
                        break;
                    }
                }
                let response = match result {
                    Ok(()) => crate::inspection::snapshot(&self.state.as_ref().unwrap().ui),
                    Err(error) => serde_json::json!({"error": error}),
                };
                let _ = pending.reply.send(response);
            }
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
            #[cfg(all(target_os = "linux", feature = "accessibility"))]
            HostEvent::LinuxEdit(pending) => {
                self.service();
                if let crate::linux_atspi::Operation::Paste {
                    position: _position,
                } = pending.operation
                {
                    #[cfg(not(feature = "clipboard"))]
                    let _ = pending
                        .reply
                        .try_send(Err("native clipboard feature is disabled".into()));
                    #[cfg(feature = "clipboard")]
                    {
                        let prepared = self
                            .state
                            .as_ref()
                            .ok_or("window unavailable".to_string())
                            .and_then(|s| crate::linux_edit::prepare_paste(&s.ui, &pending));
                        match prepared {
                            Ok(expected) => {
                                let proxy = self.wake.proxy.clone();
                                // An unresponsive clipboard owner can take seconds. Keep
                                // pointer events and paints running while waiting for it.
                                std::thread::spawn(move || {
                                    let text = arboard::Clipboard::new()
                                        .and_then(|mut c| c.get_text())
                                        .map_err(|e| e.to_string());
                                    let _ = proxy.send_event(HostEvent::LinuxPaste {
                                        pending,
                                        text,
                                        expected,
                                    });
                                });
                            }
                            Err(error) => {
                                let _ = pending.reply.try_send(Err(error));
                            }
                        }
                    }
                } else {
                    let result = self
                        .state
                        .as_mut()
                        .ok_or("window unavailable".to_string())
                        .and_then(|s| {
                            crate::linux_edit::apply(&mut s.ui, &pending, |text| {
                                #[cfg(feature = "clipboard")]
                                {
                                    copy_text(&mut self.clipboard, text)
                                }
                                #[cfg(not(feature = "clipboard"))]
                                {
                                    let _ = text;
                                    Err("native clipboard feature is disabled".into())
                                }
                            })
                        });
                    self.service();
                    let _ = pending.reply.try_send(result);
                }
            }
            #[cfg(all(target_os = "linux", feature = "accessibility", feature = "clipboard"))]
            HostEvent::LinuxPaste {
                mut pending,
                text,
                expected,
            } => {
                self.service();
                let result = text.and_then(|text| {
                    let s = self.state.as_mut().ok_or("window unavailable")?;
                    if crate::linux_edit::prepare_paste(&s.ui, &pending)? != expected {
                        return Err("text changed while reading clipboard".into());
                    }
                    let crate::linux_atspi::Operation::Paste { position } = pending.operation
                    else {
                        unreachable!()
                    };
                    pending.operation = crate::linux_atspi::Operation::Insert {
                        position,
                        length: -1,
                        text,
                    };
                    crate::linux_edit::apply(&mut s.ui, &pending, |text| {
                        copy_text(&mut self.clipboard, text)
                    })
                });
                self.service();
                let _ = pending.reply.try_send(result);
            }
            #[cfg(feature = "accessibility")]
            HostEvent::Accessibility(event) => {
                if let Some(s) = &mut self.state {
                    if event.window_id != s.window.id() {
                        return;
                    }
                    match event.window_event {
                        accesskit_winit::WindowEvent::InitialTreeRequested => {
                            s.semantic_revision = None;
                            s.accessibility_tree.reset();
                        }
                        accesskit_winit::WindowEvent::ActionRequested(request) => {
                            if let Some(action) = s
                                .accessibility_tree
                                .action(request.clone(), &s.ui.semantics())
                            {
                                let _ = s.ui.accessibility(request.target_node.0, action);
                            }
                        }
                        accesskit_winit::WindowEvent::AccessibilityDeactivated => {
                            s.semantic_revision = None;
                            s.accessibility_tree.reset();
                        }
                    }
                }
            }
        }
        self.service();
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(feature = "accessibility")]
        if let Some(s) = &mut self.state {
            s.accessibility.process_event(&s.window, &event);
        }
        if !matches!(event, WindowEvent::CursorMoved { .. }) {
            self.flush_pointer()
        }
        match event {
            WindowEvent::CloseRequested => {
                if self.state.as_mut().is_none_or(|s| s.ui.request_close()) {
                    self.service();
                    event_loop.exit();
                } else {
                    self.service();
                }
            }
            WindowEvent::Moved(position) => {
                if let Some(s) = &mut self.state {
                    s.ui.window_position(position.x, position.y);
                }
                self.service();
            }
            WindowEvent::DroppedFile(path) => self.input(Input::FileDropped(path)),
            WindowEvent::ModifiersChanged(m) => {
                self.modifiers = m.state();
                self.input(Input::Modifiers(Modifiers {
                    shift: self.modifiers.shift_key(),
                    control: self.modifiers.control_key(),
                    alt: self.modifiers.alt_key(),
                    meta: self.modifiers.super_key(),
                }));
            }
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
                if let Some(session) = self.ime_session.take() {
                    self.input(Input::Preedit {
                        session,
                        text: String::new(),
                        selection: None,
                    });
                }
                self.composing = false;
            }
            WindowEvent::Ime(Ime::Preedit(text, selection)) => {
                if let Some(session) = self.ime_session {
                    self.composing = !text.is_empty();
                    self.input(Input::Preedit {
                        session,
                        text,
                        selection,
                    });
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
                if !self.composing {
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
            self.service();
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
    #[cfg(feature = "accessibility")] proxy: EventLoopProxy<HostEvent>,
) -> Result<State<W>, String> {
    let attrs = Window::default_attributes()
        .with_visible(false)
        .with_decorations(options.decorations && !options.overlay)
        .with_active(!options.overlay)
        .with_window_level(if options.overlay {
            winit::window::WindowLevel::AlwaysOnTop
        } else {
            winit::window::WindowLevel::Normal
        })
        .with_title(&options.title)
        .with_inner_size(LogicalSize::new(options.size.width, options.size.height))
        .with_min_inner_size(LogicalSize::new(
            options.min_size.width,
            options.min_size.height,
        ));
    #[cfg(all(target_os = "linux", feature = "x11"))]
    let attrs = if options.overlay {
        use winit::platform::x11::{WindowAttributesExtX11, WindowType};
        attrs
            .with_override_redirect(true)
            .with_x11_window_type(vec![WindowType::Notification])
    } else {
        attrs
    };
    let attrs = if let Some((x, y)) = options.position {
        attrs.with_position(winit::dpi::PhysicalPosition::new(x, y))
    } else {
        attrs
    };
    let (window, config) = DisplayBuilder::new()
        .with_window_attributes(Some(attrs))
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
    #[cfg(feature = "accessibility")]
    let accessibility =
        crate::platform_accessibility::Adapter::with_event_loop_proxy(event_loop, &window, proxy);
    if options.overlay {
        if matches!(
            window.window_handle().map_err(|e| e.to_string())?.as_raw(),
            raw_window_handle::RawWindowHandle::Wayland(_)
        ) {
            return Err("Passive overlays require X11/XWayland on Linux".into());
        }
        window
            .set_cursor_hittest(false)
            .map_err(|e| e.to_string())?;
    }
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
    let mut text = if let Some(path) = &options.font {
        NativeText::with_font(path)?
    } else {
        NativeText::empty()
    };
    for path in &options.fallback_fonts {
        text.add_font(path)?;
    }
    let canvas =
        Canvas::new_with_text_context(renderer, text.context.clone()).map_err(|e| e.to_string())?;
    let mut ui = Ui::new(root, options.size, options.limits)
        .map_err(|e| format!("UI admission failed: {e:?}"))?;
    ui.set_clipboard_enabled(cfg!(feature = "clipboard"));
    Ok(State {
        #[cfg(feature = "accessibility")]
        accessibility,
        #[cfg(feature = "accessibility")]
        accessibility_tree: Default::default(),
        #[cfg(feature = "accessibility")]
        semantic_revision: None,
        canvas,
        backing: None,
        partial_repaint: options.partial_repaint,
        presenter: if options.partial_repaint {
            unsafe { crate::present::Presenter::new(|name| display.get_proc_address(name)) }
        } else {
            None
        },
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
    if s.text.fonts.is_empty() && s.text.layouts != 0 {
        return Err("Text requires a font; set WindowOptions::font or fallback_fonts".into());
    }
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
        s.surface_size = (size.width, size.height);
        if let Some(image) = s.backing.take() {
            s.canvas.delete_image(image);
        }
        s.ui.repaint();
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
            s.ui.repaint();
        }
        let backing = s.backing.unwrap();
        let scale = s.window.scale_factor() as f32;
        if let Some(damage) = s.ui.paint_damage() {
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
            s.ui.paint_region(
                &mut GlPainter::new(
                    &mut s.canvas,
                    &s.text,
                    Rect::new(0., 0., size.width as f32, size.height as f32),
                ),
                region,
            );
            if profile {
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
        let scale = s.window.scale_factor() as f32;
        s.canvas.scale(scale, scale);
        s.ui.paint(&mut GlPainter::new(
            &mut s.canvas,
            &s.text,
            Rect::new(0., 0., size.width as f32, size.height as f32),
        ));
        s.canvas.flush();
        if profile {
            eprintln!(
                "paint_area_pixels={}",
                size.width as u64 * size.height as u64
            );
        }
    }
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

fn resize_edge(p: Point, size: Size) -> Option<ResizeEdge> {
    let left = p.x < 4.;
    let right = p.x >= size.width - 4.;
    let top = p.y < 4.;
    let bottom = p.y >= size.height - 4.;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeEdge::NorthWest),
        (_, true, true, _) => Some(ResizeEdge::NorthEast),
        (true, _, _, true) => Some(ResizeEdge::SouthWest),
        (_, true, _, true) => Some(ResizeEdge::SouthEast),
        (true, _, _, _) => Some(ResizeEdge::West),
        (_, true, _, _) => Some(ResizeEdge::East),
        (_, _, true, _) => Some(ResizeEdge::North),
        (_, _, _, true) => Some(ResizeEdge::South),
        _ => None,
    }
}

#[cfg(all(target_os = "linux", feature = "accessibility", feature = "clipboard"))]
fn copy_text(slot: &mut Option<arboard::Clipboard>, text: &str) -> Result<(), String> {
    if slot.is_none() {
        *slot = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    slot.as_mut()
        .unwrap()
        .set_text(text)
        .map_err(|e| e.to_string())
}
