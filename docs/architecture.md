# Fire UI architecture

The framework and reusable widgets are the product. Fire Notes and games consume
its supported interfaces. The replacement is implemented directly in `crates/`.
Previous frameworks and executable design models exist only in Git history.
[Project rules](../AGENTS.md) explicitly reject backward compatibility.
Fire Notes is a separate repository at `../fire-notes`; this workspace contains
only the framework crates and studio. Dependency direction is app to framework.

## Authoring

A persistent `Widget` owns its state and declares `Command` and `Output` types.
`Element::build` constructs its children once and returns invariant `Child<W>`
handles. A child output maps to an owner command. Owners can send commands to direct
children without downcasts, numeric routing tags or access to their private state.

```rust
Element::build(|children| Counter {
    value: children.add(Element::leaf(Label::new("0"))),
    add: children.connect(button("Add one"), |_| ()),
    count: 0,
    pending: false,
})
```

The [studio](../examples/studio/src/main.rs) is the worked example: a shell owning
seven pages, every shipped control, a searchable list, a custom canvas and a live
layout playground. None of them has access to private runtime nodes. The list page
owns its own search and filtering; its owner receives only a selected key.
`Menu` accepts action keys, labels, shortcut hints, checked and enabled states.
It owns keyboard navigation and accessible action rows; its parent receives the
chosen key or dismissal and controls the modal overlay. Fire Notes uses it for
tab actions without adding app commands to the framework.

`Children::add` accepts children that cannot emit output. `connect` maps a borrowed
output to an owner command; `forward` transfers an owned output unchanged as this
widget's command; `bubble` passes it through as this widget's own output, without
invoking `update`, which is what a transparent decorator needs; `discard` explicitly
ignores output. Detached subtrees are admitted atomically under node and
message limits. Mount runs before layout and input. Dynamic handles become usable
when their admitted structural change is committed after the callback.

## Runtime and phases

There is one ownership tree. Compact reusable slots store private nodes; process-wide
monotonic identities prevent stale or foreign typed handles from naming reused storage.
Parent links determine routes and cleanup. Placement may differ from ownership for
anchored overlays. Published transforms, clips and bounds feed painting, pointer
conversion, IME positioning and semantic geometry.

| Phase | Capabilities |
| --- | --- |
| Update/input/lifecycle | Change local state, send typed commands, emit output, request effects |
| Layout | Measure and place direct children; obtain immutable paragraphs |
| Paint | Read published state and draw; no mutation or scheduling |

Callbacks never synchronously borrow another widget mutably. Commands and outputs
use one bounded mailbox. Mapped output reserves the resulting command's byte cost
before admission. Forwarding uses the freed mailbox slot. `Full`, `Stale` and
`NotOwned` return the original payload. Removal cancels queued delivery; successful
admission is not a promise of delivery after removal.

A callback's structural mutations commit after its widget is restored. Lifecycle
callbacks defer their requested mutations through bounded delivery, so focus changes
cannot recurse indefinitely. Retirement makes the entire subtree ineligible, cancels
held keys/capture/work, then sends child-first unmount notifications.

Structural mutations currently preserve callback order. Frame demand and timer/task
assignments coalesce by owner/handle. This is not a transaction and cannot roll back
widget state. Consumers must handle overload. The studio's list page retains the
latest filter for retry on the next frame; applications must choose their own policy
for critical outputs. `Data::bytes` is a trusted accounting contract, not a sandbox for arbitrary
widget code or a bound on document/model storage.

## Layout and composition

Children report intrinsic size and optional baseline under constraints. Parents
choose placement, clipping and transforms. Row/column helpers borrow child handles;
they do not build a second tree. Scroll measures its content at viewport width and
derives the extent.

A flow carries the gap, a `Justify` for leftover main-axis space and an `Align`
across the other axis; each `Entry` claims a natural, fixed or weighted length, and
may override the alignment for itself alone. `Fill` degrades to natural measurement
under an unbounded axis, which is what a column inside a viewport gets. `stack`
overlays children, and `grid` takes a minimum column width instead of a column count
so it reflows with the window. Every policy has an `_at` variant that lays out from
an origin, so one widget sequences a column and then a grid below it without a
container type for each combination.

Wrappers are transparent in both directions. `Children::bubble` adopts a child whose
output *is* the wrapper's output: the payload is delivered past the wrapper without
invoking its `update`, which leaves `Command` free to address the content. Padding,
alignment, constraints, surfaces and viewports are all built this way, so decorating
a widget never severs the owner's ability to command it.

Overlays anchor to the window or to a sibling; both escape ancestor clipping and are
published against the viewport. An overlay must be born anchored, through
`insert_at`: a handle returned by `insert` names a child the widget does not yet own
when the callback runs, so anchoring afterwards is rejected. This is how a dropdown
list leaves the scrolling panel that opened it.

