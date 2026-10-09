//! A status notifier (tray) icon for Linux desktops, with no dependencies.
//!
//! [`Tray`] speaks just enough D-Bus over the session bus socket to publish one
//! `org.kde.StatusNotifierItem`: KDE Plasma, GNOME with the AppIndicator
//! extension, and other freedesktop panels show it. It needs no window, so a
//! headless service can own the icon and open a window only when clicked.
//!
//! The connection is blocking and owned by the caller's thread. Call
//! [`Tray::next`] to answer the panel and receive clicks; between calls,
//! requests wait in the socket.
//!
//! ```no_run
//! use fire_ui_tray::{Event, Icon, Item, Tray};
//! let mut tray = Tray::new(Item::new("my-app", "My App", Icon::Named("network-wireless".into())))?;
//! loop {
//!     if let Some(Event::Activate { x, y }) = tray.next(None)? {
//!         println!("clicked at {x},{y}");
//!     }
//! }
//! # Ok::<(), std::io::Error>(())
//! ```

mod wire;

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};
use wire::{Message, Reader, Writer};

const PATH: &str = "/StatusNotifierItem";
const ITEM: &str = "org.kde.StatusNotifierItem";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const BUS: &str = "org.freedesktop.DBus";
const MENU_PATH: &str = "/MenuBar";
const MENU: &str = "com.canonical.dbusmenu";
const PROPS: &str = "org.freedesktop.DBus.Properties";

/// What the panel shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Stable application id, such as `internet-reliability`.
    pub id: String,
    /// Human readable name.
    pub title: String,
    pub status: Status,
    pub icon: Icon,
    pub tooltip: Option<Tooltip>,
    /// The item's menu. GNOME's AppIndicator extension opens it on a primary
    /// click and does not show items without one.
    pub menu: Vec<MenuItem>,
}

impl Item {
    pub fn new(id: impl Into<String>, title: impl Into<String>, icon: Icon) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            status: Status::Active,
            icon,
            tooltip: None,
            menu: Vec::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Hidden by panels that collapse unimportant items.
    Passive,
    Active,
    NeedsAttention,
}

