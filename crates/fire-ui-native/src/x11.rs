//! Focus queries and EWMH integration, kept private to the X11 host.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

type Result<T> = std::result::Result<T, String>;
mod focus;
pub(crate) use focus::Focus;
fn connection(window: &Window) -> Result<(RustConnection, usize, u32)> {
    let id = match window.window_handle().map_err(|e| e.to_string())?.as_raw() {
        RawWindowHandle::Xlib(h) => h.window as u32,
        RawWindowHandle::Xcb(h) => h.window.get(),
        _ => return Err("EWMH requires X11".into()),
    };
    let (c, screen) = x11rb::connect(None).map_err(|e| e.to_string())?;
    Ok((c, screen, id))
}
fn atom(c: &RustConnection, name: &[u8]) -> Result<u32> {
    c.intern_atom(false, name)
        .map_err(|e| e.to_string())?
        .reply()
        .map(|r| r.atom)
        .map_err(|e| e.to_string())
}
pub(crate) fn launcher(window: &Window) -> Result<()> {
    let (c, _, id) = connection(window)?;
    let state = atom(&c, b"_NET_WM_STATE")?;
    let states = [
        atom(&c, b"_NET_WM_STATE_SKIP_TASKBAR")?,
        atom(&c, b"_NET_WM_STATE_SKIP_PAGER")?,
        atom(&c, b"_NET_WM_STATE_ABOVE")?,
    ];
    c.change_property32(PropMode::REPLACE, id, state, AtomEnum::ATOM, &states)
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    c.flush().map_err(|e| e.to_string())
}
pub(crate) fn activate(window: &Window, launcher: bool) -> Result<()> {
    let (c, screen, id) = connection(window)?;
    activate_window(&c, c.setup().roots[screen].root, id, launcher)
}

fn add_window_state(c: &RustConnection, root: u32, id: u32, states: [u32; 2]) -> Result<()> {
    let event = ClientMessageEvent::new(
        32,
        id,
        atom(c, b"_NET_WM_STATE")?,
        // EWMH ADD, up to two atoms, source application.
        [1, states[0], states[1], 1, 0],
    );
    c.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| e.to_string())?
    .check()
    .map_err(|e| e.to_string())
}

fn activate_window(c: &RustConnection, root: u32, id: u32, launcher: bool) -> Result<()> {
    if launcher {
        // Initial properties only apply before the first map. WMs may discard
        // them on withdrawal, so restore launcher states after every map.
        add_window_state(c, root, id, [atom(c, b"_NET_WM_STATE_ABOVE")?, 0])?;
        add_window_state(
            c,
            root,
            id,
            [
                atom(c, b"_NET_WM_STATE_SKIP_TASKBAR")?,
                atom(c, b"_NET_WM_STATE_SKIP_PAGER")?,
            ],
        )?;
    }
    // Ask the X server for current time. EWMH activation must not use CurrentTime:
    // a launcher may have been dormant for hours when invoked by a global binding.
    c.change_window_attributes(
        id,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .map_err(|e| e.to_string())?
    .check()
    .map_err(|e| e.to_string())?;
    let clock = atom(c, b"_FIRE_UI_ACTIVATION_TIME")?;
    c.change_property8(PropMode::REPLACE, id, clock, AtomEnum::STRING, b"time")
        .map_err(|e| e.to_string())?;
    c.flush().map_err(|e| e.to_string())?;
    let timestamp = loop {
        if let Event::PropertyNotify(event) = c.wait_for_event().map_err(|e| e.to_string())? {
            if event.atom == clock {
                break event.time;
            }
        }
    };
    // Source 2 identifies a desktop launcher/pager, rather than an unsolicited app.
    let event = ClientMessageEvent::new(
        32,
        id,
        atom(c, b"_NET_ACTIVE_WINDOW")?,
        [if launcher { 2 } else { 1 }, timestamp, 0, 0, 0],
    );
    c.send_event(
        false,
        root,
        EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
        event,
    )
    .map_err(|e| e.to_string())?;
    // Bare X servers have no WM to consume EWMH; support probes without overriding
    // a real WM's focus-stealing policy.
    let wm = c
        .get_property(
            false,
            root,
            atom(c, b"_NET_SUPPORTING_WM_CHECK")?,
            AtomEnum::WINDOW,
            0,
            1,
        )
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    if wm.value.is_empty() {
        c.set_input_focus(InputFocus::PARENT, id, timestamp)
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| e.to_string())?;
    }
    if launcher {
        // Also raise on bare servers and WMs that do not implement ABOVE.
        c.configure_window(id, &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE))
            .map_err(|e| e.to_string())?
            .check()
            .map_err(|e| e.to_string())?;
    }
    c.flush().map_err(|e| e.to_string())
}
/// Whether the window has an alpha visual and a compositing manager owns this screen.
pub(crate) fn composited(window: &Window) -> Result<bool> {
    let (c, screen, id) = connection(window)?;
    let depth = c
        .get_geometry(id)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?
        .depth;
    let manager = atom(&c, format!("_NET_WM_CM_S{screen}").as_bytes())?;
    let owner = c
        .get_selection_owner(manager)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?
        .owner;
    Ok(depth == 32 && owner != x11rb::NONE)
}
pub(crate) fn pointer() -> Result<(i32, i32)> {
    let (c, screen) = x11rb::connect(None).map_err(|e| e.to_string())?;
    let p = c
        .query_pointer(c.setup().roots[screen].root)
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?;
    Ok((p.root_x.into(), p.root_y.into()))
}

