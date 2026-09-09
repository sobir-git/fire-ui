# fire-ui-widgets

Optional controls, layout policies and a design-token system, built entirely on
Fire UI's public widget protocol. Nothing here is privileged: a custom widget uses
exactly the same interfaces.

**Controls.** `Label`, `Button` in four emphasis levels, `Checkbox`, `Switch`,
`Slider`, `Progress`, `Dropdown`, `Tabs`, `Divider`, a Unicode `Editor` with undo
and IME support, a modal `Menu`, and a fixed-height `VirtualList`. Each one carries
its own keyboard handling, focus ring, pointer cursor and accessible semantics.

**Layout.** `row` and `column` take a `Flow` — a gap, a `Justify` for leftover
space, and an `Align` across the axis — and `Entry` says whether a child is
`natural`, `fixed` or a weighted `fill`. `stack` overlays children; `grid` reflows
to a minimum column width. The `_at` variants start from an origin, so a widget can
sequence several policies without a container per combination. A column measures each child against
the space its predecessors left, not the whole extent, so content cannot silently
overflow the bottom. `Padding`, `Aligned`, `Constrain`, `Spacer`, `Surface` and
`Scroll` are the wrapper widgets;
all of them are transparent in both directions — commands reach the content and its
output passes straight through.

**Theme.** A `Palette` of semantic colours and a `Scale` of sizes, as two
independent axes. Widgets name a role — `surface`, `muted`, `accent`, `on_accent` —
and never a literal value, so a single `AppearanceScope` swap restyles everything
correctly. Three pairings ship (`Theme::dark`, `light` and `compact`), but a theme
is a plain struct of public fields: write your own, mix any palette with any scale,
or install a different one for one subtree. Changing only the palette repaints;
changing the scale relays.

```rust
use fire_ui::Element;
use fire_ui_widgets::{Button, ButtonStyle, Insets, Label, Padding};

let button = Button::styled(
    Element::leaf(Label::new("Continue")),
    "Continue",
    ButtonStyle::Primary,
);
let content = Padding::new(button, Insets::all(16.0));
```

Connect a control's output to an owner command with `children.connect`, or adopt a
decorator's content with `children.bubble`. `EditorDecoration` lets consumers
animate typed or selected text without replacing the editor. Virtual lists mount
only their visible rows.

See the [quick start](https://github.com/sobir-git/fire-ui#start-an-app)
and [studio](https://github.com/sobir-git/fire-ui/tree/v0.7.0/examples/studio).
Experimental 0.x API; MIT licensed.
