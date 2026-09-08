# fire-ui-widgets

Optional controls and composition layers built entirely on Fire UI's public widget
protocol. Includes labels, arbitrary-content buttons, row/column layout, padding,
themes, a Unicode text editor with undo/redo, scrolling, modal menus and a
fixed-height virtual list.

```rust
use fire_ui::Element;
use fire_ui_widgets::{Button, Label, Padding};

let button = Button::new(Element::leaf(Label::new("Continue")), "Continue");
let content = Padding::new(button, 16.0);
```

Connect a button's output to an owner command with `children.connect`. Themes and
layout policies are optional; custom widgets retain direct input, layout and
painting control. `EditorDecoration` lets consumers animate typed or selected text
without replacing the editor. Virtual lists mount only their visible rows.

See the [quick start](https://github.com/sobir-git/fire-ui#start-an-app)
and [studio](https://github.com/sobir-git/fire-ui/tree/v0.4.0/examples/studio).
Experimental 0.x API; MIT licensed.
