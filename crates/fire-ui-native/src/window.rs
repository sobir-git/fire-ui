use crate::{Renderer, RendererFactory};
use fire_ui::*;
use raw_window_handle::HasWindowHandle;
use std::{
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
    /// Request a native window whose pixels preserve alpha transparency.
    /// Support depends on the platform compositor and renderer surface.
    pub transparent: bool,
    /// Override pointer pass-through. `None` preserves the historical behavior:
    /// overlays are click-through and ordinary windows are interactive.
    pub click_through: Option<bool>,
    pub min_size: Size,
    pub position: Option<(i32, i32)>,
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
            transparent: false,
            click_through: None,
            min_size: Size::new(420., 360.),
            position: None,
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
// Drop drawing and text resources before their native window.
struct State<W: Widget> {
    #[cfg(feature = "accessibility")]
    accessibility: crate::platform_accessibility::Adapter,
    #[cfg(feature = "accessibility")]
    accessibility_tree: crate::accessibility::AccessibilityTree,
    #[cfg(feature = "accessibility")]
    semantic_revision: Option<u64>,
    renderer: Box<dyn Renderer>,
    text: Box<dyn TextEngine>,
    ui: Ui<W>,
    window: Arc<Window>,
    occluded: bool,
    resize_at: Option<Instant>,
}
struct Host<W: Widget, F, R> {
    factory: Option<R>,
    text: Option<Box<dyn TextEngine>>,
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
pub fn run<W: Widget>(
    root: Element<W>,
    options: WindowOptions,
    text: impl TextEngine + 'static,
    renderer: impl RendererFactory,
) -> Result<(), String> {
    run_with(root, options, text, renderer, |_, _| {})
}
pub fn run_with<W: Widget, F: FnMut(W::Output, &WakeHandle<W>) + 'static>(
    root: Element<W>,
    options: WindowOptions,
    text: impl TextEngine + 'static,
    renderer: impl RendererFactory,
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
        factory: Some(renderer),
        text: Some(Box::new(text)),
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
impl<W: Widget, F: FnMut(W::Output, &WakeHandle<W>), R: RendererFactory> Host<W, F, R> {
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
        #[cfg(all(feature = "accessibility", target_os = "linux"))]
        s.accessibility
            .synchronize(s.ui.layout_pending(s.text.as_ref()), |active| {
                s.ui.layout(s.text.as_mut());
                let revision = s.ui.semantic_revision();
                if !active || s.semantic_revision == Some(revision) {
                    return None;
                }
                let started = Instant::now();
                let source = s.ui.semantics();
                let snapshots =
                    crate::linux_atspi::text::snapshots(&source, s.window.scale_factor());
                let tree =
                    s.accessibility_tree
                        .tree(source, &self.options.title, s.window.scale_factor());
                if self.profile {
                    eprintln!("accessibility_update_us={}", started.elapsed().as_micros());
                }
                s.semantic_revision = Some(revision);
                Some((tree, snapshots))
            });
        #[cfg(not(all(feature = "accessibility", target_os = "linux")))]
        s.ui.layout(s.text.as_mut());
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

        #[cfg(all(feature = "accessibility", not(target_os = "linux")))]
        {
            let revision = s.ui.semantic_revision();
            if s.semantic_revision != Some(revision) {
                let started = Instant::now();
                s.accessibility.update_if_active(|| {
                    s.accessibility_tree.tree(
                        s.ui.semantics(),
                        &self.options.title,
                        s.window.scale_factor(),
                    )
                });
                if self.profile {
                    eprintln!("accessibility_update_us={}", started.elapsed().as_micros());
                }
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
            s.ui.dispatch(input, s.text.as_mut());
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
impl<W: Widget, F: FnMut(W::Output, &WakeHandle<W>), R: RendererFactory>
    ApplicationHandler<HostEvent> for Host<W, F, R>
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match create(
            event_loop,
            &self.options,
            self.root.take().unwrap(),
            self.text.take().unwrap(),
            self.factory.take().unwrap(),
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
                    pending.complete(Err("native clipboard feature is disabled".into()));
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
                                pending.complete(Err(error));
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
                    pending.complete(result);
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
                pending.complete(result);
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
    text: Box<dyn TextEngine>,
    factory: impl RendererFactory,
    #[cfg(feature = "accessibility")] proxy: EventLoopProxy<HostEvent>,
) -> Result<State<W>, String> {
    let attrs = Window::default_attributes()
        .with_visible(false)
        .with_transparent(options.transparent)
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
        let interactive = options.click_through == Some(false);
        attrs
            .with_override_redirect(!interactive)
            .with_x11_window_type(vec![if interactive {
                WindowType::Utility
            } else {
                WindowType::Notification
            }])
    } else {
        attrs
    };
    let attrs = if let Some((x, y)) = options.position {
        attrs.with_position(winit::dpi::PhysicalPosition::new(x, y))
    } else {
        attrs
    };
    let (window, renderer) = factory.create(event_loop, attrs)?;
    #[cfg(feature = "accessibility")]
    let accessibility =
        crate::platform_accessibility::Adapter::with_event_loop_proxy(event_loop, &window, proxy);
    if options.overlay
        && matches!(
            window.window_handle().map_err(|e| e.to_string())?.as_raw(),
            raw_window_handle::RawWindowHandle::Wayland(_)
        )
    {
        return Err("Passive overlays require X11/XWayland on Linux".into());
    }
    window.set_visible(true);
    if options.overlay || options.click_through.is_some() {
        window
            .set_cursor_hittest(!options.click_through.unwrap_or(options.overlay))
            .map_err(|e| e.to_string())?;
    }
    let mut ui = Ui::new(root, options.size, options.limits)
        .map_err(|e| format!("UI admission failed: {e:?}"))?;
    ui.set_clipboard_enabled(cfg!(feature = "clipboard"));
    Ok(State {
        #[cfg(feature = "accessibility")]
        accessibility,
        #[cfg(feature = "accessibility")]
        accessibility_tree: crate::accessibility::AccessibilityTree::new(!cfg!(
            target_os = "linux"
        )),
        #[cfg(feature = "accessibility")]
        semantic_revision: None,
        renderer,
        text,
        ui,
        window,
        occluded: false,
        resize_at: None,
    })
}
fn render<W: Widget>(s: &mut State<W>, background: Color, profile: bool) -> Result<(), String> {
    let damage = s.ui.paint_damage();
    s.renderer
        .render(&s.window, background, damage, &mut |painter, region| {
            if let Some(region) = region {
                s.ui.paint_region(painter, region);
            } else {
                s.ui.paint(painter);
            }
        })?;
    if let Some(start) = s.resize_at.take() {
        if profile {
            eprintln!("resize_frame_us={}", start.elapsed().as_micros());
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

#[cfg(test)]
mod tests {
    use super::WindowOptions;

    #[test]
    fn window_capability_defaults_preserve_existing_behavior() {
        let options = WindowOptions::default();
        assert!(!options.transparent);
        assert_eq!(options.click_through, None);
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