Buttons own arbitrary ordinary content and add activation, capture, focus, semantics
and appearance. Decorative content leaves activation to the button. A virtual list
accepts a row factory and unique stable keys, keeps visible rows plus overscan mounted,
and owns selection/navigation. Selection is published to row content through
`ListRowState`; optional hover selection supports pickers. Navigation can be driven
by an owner while focus remains in its search field. Its single row height is explicit, and may be a
number or a rule read from the theme, in which case rows follow a theme change. Key indexing avoids
scanning the entire dataset while laying out visible rows.

One `Scrollbar` serves every scrolling surface — viewport, virtual list and editor —
and reserves its own strip rather than floating above content, so a bar is always
clickable and never steals the pointer shape from what is beneath it.

Immutable theme values propagate through an affected subtree until a nested explicit
scope. A theme is a semantic `Palette` and a `Scale`, two axes that vary
independently; widgets name a role and never a literal value, which is what allows
one swap to restyle a whole window correctly. Shipped pairings are conveniences over
public structs, not a closed set, and a subtree can install its own. Local label appearance can override the type-scale step or the colour
role. Metric changes invalidate layout; colour changes repaint without reshaping text.

A nested scope also stops inheritance, which controls rely on: a filled button
publishes a theme to its own content so the label resolves to `on_accent`. That
would otherwise freeze the content on the palette present at mount, so a widget
whose child holds its own value is told `Lifecycle::Inherited` when the ambient
value changes, and republishes what it derived. Clean pointer events
skip geometry traversal, and valid paragraphs survive height-only resizing.

## Input and text

Input routes through preview, target and bubble phases. Each pointer has independent
capture. Focus sessions invalidate stale keyboard routes and paste delivery. Modal
scopes restrict input and restore eligible focus on close, hide and removal. Anchored
overlays publish their dependencies before their own geometry.
Hover includes the target and its eligible ancestors. Moving between a control's
label and padding keeps that control hovered; shared ancestors receive no spurious
leave/enter events. Paint state and hover lifecycle notifications use the same path.

The editor owns a `Document` adapter. `StringDocument` is the default; storage is not
part of the kernel. Edits support selection, insertion, grapheme deletion, clipboard,
preedit, undo and redo. Programmatic `Set` is silent; actual edits emit a revision and
text snapshot. Undo history is bounded. An optional byte limit rejects growth with
an explicit output, preserving the previous text and selection. Caret blinking can
be disabled without losing the visible caret, allowing a focused notes editor to sleep.
`EditorState` restores caret, anchor, scroll and wrap together, without forcing the
caret back into view. Selection and scrolling emit state updates separately from
text edits. Focus changes are also observable by consumers such as inline rename.
The editor supports word/line clicks, Shift-click, line movement and scrollbar drag.

`EditorDecoration` receives the editor's shared paragraph, selection, caret and
visible rectangle. It paints behind or above text through the ordinary `Painter`.
Its frame callback asks for another frame only while needed. Fire Notes implements
its bounded particle effect entirely through this public extension, without runtime
or renderer access. Visible selection geometry and caret-line lookup avoid scanning
the entire document on each animated frame.

Native wrapping prefers whitespace boundaries and falls back to graphemes for long words.
Shared immutable paragraphs provide drawing,
caret hit testing, selection geometry and IME cursor position. Native measurement and
drawing share the same font context. The renderer skips paragraphs outside the clip
and selects visible lines before issuing text draws.

Unicode bidi levels determine visual runs. Painting, hit testing, arrow movement
and discontiguous selection share shaped grapheme cells. Ordered fallback fonts are
optional host configuration, empty by default. The native host does not discover fonts.
Native preedit/commit events carry the enabled input method's focus session and
composition selection. The editor builds a temporary display paragraph without
changing the document until commit.
Linux IBus Cangjie5/XIM composition and Orca speech generation are verified by native
probes. Windows/macOS input-method combinations need platform verification. Do not infer these guarantees from headless tests.

## Work and native host

Each owner has one next-frame request. The scheduler selects a bounded batch and
revalidates eligibility before each callback; requests during a callback apply to a
later frame. Independent timer handles replace deadlines and cancel on removal.
Visible lifetime is the default; nonvisual mounted lifetime is explicit. Task tickets
carry owner, slot and replacement epoch, checked at admission and delivery.
`WakeHandle::complete` transports root task results through the same ticket checks;
`post` sends ordinary root commands. A replaced file-open request cannot deliver an
older result after its replacement.

