# Changelog

## 0.6.0 · unreleased

Ambient values, a second scale, and the widget cleanups the compact theme exposed.
Public contracts changed; consumers update directly.

- Separate text services and renderer resources from the native host. Add direct
  X11/Cairo drawing and move OpenGL drawing into an optional crate.
- Share explicit font storage between text and drawing, remove redundant text
  geometry storage, and release obsolete accessibility snapshots before layout.
- Preserve native text actions and unlimited editor history; add semantic scrolling
  through the same public interface for built-in and custom widgets.
- Measure native memory with exact-byte, zero-swap checks. Full Fire Notes memory
  acceptance remains under investigation; no replacement has been released.

- Add `Palette::slate_dark` and `Scale::compact`, and `Theme::compact` pairing
  them: a dense instrument panel against the roomy ember default. Palette and
  scale stay independent axes, so any pairing works —
  `Theme { color: Palette::ember_dark(), scale: Scale::compact() }` is a compact
  ember panel — and nothing in the framework prefers a shipped pair.
- A node's ambient values are now keyed by type instead of held in a single slot.
  Publishing one type to a child no longer evicts the others or blocks them from
  ever reaching it: a virtual list's row can hold the selection its owner published
  *and* still read the theme, which it could not before. Inheritance stops per type,
  at the node that publishes that type. Nodes share one map until something
  publishes, so installing a value for a subtree usually allocates once for the
  whole subtree.
- `VirtualList` takes a `RowHeight`: a number, or a rule read from the theme so rows
  tighten with it. `Scrollbar` geometry takes the strip width rather than a theme,
  so a scrolling widget remembers one number between layouts instead of a palette.
- `Lifecycle::Inherited` now reaches every widget whose inherited environment
  changed, not only those publishing their own to a child. A widget whose structure
  depends on the ambient value — how tall a row is, and so how many to own — cannot
  learn it from layout alone.
- The studio's theme section picks between all three. Choosing the compact one
  changes metrics rather than only colours, so it relays the window instead of
  repainting it, which the probe now checks.

## 0.5.0 · 2026-09-08

Controls, layout and theming are the focus. Public contracts changed throughout;
consumers update directly.

- Add `Checkbox`, `Switch`, `Slider`, `Progress`, `Dropdown`, `Tabs`, `Divider`
  and `Surface`. Each carries keyboard handling, a focus ring, a pointer cursor
  and accessible semantics rather than paint alone.
- Replace the flat `Theme` with a semantic `Palette` and a `Scale`, a six-step
  `TextRole` type scale and `ColorRole` text colours. Widgets name roles, never
  literal values, so one `AppearanceScope` swap repaints a whole window; ships
  ember dark and ember light palettes.
- Rework layout: `Flow` carries gap, `Justify` and `Align`; `Entry` claims natural,
  fixed or weighted length; add `stack`, `grid`, per-side `Insets`, `Aligned`,
  `Constrain` and `Spacer`. The `_at` variants lay out from an origin so one widget
  can sequence several policies without a container per combination.
- A row or column now measures each child against the space its predecessors left,
  not the whole extent. Previously a heading and a caption above a panel pushed the
  panel past the bottom, where an ancestor clipped its border away — invisible in
  published bounds, which are already intersected with the clip.
- Controls reserve room for the halo they draw when hovered, checked or focused
  (`Scale::halo`), so it no longer crosses their bounds and is clipped into a hard
  edge. `focus_ring` draws only a ring: it used to add a glow whose filled interior
  washed the control it marked.
- `Editor::chrome(false)` now suppresses the background as well as the border, so an
  editor inside a themed surface is one well rather than a box inside a box.
- One `Scrollbar` is shared by the viewport, the virtual list and the editor, which
  previously had three separate implementations with different sizes, positions and
  colours — the editor's ignored the theme entirely and drew itself in white. Each
  bar occupies its own strip rather than floating over the content, so text wraps
  beside it and the content underneath no longer claims the pointer: hovering a
  scrollbar used to show a text caret, and the list's showed a hand as though the
  bar were a row. The virtual list's bar can now be dragged; it only ever painted
  one. A bar answers the pointer: the thumb rests narrow and dim, grows to the full
  track and brightens under the pointer, and takes the accent while dragged.
- Moving the pointer over an editor no longer scrolls the view back to the caret.
- Add `Children::bubble`, so a decorator is transparent in both directions: commands
  reach its content and the content's output passes through untouched. Padding,
  alignment, constraints, surfaces and viewports all use it.
- Overlays can anchor to the window, not only to a sibling, and are born anchored
  through `Update::insert_at` — a handle returned by `insert` is not yet owned when
  the inserting callback runs, so anchoring afterwards was silently rejected. This
  is what lets a dropdown's list escape a scrolling panel.
- Add `Lifecycle::Inherited`, delivered when an inherited environment value changed
  and a widget publishes its own to a child. Without it a control that derives a
  theme for its content froze that content on the palette present at mount.
- Add `Brush::Radial` and `Brush::Box` to the paint contract, and the
  `glow`/`focus_ring`/`drop_shadow` helpers built on them.
- Add `Role::{Radio, Switch, Slider, Progress, TabList, Heading, Link, Group}`, a
  numeric `Semantics::range` published to AccessKit and inspection, and
  `SemanticError::InvalidValue`.
- Add `Update::window_bounds` and `Layout::viewport` for widgets that position
  themselves against the window.
- Replace the studio with a navigable gallery of seven sections. It contains no
  literal colour, font size or panel coordinate; every gap it revealed is closed
  above rather than worked around in the app.

Known gap: `VirtualList` publishes itself but not per-row list items, so rows reach
assistive technology as their own content rather than as selectable list entries.

## 0.4.0 · 2026-09-08

- Native defaults include X11 rendering only; accessibility, inspection, clipboard,
  dialogs and bitmap-font decoding are independent opt-ins.
- The host loads no fonts by default. Apps select primary and fallback files.
- Direct rendering is the default; retained partial repainting is an explicit
  CPU/memory tradeoff through `WindowOptions::partial_repaint`.
- Notes and Studio choose their required services, multilingual fonts and retained rendering.


- Complete Linux AT-SPI EditableText operations with atomic Unicode range editing,
  undo, failure acknowledgements and asynchronous clipboard reads.
- Verify installed IBus XIM and Orca; fix CJK fallback, candidate placement and stale
  composition after external edits. Native Windows/macOS adapters stay separate.

- Add shared semantic action capabilities, validated text selection, stable app keys,
  editor composition state, AccessKit text runs, and opt-in Unix JSON inspection.
- Use Unicode bidi runs and shaped grapheme geometry for drawing, visual caret
  movement, hit testing and selection. Preserve IME selection and temporary composition.
- Add ordered fallback fonts, local paint damage and retained framebuffer presentation.
  Editor decorations can report their changed bounds.
- Add per-button styling and pointer cursors, correct menu/check/tab semantics, and
  native focus, restore, fullscreen and always-on-top requests.
- Public contracts changed; consumers must update directly. Wayland work is deferred.

## 0.2.0

- Add passive, click-through native overlays and configurable minimum window size.
- Linux overlays require X11/XWayland; native Wayland overlay placement is not supported.

## 0.1.0

Initial standalone, MIT-licensed release: widget core, optional controls/editor,
native host, studio and minimal app example. Shared crate version, Rust 1.88
baseline, package verification and platform CI. Experimental; breaking redesigns
are expected. Current limits are listed in the README.