impl Status {
    fn name(self) -> &'static str {
        match self {
            Self::Passive => "Passive",
            Self::Active => "Active",
            Self::NeedsAttention => "NeedsAttention",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Icon {
    /// An icon theme name, such as `network-wireless`.
    Named(String),
    /// The same image at several sizes; the panel picks the closest.
    Pixels(Vec<Pixmap>),
}

/// One icon image in the protocol's ARGB32 network byte order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pixmap {
    pub width: u32,
    pub height: u32,
    argb: Vec<u8>,
}

impl Pixmap {
    /// Straight (non-premultiplied) RGBA rows, top to bottom.
    pub fn from_rgba(width: u32, height: u32, rgba: &[u8]) -> Self {
        assert_eq!(
            rgba.len(),
            width as usize * height as usize * 4,
            "rgba length"
        );
        let argb = rgba
            .chunks_exact(4)
            .flat_map(|p| [p[3], p[0], p[1], p[2]])
            .collect();
        Self {
            width,
            height,
            argb,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    Action { label: String, enabled: bool },
    Separator,
}

impl MenuItem {
    pub fn action(label: impl Into<String>) -> Self {
        Self::Action {
            label: label.into(),
            enabled: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tooltip {
    pub title: String,
    pub text: String,
}

/// User interaction with the icon. Coordinates are physical screen pixels when
/// the panel provides them, suitable for anchoring a popup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// Primary click.
    Activate {
        x: i32,
        y: i32,
    },
    /// Middle click.
    SecondaryActivate {
        x: i32,
        y: i32,
    },
    /// Secondary click, for items without a menu.
    ContextMenu {
        x: i32,
        y: i32,
    },
    Scroll {
        delta: i32,
        vertical: bool,
    },
    /// A menu action, by index into [`Item::menu`].
    Menu(usize),
    /// The panel opened the item's menu. On GNOME's AppIndicator extension this
    /// is how a single primary click arrives.
    MenuOpened,
}

pub struct Tray {
    stream: UnixStream,
    buf: Vec<u8>,
    serial: u32,
    name: String,
    item: Item,
    revision: u32,
}

impl Tray {
    /// Connect to the session bus and show the item. A panel that starts later
    /// picks the item up automatically.
    pub fn new(item: Item) -> io::Result<Self> {
        let mut stream = connect()?;
        authenticate(&mut stream)?;
        let name = format!("{ITEM}-{}-1", std::process::id());
        let mut tray = Self {
            stream,
            buf: Vec::new(),
            serial: 0,
            name,
            item,
            revision: 1,
        };
        tray.call(BUS, "/org/freedesktop/DBus", BUS, "Hello", "", |_| {})?;
        let name = tray.name.clone();
        tray.call(
            BUS,
            "/org/freedesktop/DBus",
            BUS,
            "RequestName",
            "su",
            |w| {
                w.str(&name);
                w.u32(4); // DO_NOT_QUEUE
            },
        )?;
        let rule =
            format!("type='signal',sender='{BUS}',member='NameOwnerChanged',arg0='{WATCHER}'");
        tray.call(BUS, "/org/freedesktop/DBus", BUS, "AddMatch", "s", |w| {
            w.str(&rule)
        })?;
        tray.register()?;
        Ok(tray)
    }

    pub fn item(&self) -> &Item {
        &self.item
    }

    /// Change what the panel shows; only changed parts are announced.
    pub fn set(&mut self, item: Item) -> io::Result<()> {
        let old = std::mem::replace(&mut self.item, item);
        if old.title != self.item.title {
            self.signal("NewTitle", "", |_| {})?;
        }
        if old.icon != self.item.icon {
            self.signal("NewIcon", "", |_| {})?;
        }
        if old.tooltip != self.item.tooltip {
            self.signal("NewToolTip", "", |_| {})?;
        }
        if old.menu != self.item.menu {
            self.revision += 1;
            let mut b = Writer::default();
            b.u32(self.revision);
            b.i32(0);
            self.emit(MENU_PATH, MENU, "LayoutUpdated", "ui", b)?;
        }
        if old.status != self.item.status {
            let status = self.item.status.name();
            self.signal("NewStatus", "s", |w| w.str(status))?;
        }
        Ok(())
    }

    /// Answer panel requests until the user interacts with the icon or the
    /// timeout passes (`None` waits indefinitely).
    pub fn next(&mut self, timeout: Option<Duration>) -> io::Result<Option<Event>> {
        let deadline = timeout.map(|t| Instant::now() + t);
        loop {
            while let Some(len) = wire::complete(&self.buf) {
                let frame: Vec<u8> = self.buf.drain(..len).collect();
                if let Some(event) = self.handle(&frame)? {
                    return Ok(Some(event));
                }
            }
            let wait = match deadline {
                Some(d) => match d.checked_duration_since(Instant::now()) {
                    Some(w) if !w.is_zero() => Some(w),
                    _ => return Ok(None),
                },
                None => None,
            };
            self.stream.set_read_timeout(wait)?;
            let mut chunk = [0; 4096];
            match self.stream.read(&mut chunk) {
                Ok(0) => return Err(io::ErrorKind::ConnectionAborted.into()),
                Ok(n) => self.buf.extend_from_slice(&chunk[..n]),
                Err(e)
                    if matches!(
                        e.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None)
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(e) => return Err(e),
            }
        }
    }

    fn register(&mut self) -> io::Result<()> {
        let name = self.name.clone();
        self.call(
            WATCHER,
            "/StatusNotifierWatcher",
            WATCHER,
            "RegisterStatusNotifierItem",
            "s",
            |w| w.str(&name),
        )
    }

    fn handle(&mut self, frame: &[u8]) -> io::Result<Option<Event>> {
        let Some(m) = Message::parse(frame) else {
            return Ok(None);
        };
        match m.kind {
            wire::SIGNAL if m.member == "NameOwnerChanged" => {
                let mut r = m.body();
                let (name, _old, new) = (r.str(), r.str(), r.str());
                if name == Some(WATCHER) && new.is_some_and(|n| !n.is_empty()) {
                    self.register()?;
                }
                Ok(None)
            }
            wire::CALL => self.answer(&m),
            _ => Ok(None),
        }
    }

    fn answer(&mut self, m: &Message) -> io::Result<Option<Event>> {
        let mut r = m.body();
        let point = |r: &mut Reader| (r.i32().unwrap_or(0), r.i32().unwrap_or(0));
        let menu = m.path == MENU_PATH;
        let mut body = Writer::default();
        let (sig, event) = match (m.interface, m.member) {
            (PROPS, "Get") => {
                let (_, name) = (r.str(), r.str().unwrap_or(""));
                let known = if menu {
                    menu_property(&mut body, name)
                } else {
                    property(&mut body, &self.item, name)
                };
                if !known {
                    return self.error(m, "org.freedesktop.DBus.Error.UnknownProperty");
                }
                ("v", None)
            }
            (PROPS, "GetAll") => {
                let names: &[&str] = if menu { &MENU_PROPERTIES } else { &PROPERTIES };
                body.array(8, |w| {
                    for name in names {
                        w.pad(8);
                        w.str(name);
                        if menu {
                            menu_property(w, name);
                        } else {
                            property(w, &self.item, name);
                        }
                    }
                });
                ("a{sv}", None)
            }
            ("org.freedesktop.DBus.Introspectable", "Introspect") => {
                body.str(if menu {
                    MENU_INTROSPECTION
                } else {
                    INTROSPECTION
                });
                ("s", None)
            }
            ("org.freedesktop.DBus.Peer", "Ping") => ("", None),
            (MENU, "GetLayout") => {
                body.u32(self.revision);
                layout(&mut body, &self.item.menu);
                ("u(ia{sv}av)", None)
            }
            (MENU, "GetGroupProperties") => {
                let ids = r.i32_array().unwrap_or_default();
                body.array(8, |w| {
                    for id in ids {
                        w.pad(8);
                        w.i32(id);
                        entry_properties(w, &self.item.menu, id);
                    }
                });
                ("a(ia{sv})", None)
            }
            (MENU, "Event") => {
                let (id, name) = (r.i32().unwrap_or(0), r.str().unwrap_or(""));
                let index = (id as usize).wrapping_sub(1);
                let action = matches!(
                    self.item.menu.get(index),
                    Some(MenuItem::Action { enabled: true, .. })
                );
                let event = match name {
                    "clicked" if action => Some(Event::Menu(index)),
                    "opened" if id == 0 => Some(Event::MenuOpened),
                    _ => None,
                };
                ("", event)
            }
            (MENU, "AboutToShow") => {
                body.u32(0);
                ("b", None)
            }
            (MENU, "AboutToShowGroup") => {
                body.array(4, |_| {});
                body.array(4, |_| {});
                ("aiai", None)
            }
            (ITEM, "Activate") => (
                "",
                Some(point(&mut r)).map(|(x, y)| Event::Activate { x, y }),
            ),
            (ITEM, "SecondaryActivate") => (
                "",
                Some(point(&mut r)).map(|(x, y)| Event::SecondaryActivate { x, y }),
            ),
            (ITEM, "ContextMenu") => (
                "",
                Some(point(&mut r)).map(|(x, y)| Event::ContextMenu { x, y }),
            ),
            (ITEM, "Scroll") => {
                let delta = r.i32().unwrap_or(0);
                let vertical = !r
                    .str()
                    .is_some_and(|o| o.eq_ignore_ascii_case("horizontal"));
                ("", Some(Event::Scroll { delta, vertical }))
            }
            _ => return self.error(m, "org.freedesktop.DBus.Error.UnknownMethod"),
        };
        self.reply(m, sig, body)?;
        Ok(event)
    }

    fn next_serial(&mut self) -> u32 {
        self.serial = self.serial.wrapping_add(1).max(1);
        self.serial
    }

    fn call(
        &mut self,
        to: &str,
        path: &str,
        iface: &str,
        member: &str,
        sig: &str,
        body: impl FnOnce(&mut Writer),
    ) -> io::Result<()> {
        let mut b = Writer::default();
        body(&mut b);
        let serial = self.next_serial();
        let frame = wire::frame(
            wire::CALL,
            wire::NO_REPLY,
            serial,
            &[
                (1, 'o', path),
                (2, 's', iface),
                (3, 's', member),
                (6, 's', to),
            ],
            None,
            sig,
            &b.into_bytes(),
        );
        self.stream.write_all(&frame)
    }

    fn signal(
        &mut self,
        member: &str,
        sig: &str,
        body: impl FnOnce(&mut Writer),
    ) -> io::Result<()> {
        let mut b = Writer::default();
        body(&mut b);
        self.emit(PATH, ITEM, member, sig, b)
    }

    fn emit(
        &mut self,
        path: &str,
        iface: &str,
        member: &str,
        sig: &str,
        body: Writer,
    ) -> io::Result<()> {
        let serial = self.next_serial();
        let fields = [(1, 'o', path), (2, 's', iface), (3, 's', member)];
        let frame = wire::frame(
            wire::SIGNAL,
            0,
            serial,
            &fields,
            None,
            sig,
            &body.into_bytes(),
        );
        self.stream.write_all(&frame)
    }

    fn reply(&mut self, m: &Message, sig: &str, body: Writer) -> io::Result<()> {
        if m.flags & wire::NO_REPLY != 0 {
            return Ok(());
        }
        let serial = self.next_serial();
        let frame = wire::frame(
            wire::RETURN,
            0,
            serial,
            &[(6, 's', m.sender)],
            Some(m.serial),
            sig,
            &body.into_bytes(),
        );
        self.stream.write_all(&frame)
    }

    fn error(&mut self, m: &Message, name: &str) -> io::Result<Option<Event>> {
        if m.flags & wire::NO_REPLY != 0 {
            return Ok(None);
        }
        let serial = self.next_serial();
        let frame = wire::frame(
            wire::ERROR,
            0,
            serial,
            &[(4, 's', name), (6, 's', m.sender)],
            Some(m.serial),
            "",
            &[],
        );
        self.stream.write_all(&frame).map(|_| None)
    }
}

const PROPERTIES: [&str; 16] = [
    "Category",
    "Id",
    "Title",
    "Status",
    "WindowId",
    "IconName",
    "IconPixmap",
    "OverlayIconName",
    "OverlayIconPixmap",
    "AttentionIconName",
    "AttentionIconPixmap",
    "ToolTip",
    "ItemIsMenu",
    "Menu",
    "IconAccessibleDesc",
    "AttentionAccessibleDesc",
];

/// Write one property as a variant; false if it does not exist.
fn property(w: &mut Writer, item: &Item, name: &str) -> bool {
    let (named, pixmaps): (&str, &[Pixmap]) = match &item.icon {
        Icon::Named(n) => (n, &[]),
        Icon::Pixels(p) => ("", p),
    };
    match name {
        "Category" => w.variant("s", |w| w.str("ApplicationStatus")),
        "Id" => w.variant("s", |w| w.str(&item.id)),
        "Title" => w.variant("s", |w| w.str(&item.title)),
        "Status" => w.variant("s", |w| w.str(item.status.name())),
        "WindowId" => w.variant("i", |w| w.i32(0)),
        "IconName" => w.variant("s", |w| w.str(named)),
        "IconPixmap" => w.variant("a(iiay)", |w| write_pixmaps(w, pixmaps)),
        "OverlayIconName" | "AttentionIconName" => w.variant("s", |w| w.str("")),
        "OverlayIconPixmap" | "AttentionIconPixmap" => {
            w.variant("a(iiay)", |w| write_pixmaps(w, &[]))
        }
        "ToolTip" => w.variant("(sa(iiay)ss)", |w| {
            let (title, text) = item
                .tooltip
                .as_ref()
                .map_or(("", ""), |t| (&*t.title, &*t.text));
            w.pad(8);
            w.str("");
            write_pixmaps(w, &[]);
            w.str(title);
            w.str(text);
        }),
        "ItemIsMenu" => w.variant("b", |w| w.u32(0)),
        "Menu" => w.variant("o", |w| w.str(MENU_PATH)),
        // Panels fetch these with the icon and drop the update if one fails.
        "IconAccessibleDesc" | "AttentionAccessibleDesc" => {
            let desc = item.tooltip.as_ref().map_or(&*item.title, |t| &*t.title);
            w.variant("s", |w| w.str(desc))
        }
        _ => return false,
    }
    true
}

const MENU_PROPERTIES: [&str; 4] = ["Version", "TextDirection", "Status", "IconThemePath"];

fn menu_property(w: &mut Writer, name: &str) -> bool {
    match name {
        "Version" => w.variant("u", |w| w.u32(3)),
        "TextDirection" => w.variant("s", |w| w.str("ltr")),
        "Status" => w.variant("s", |w| w.str("normal")),
        "IconThemePath" => w.variant("as", |w| w.array(4, |_| {})),
        _ => return false,
    }
    true
}

/// The root (id 0) with one child per menu item (ids from 1).
fn layout(w: &mut Writer, menu: &[MenuItem]) {
    w.pad(8);
    w.i32(0);
    entry_properties(w, menu, 0);
    w.array(1, |w| {
        for id in 1..=menu.len() as i32 {
            w.variant("(ia{sv}av)", |w| {
                w.pad(8);
                w.i32(id);
                entry_properties(w, menu, id);
                w.array(1, |_| {});
            });
        }
    });
}

fn entry_properties(w: &mut Writer, menu: &[MenuItem], id: i32) {
    let entry = |w: &mut Writer, key: &str, sig: &str, value: &dyn Fn(&mut Writer)| {
        w.pad(8);
        w.str(key);
        w.variant(sig, value);
    };
    w.array(8, |w| {
        match (id as usize).checked_sub(1).map(|i| menu.get(i)) {
            None => entry(w, "children-display", "s", &|w| w.str("submenu")),
            Some(Some(MenuItem::Action { label, enabled })) => {
                entry(w, "label", "s", &|w| w.str(label));
                entry(w, "enabled", "b", &|w| w.u32(*enabled as u32));
            }
            Some(Some(MenuItem::Separator)) => entry(w, "type", "s", &|w| w.str("separator")),
            Some(None) => {}
        }
    });
}

fn write_pixmaps(w: &mut Writer, pixmaps: &[Pixmap]) {
    w.array(8, |w| {
        for p in pixmaps {
            w.pad(8);
            w.i32(p.width as i32);
            w.i32(p.height as i32);
            w.array(1, |w| w.bytes(&p.argb));
        }
    });
}

const INTROSPECTION: &str = r#"<node>
 <interface name="org.kde.StatusNotifierItem">
  <property name="Category" type="s" access="read"/>
  <property name="Id" type="s" access="read"/>
  <property name="Title" type="s" access="read"/>
  <property name="Status" type="s" access="read"/>
  <property name="WindowId" type="i" access="read"/>
  <property name="IconName" type="s" access="read"/>
  <property name="IconPixmap" type="a(iiay)" access="read"/>
  <property name="OverlayIconName" type="s" access="read"/>
  <property name="OverlayIconPixmap" type="a(iiay)" access="read"/>
  <property name="AttentionIconName" type="s" access="read"/>
  <property name="AttentionIconPixmap" type="a(iiay)" access="read"/>
  <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
  <property name="ItemIsMenu" type="b" access="read"/>
  <method name="Activate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
  <method name="SecondaryActivate"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
  <method name="ContextMenu"><arg name="x" type="i" direction="in"/><arg name="y" type="i" direction="in"/></method>
  <method name="Scroll"><arg name="delta" type="i" direction="in"/><arg name="orientation" type="s" direction="in"/></method>
  <signal name="NewTitle"/>
  <signal name="NewIcon"/>
  <signal name="NewToolTip"/>
  <signal name="NewStatus"><arg name="status" type="s"/></signal>
 </interface>
 <interface name="org.freedesktop.DBus.Properties">
  <method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method>
  <method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method>
 </interface>
</node>"#;

const MENU_INTROSPECTION: &str = r#"<node>
 <interface name="com.canonical.dbusmenu">
  <property name="Version" type="u" access="read"/>
  <property name="TextDirection" type="s" access="read"/>
  <property name="Status" type="s" access="read"/>
  <property name="IconThemePath" type="as" access="read"/>
  <method name="GetLayout"><arg type="i" direction="in"/><arg type="i" direction="in"/><arg type="as" direction="in"/><arg type="u" direction="out"/><arg type="(ia{sv}av)" direction="out"/></method>
  <method name="GetGroupProperties"><arg type="ai" direction="in"/><arg type="as" direction="in"/><arg type="a(ia{sv})" direction="out"/></method>
  <method name="Event"><arg type="i" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/><arg type="u" direction="in"/></method>
  <method name="AboutToShow"><arg type="i" direction="in"/><arg type="b" direction="out"/></method>
  <method name="AboutToShowGroup"><arg type="ai" direction="in"/><arg type="ai" direction="out"/><arg type="ai" direction="out"/></method>
  <signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
 </interface>
</node>"#;

fn connect() -> io::Result<UnixStream> {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    for entry in address.iter().flat_map(|a| a.split(';')) {
        let Some(params) = entry.strip_prefix("unix:") else {
            continue;
        };
        for kv in params.split(',') {
            if let Some(path) = kv.strip_prefix("path=") {
                return UnixStream::connect(unescape(path));
            }
            if let Some(name) = kv.strip_prefix("abstract=") {
                use std::os::linux::net::SocketAddrExt;
                let addr =
                    std::os::unix::net::SocketAddr::from_abstract_name(unescape(name).as_bytes())?;
                return UnixStream::connect_addr(&addr);
            }
        }
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").ok_or(io::ErrorKind::NotFound)?;
    UnixStream::connect(std::path::Path::new(&runtime).join("bus"))
}

/// D-Bus addresses percent-escape bytes outside a safe set.
fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn authenticate(stream: &mut UnixStream) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    let uid = std::fs::metadata("/proc/self")?.uid().to_string();
    let hex: String = uid.bytes().map(|b| format!("{b:02x}")).collect();
    stream.write_all(format!("\0AUTH EXTERNAL {hex}\r\n").as_bytes())?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut line = Vec::new();
    let mut byte = [0];
    while !line.ends_with(b"\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        line.push(byte[0]);
    }
    if !line.starts_with(b"OK ") {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "D-Bus authentication rejected",
        ));
    }
    stream.write_all(b"BEGIN\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgba_converts_to_network_order_argb() {
        let p = Pixmap::from_rgba(1, 1, &[1, 2, 3, 4]);
        assert_eq!(p.argb, [4, 1, 2, 3]);
    }

    #[test]
    fn addresses_unescape() {
        assert_eq!(unescape("/run/user/1000/bus"), "/run/user/1000/bus");
        assert_eq!(unescape("/tmp/a%20b"), "/tmp/a b");
    }

    #[test]
    fn property_variants_round_trip_their_signatures() {
        let item = Item::new(
            "id",
            "Title",
            Icon::Pixels(vec![Pixmap::from_rgba(1, 1, &[0; 4])]),
        );
        for name in PROPERTIES {
            let mut w = Writer::default();
            assert!(property(&mut w, &item, name), "{name}");
            let bytes = w.into_bytes();
            let mut r = Reader::new(&bytes);
            assert!(r.signature().is_some(), "{name}");
        }
        assert!(!property(&mut Writer::default(), &item, "Unknown"));
    }
}
