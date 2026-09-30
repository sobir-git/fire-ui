//! EWMH launcher integration, kept private to the X11 host.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::Window;
use x11rb::{
    connection::Connection,
    protocol::{xproto::*, Event},
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

type Result<T> = std::result::Result<T, String>;
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
    // Ask the X server for current time. EWMH activation must not use CurrentTime:
    // a launcher may have been dormant for hours when invoked by a global binding.
    c.change_window_attributes(
        id,
        &ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE),
    )
    .map_err(|e| e.to_string())?
    .check()
    .map_err(|e| e.to_string())?;
    let clock = atom(&c, b"_FIRE_UI_ACTIVATION_TIME")?;
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
    let root = c.setup().roots[screen].root;
    // Source 2 identifies a desktop launcher/pager, rather than an unsolicited app.
    let event = ClientMessageEvent::new(
        32,
        id,
        atom(&c, b"_NET_ACTIVE_WINDOW")?,
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
            atom(&c, b"_NET_SUPPORTING_WM_CHECK")?,
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
