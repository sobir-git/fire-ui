//! Core focus events retain real focus changes made during a keyboard grab.
//! Winit's XI2 focus bookkeeping can suppress those changes after NotifyGrab.
use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

pub(crate) struct Focus {
    connection: RustConnection,
    window: u32,
    focused: bool,
    poller: Arc<polling::Poller>,
    stopped: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Focus {
    pub(crate) fn new(window: &Window, wake: impl Fn() -> bool + Send + 'static) -> Result<Self> {
        let (connection, _, window) = connection(window)?;
        Self::observe(connection, window, wake)
    }

    fn observe(
        connection: RustConnection,
        window: u32,
        wake: impl Fn() -> bool + Send + 'static,
    ) -> Result<Self> {
        connection
            .change_window_attributes(
                window,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::FOCUS_CHANGE),
            )
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| e.to_string())?;
        // Subscribe while the newly created window is still unmapped. Its initial
        // focus is false; all subsequent transitions arrive on this connection.
        let poller = Arc::new(polling::Poller::new().map_err(|e| e.to_string())?);
        // SAFETY: Drop removes the stream before the connection is dropped.
        unsafe { poller.add(connection.stream(), polling::Event::readable(0)) }
            .map_err(|e| e.to_string())?;
        let stopped = Arc::new(AtomicBool::new(false));
        let mut result = Self {
            connection,
            window,
            focused: false,
            poller: poller.clone(),
            stopped: stopped.clone(),
            thread: None,
        };
        result.thread = Some(
            std::thread::Builder::new()
                .name("fire-ui-focus".into())
                .stack_size(128 * 1024)
                .spawn(move || {
                    let mut events = polling::Events::new();
                    while !stopped.load(Ordering::Acquire) {
                        events.clear();
                        let failed = poller.wait(&mut events, None).is_err();
                        if stopped.load(Ordering::Acquire) {
                            break;
                        }
                        if failed {
                            stopped.store(true, Ordering::Release);
                        }
                        if (failed || !events.is_empty()) && !wake() {
                            break;
                        }
                        if failed {
                            break;
                        }
                    }
                })
                .map_err(|e| e.to_string())?,
        );
        Ok(result)
    }

    pub(crate) fn drain(&mut self, synchronize: bool) -> Result<Vec<bool>> {
        if self.stopped.load(Ordering::Acquire) {
            return Err("focus readiness listener stopped".into());
        }
        if synchronize {
            // Winit's focus callback must see the core events already generated
            // by the server before it dispatches immediate keyboard input.
            self.connection
                .get_input_focus()
                .map_err(|e| e.to_string())?
                .reply()
                .map_err(|e| e.to_string())?;
        }
        let mut changes = Vec::new();
        while let Some(event) = self
            .connection
            .poll_for_event()
            .map_err(|e| e.to_string())?
        {
            let (event, gained) = match event {
                Event::FocusIn(event) => (event, true),
                Event::FocusOut(event) => (event, false),
                _ => continue,
            };
            if event.event != self.window
                || matches!(event.mode, NotifyMode::GRAB | NotifyMode::UNGRAB)
                || matches!(event.detail, NotifyDetail::INFERIOR | NotifyDetail::POINTER)
            {
                continue;
            }
            let focused = if matches!(
                event.detail,
                NotifyDetail::POINTER_ROOT | NotifyDetail::NONE
            ) {
                input_focus_within(&self.connection, self.window)?
            } else {
                gained
            };
            if self.focused != focused {
                self.focused = focused;
                changes.push(focused);
            }
        }
        self.poller
            .modify(self.connection.stream(), polling::Event::readable(0))
            .map_err(|e| e.to_string())?;
        Ok(changes)
    }
}

impl Drop for Focus {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        let _ = self.poller.notify();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = self.poller.delete(self.connection.stream());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated X11 server (run with xvfb-run)"]
    fn ordered_focus_survives_grabs_and_descendants_without_idle_wakes() {
        let (control, screen) = x11rb::connect(None).unwrap();
        let root = control.setup().roots[screen].root;
        let [window, child, sibling, other] =
            std::array::from_fn(|_| control.generate_id().unwrap());
        for (id, parent) in [
            (window, root),
            (child, window),
            (sibling, window),
            (other, root),
        ] {
            control
                .create_window(
                    x11rb::COPY_DEPTH_FROM_PARENT,
                    id,
                    parent,
                    0,
                    0,
                    100,
                    100,
                    0,
                    WindowClass::INPUT_OUTPUT,
                    x11rb::COPY_FROM_PARENT,
                    &CreateWindowAux::new().override_redirect(1),
                )
                .unwrap()
                .check()
                .unwrap();
        }
        let (connection, _) = x11rb::connect(None).unwrap();
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut focus =
            Focus::observe(connection, window, move || sender.send(()).is_ok()).unwrap();
        for id in [window, child, sibling, other] {
            control.map_window(id).unwrap().check().unwrap();
        }
        let set = |id| {
            control
                .set_input_focus(InputFocus::PARENT, id, x11rb::CURRENT_TIME)
                .unwrap()
                .check()
                .unwrap();
        };
        set(window);
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(focus.drain(true).unwrap(), [true]);
        set(child);
        set(sibling);
        set(window);
        assert!(
            focus.drain(true).unwrap().is_empty(),
            "descendants retain window focus"
        );
        assert_eq!(
            control
                .grab_keyboard(
                    false,
                    root,
                    x11rb::CURRENT_TIME,
                    GrabMode::ASYNC,
                    GrabMode::ASYNC
                )
                .unwrap()
                .reply()
                .unwrap()
                .status,
            GrabStatus::SUCCESS
        );
        assert!(
            focus.drain(true).unwrap().is_empty(),
            "grab is not focus loss"
        );
        set(other);
        assert_eq!(
            focus.drain(true).unwrap(),
            [false],
            "real loss while grabbed must arrive"
        );
        control
            .ungrab_keyboard(x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        assert!(focus.drain(true).unwrap().is_empty());
        set(window);
        set(other);
        set(window);
        assert_eq!(
            focus.drain(true).unwrap(),
            [true, false, true],
            "do not collapse rapid loss and return"
        );
        control.unmap_window(window).unwrap().check().unwrap();
        assert_eq!(focus.drain(true).unwrap(), [false]);
        control.map_window(window).unwrap().check().unwrap();
        set(window);
        assert_eq!(focus.drain(true).unwrap(), [true]);
        // Let a readiness notification already in flight finish, then check that
        // a quiescent connection causes no periodic wakeups.
        std::thread::sleep(std::time::Duration::from_millis(30));
        while receiver.try_recv().is_ok() {}
        assert!(receiver
            .recv_timeout(std::time::Duration::from_millis(100))
            .is_err());
        drop(focus); // Must join even while the window still exists and the reader sleeps.
        assert!(matches!(
            receiver.recv_timeout(std::time::Duration::from_secs(1)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
        control.destroy_window(window).unwrap().check().unwrap();
        control.destroy_window(other).unwrap().check().unwrap();
    }
}