The host asks the root's `close_requested` hook before exiting. A root may defer
closing while a save completes, then request another close with `Update::close_window`.
The host pumps outputs from that final hook before exiting and closes worker wake
handles before dropping output handlers, so writer shutdown cannot wait forever on
a full event queue. Only the root can request window closure or native window actions.
`WindowAction` exposes minimize, maximize, drag and edge resize. `WindowOptions`
controls native decorations, font and initial placement. Move events reach the root
as lifecycle data. Widget cursor choices inherit through ownership; native pointer
shapes update without relayout. File drops reach the root, and blocking system file
choosers can run on a platform dialog thread. Keyboard events with no focused child
target the root, so empty applications can still handle their shortcuts.

The native accessibility bridge publishes roles, values, focus and shared geometry
through AccessKit. Windows and macOS use its winit adapter; Linux owns the AT-SPI
transport and uses the published common adapter for tree translation.
It supplies native activation, focus and editor actions, text runs, character geometry
and directed selection. Only changed AccessKit nodes are sent after activation.
The Unix inspection socket exposes the same semantic contract for agents and tests;
see [inspection](inspection.md). It is opt-in and uses a private local socket.
Linux implements all six AT-SPI EditableText operations through one UI-thread request
per operation. Positions are Unicode scalar offsets; the host converts against the
current value and dispatches a shared atomic `ReplaceText` action. Copy preserves
selection; cut checks clipboard success before deleting. Clipboard reads run on a
worker and revalidate the target/value before pasting. Requests are bounded and
expire. The transport derives from MIT-licensed AccessKit Unix 0.23, whose released
EditableText implementation supports only SetTextContents. It stays private to the
native crate; public widgets do not depend on Linux or D-Bus types.

Widget accessibility handlers return a result. Rejected edits preserve state and
successful edits use normal undo history. Successful focused-editor mutations or
selection changes advance the input session, causing the host to restart native
composition and reject stale input.

The host combines ready work, frame demand, repaint and the next deadline. It sleeps
when idle. Worker posts have count/byte limits and wake the event loop. Running a widget does
not require its state or commands to be `Send`; only worker-posted commands require it. Consecutive
pointer moves coalesce before edge events or rendering; paddle position is assigned
directly. Resize reconfigures the surface at paint time. Frame requests are paced to
the monitor refresh rate even when software GL does not enforce swap interval.

Drawing streams through a portable `Painter` on one UI thread. OpenGL is the current
backend. Custom backend painting is an explicit optional capability returning whether
it was handled. Local repaint requests accumulate window-coordinate damage. Layout
changes invalidate the full window. The host retains a framebuffer image and repaints
intersecting branches into it, then copies that image to the swap surface. Decoration
bounds and caret blinking use local damage. No render thread is required.

## Verification and remaining work

The workspace tests exercise real framework instances: removal, focus, held keys,
modal recovery, anchor chains, bounded admission, stale task results, frame fairness,
Unicode edits, paragraph reuse, native shaping and 100,000-item virtualization.
The isolated native probe drives the studio through its own inspection socket rather
than fixed coordinates: it navigates every section by semantic action, presses each
button style, checks a disabled control refuses activation, ticks a checkbox, moves a
slider by pointer, arrow keys and assistive `set_value` (rejecting a value that is not
a number), opens a dropdown and confirms its list lands below the field and outside
the scrolling panel, edits text, filters the 100,000-row list, times visible paddle
response, changes a layout rule and watches the blocks move, swaps the palette, and
resizes wide and narrow — with screenshots throughout. It also discovers a real AT-SPI
button and invokes it, checking that its command reaches the studio.
[Performance results](performance.md) distinguish measured behavior from targets.

Remaining limitations include variable-height virtualization, per-row list-item
semantics (a virtual list publishes itself but its rows reach assistive technology as
their own content, not as selectable entries), Windows/macOS native IME and
screen-reader verification, and large-document layout costs. Geometry changes and visibility reconciliation still contain
whole-tree passes; clean pointer input avoids them. Physical mouse-to-display latency,
hardware-GPU power use and cross-platform behavior require target-device measurements.
These limits are implementation work, not reasons for another framework rewrite.

## Reference study

I inspected these repositories before implementing the new kernel. No framework source was copied into the implementation.

- [Masonry widget protocol](https://github.com/linebender/xilem/blob/b81d8d7a631849def6eeab282561439b963862e5/masonry_core/src/core/widget.rs). Useful reference for persistent widgets, targeted input, and explicit animation requests. The separation from Xilem reinforces keeping high-level composition optional.
- [Iced widget protocol](https://github.com/iced-rs/iced/blob/f728e14a571e2932bd2bee58d7d78ebd39a8422e/core/src/widget.rs). Useful reference for making custom layout, drawing, interaction, and state available to widget authors.
- [egui repaint scheduling](https://github.com/emilk/egui/blob/7809b4fd9a0f52fdda818e9bf080a62329b5409a/crates/egui/src/context.rs). Useful reference for explicit repaint deadlines. Fire UI keeps widget instances alive between events.

The chosen ownership and API are design decisions for this project. They are not claims that one reference framework is universally faster or better than another.