fn input_focus_within(c: &impl Connection, window: u32) -> Result<bool> {
    let mut focus = c
        .get_input_focus()
        .map_err(|e| e.to_string())?
        .reply()
        .map_err(|e| e.to_string())?
        .focus;
    // None and PointerRoot are special focus values, not window IDs.
    while focus != x11rb::NONE && focus != u32::from(InputFocus::POINTER_ROOT) {
        if focus == window {
            return Ok(true);
        }
        let tree = c
            .query_tree(focus)
            .map_err(|e| e.to_string())?
            .reply()
            .map_err(|e| e.to_string())?;
        focus = tree.parent;
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires an isolated X11 server without a WM (run with xvfb-run)"]
    fn launcher_remaps_request_states_before_activation_and_raise() {
        let (c, screen) = x11rb::connect(None).unwrap();
        let root = c.setup().roots[screen].root;
        let (observer, _) = x11rb::connect(None).unwrap();
        observer
            .change_window_attributes(
                root,
                &ChangeWindowAttributesAux::new().event_mask(EventMask::SUBSTRUCTURE_NOTIFY),
            )
            .unwrap()
            .check()
            .unwrap();
        let window = c.generate_id().unwrap();
        let other = c.generate_id().unwrap();
        for id in [window, other] {
            c.create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                id,
                root,
                0,
                0,
                100,
                100,
                0,
                WindowClass::INPUT_OUTPUT,
                x11rb::COPY_FROM_PARENT,
                &CreateWindowAux::new(),
            )
            .unwrap()
            .check()
            .unwrap();
        }
        let state = atom(&c, b"_NET_WM_STATE").unwrap();
        let above = atom(&c, b"_NET_WM_STATE_ABOVE").unwrap();
        let taskbar = atom(&c, b"_NET_WM_STATE_SKIP_TASKBAR").unwrap();
        let pager = atom(&c, b"_NET_WM_STATE_SKIP_PAGER").unwrap();
        let active = atom(&c, b"_NET_ACTIVE_WINDOW").unwrap();
        for _ in 0..2 {
            // Simulate a WM discarding state on withdrawal; restore via messages,
            // never by overwriting the WM-owned property of a mapped window.
            c.delete_property(window, state).unwrap().check().unwrap();
            c.map_window(window).unwrap().check().unwrap();
            c.map_window(other).unwrap().check().unwrap();
            c.configure_window(
                other,
                &ConfigureWindowAux::new().stack_mode(StackMode::ABOVE),
            )
            .unwrap()
            .check()
            .unwrap();
            activate_window(&c, root, window, true).unwrap();
            assert_eq!(
                c.query_tree(root).unwrap().reply().unwrap().children.last(),
                Some(&window)
            );
            assert!(c
                .get_property(false, window, state, AtomEnum::ATOM, 0, 3)
                .unwrap()
                .reply()
                .unwrap()
                .value
                .is_empty());
            observer.get_input_focus().unwrap().reply().unwrap();
            let mut messages = Vec::new();
            while let Some(event) = observer.poll_for_event().unwrap() {
                if let Event::ClientMessage(event) = event {
                    assert_eq!(event.window, window);
                    assert_eq!(event.format, 32);
                    messages.push((event.type_, event.data.as_data32()));
                }
            }
            assert_eq!(messages.len(), 3);
            assert_eq!(messages[0], (state, [1, above, 0, 1, 0]));
            assert_eq!(messages[1], (state, [1, taskbar, pager, 1, 0]));
            assert_eq!(messages[2].0, active);
            assert_eq!(messages[2].1[0], 2);
            assert_ne!(messages[2].1[1], x11rb::CURRENT_TIME);
            c.unmap_window(window).unwrap().check().unwrap();
        }
    }

    #[test]
    #[ignore = "requires an isolated X11 server (run with xvfb-run)"]
    fn input_focus_tracks_descendants_and_survives_keyboard_grabs() {
        let (c, screen) = x11rb::connect(None).unwrap();
        let root = c.setup().roots[screen].root;
        let window = c.generate_id().unwrap();
        let child = c.generate_id().unwrap();
        for (id, parent) in [(window, root), (child, window)] {
            c.create_window(
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
                &CreateWindowAux::new()
                    .override_redirect(1)
                    .event_mask(EventMask::FOCUS_CHANGE),
            )
            .unwrap()
            .check()
            .unwrap();
            c.map_window(id).unwrap().check().unwrap();
        }
        for target in [window, child] {
            c.set_input_focus(InputFocus::PARENT, target, x11rb::CURRENT_TIME)
                .unwrap()
                .check()
                .unwrap();
            assert!(input_focus_within(&c, window).unwrap());
        }
        while c.poll_for_event().unwrap().is_some() {}
        let (grabber, _) = x11rb::connect(None).unwrap();
        assert_eq!(
            grabber
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
        let mut saw_grab = false;
        while !saw_grab {
            if let Event::FocusOut(event) = c.wait_for_event().unwrap() {
                saw_grab = event.mode == NotifyMode::GRAB;
            }
        }
        assert!(input_focus_within(&c, window).unwrap());
        grabber
            .ungrab_keyboard(x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        c.set_input_focus(InputFocus::PARENT, root, x11rb::CURRENT_TIME)
            .unwrap()
            .check()
            .unwrap();
        assert!(!input_focus_within(&c, window).unwrap());
        for target in [x11rb::NONE, u32::from(InputFocus::POINTER_ROOT)] {
            c.set_input_focus(InputFocus::PARENT, target, x11rb::CURRENT_TIME)
                .unwrap()
                .check()
                .unwrap();
            assert!(!input_focus_within(&c, window).unwrap());
        }
    }
}
