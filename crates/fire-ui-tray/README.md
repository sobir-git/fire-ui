# fire-ui-tray

A status notifier (tray) icon for Linux desktops with no dependencies. It
publishes one `org.kde.StatusNotifierItem` with a `com.canonical.dbusmenu` menu
over the session bus. KDE Plasma, GNOME with the AppIndicator extension (Ubuntu's
default) and other freedesktop panels show it.

`Tray` needs no window, so a headless service can own the icon and open a Fire UI
window only when the user asks for it. That keeps the always-running part at about
2 MB of RAM. The connection is blocking: call `Tray::next(timeout)` on the thread
that owns it to answer the panel and receive `Event`s; requests wait in the
socket between calls. `Tray::set` swaps the item and announces only what changed.

```rust
use fire_ui_tray::{Event, Icon, Item, MenuItem, Tray};
let mut item = Item::new("my-app", "My App", Icon::Named("network-wireless".into()));
item.menu = vec![MenuItem::action("Show"), MenuItem::Separator, MenuItem::action("Quit")];
let mut tray = Tray::new(item)?;
loop {
    match tray.next(None)? {
        Some(Event::Activate { x, y }) => { /* open a popup at x, y */ }
        Some(Event::Menu(2)) => break,
        _ => {}
    }
}
# Ok::<(), std::io::Error>(())
```

Pair a click with `fire-ui-native`'s `WindowKind::Popup` and `Placement::Anchor`
(or `Placement::Pointer` for menu actions, which carry no coordinates) for a popup
that opens beside the icon and closes when it loses focus.

Panels differ in how clicks map to events. KDE sends `Activate` on a primary
click. GNOME's AppIndicator extension opens the menu on a primary click, sends
`Activate` on a double click and `SecondaryActivate` on a middle click, and
shows no item without a menu, so give every item at least one action.
`Activate` coordinates are physical screen pixels.

Icons are theme names or ARGB pixmaps (`Pixmap::from_rgba`) at one or more sizes.
