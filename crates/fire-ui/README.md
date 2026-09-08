# fire-ui

Platform-independent widget ownership, typed commands and outputs, layout, input,
scheduling, text and drawing contracts. This crate has no dependencies and does
not select a window system or renderer. It requires the Rust standard library.

A custom widget uses the same public protocol as every built-in control:

```rust
use fire_ui::*;

struct Swatch(Color);

impl Widget for Swatch {
    type Command = ();
    type Output = ();

    fn layout(&mut self, _: &mut Layout<'_>, constraints: Constraints) -> Metrics {
        Metrics::new(constraints.constrain(Size::new(120.0, 80.0)))
    }

    fn paint(&self, cx: &mut Paint<'_>) {
        cx.painter.rect(cx.bounds, 8.0, self.0.into());
    }
}

let swatch = Element::leaf(Swatch(Color::hex(0xff8a36)));
```

A persistent `Widget` owns its state. `Element::build` installs typed children;
`Child<W>` identifies a child owned by that widget. Child outputs map to parent
commands with `connect`, or pass straight through with `bubble`, which is what
makes a decorator transparent in both directions. Use `Update` for state changes,
`Layout` for measuring and placing children, and `Paint` for drawing. Overlays —
menus, popovers — are inserted already anchored to the window or a sibling, and
escape ancestor clipping. The host sleeps unless input, queued work, a
deadline or an explicit frame request needs attention.

Custom hosts implement `Painter` and `TextEngine` and drive `Ui`. Clipboard requests
are disabled by default. Hosts that handle `HostRequest::Copy` and `HostRequest::Paste`
call `Ui::set_clipboard_enabled(true)` before delivering input; unsupported requests
return `Error::Unsupported` before widgets mutate a cut selection.
Use `fire-ui-widgets` for optional composition helpers and `fire-ui-native` for
native desktop hosting. No app storage or app-specific theme lives in this crate.

See the [architecture](https://github.com/sobir-git/fire-ui/blob/v0.6.0/docs/architecture.md)
and [studio](https://github.com/sobir-git/fire-ui/tree/v0.6.0/examples/studio).
Experimental 0.x API; MIT licensed.
