# Fire UI architecture

The framework and reusable widgets are the product. Fire Notes and games consume
its supported interfaces. The replacement is implemented directly in `crates/`.
Previous frameworks and executable design models exist only in Git history.
[Project rules](../AGENTS.md) explicitly reject backward compatibility.

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

The [studio](../examples/studio/src/main.rs) contains the complete counter, composed
panel, picker and custom canvas consumers. None has access to private runtime nodes.
The picker owns search/filtering; its parent receives a selected key.
`Menu` accepts action keys, labels, shortcut hints, checked and enabled states.
It owns keyboard navigation and accessible action rows; its parent receives the
chosen key or dismissal and controls the modal overlay. Fire Notes uses it for
tab actions without adding app commands to the framework.

`Children::add` accepts children that cannot emit output. `connect` maps a borrowed
output to an owner command; `forward` transfers an owned output unchanged; `discard`
explicitly ignores output. Detached subtrees are admitted atomically under node and
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
widget state. Consumers must handle overload. The studio retains latest counter-label
and picker updates for retry; applications must choose their own policy for critical
outputs. `Data::bytes` is a trusted accounting contract, not a sandbox for arbitrary
widget code or a bound on document/model storage.

## Layout and composition

Children report intrinsic size and optional baseline under constraints. Parents
choose placement, clipping and transforms. Row/column helpers borrow child handles;
they do not build a second tree. Padding contributes insets. Scroll measures its
content at viewport width and derives the extent.

Buttons own arbitrary ordinary content and add activation, capture, focus, semantics
and appearance. Decorative content leaves activation to the button. A virtual list
accepts a row factory and unique stable keys, keeps visible rows plus overscan mounted,
and owns selection/navigation. Selection is published to row content through
`ListRowState`; optional hover selection supports pickers. Navigation can be driven
by an owner while focus remains in its search field. Its fixed row height is explicit. Key indexing avoids
scanning the entire dataset while laying out visible rows.

Immutable theme values propagate through an affected subtree until a nested explicit
scope. Local label appearance can override typography or color. Metric changes
invalidate layout; color changes repaint without reshaping text. Clean pointer events
skip geometry traversal, and valid paragraphs survive height-only resizing.

## Input and text

Input routes through preview, target and bubble phases. Each pointer has independent
capture. Focus sessions invalidate stale keyboard routes and paste delivery. Modal
scopes restrict input and restore eligible focus on close, hide and removal. Anchored
overlays publish their dependencies before their own geometry.

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

International text remains a prototype limit: grapheme editing is tested, but complete
mixed-direction caret/selection behavior and broad font fallback are unfinished.
Native preedit/commit events carry the enabled input method's focus session;
real input-method combinations need platform
verification. Do not infer these guarantees from headless tests.

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

Fire Notes keeps an editor subtree for each open tab and metadata for other notes.
It loads closed notes on demand. Raw file content is independent of title metadata;
per-tab editor state and window placement live in the session. One worker batches the latest save snapshot per
note and writes through a temporary file and rename. Revision acknowledgments
prevent an older save from marking newer text clean. Unsaved closed notes retain
their body until the latest write completes. This is local single-writer storage;
external change detection and conflict resolution are not implemented.

The native accessibility bridge publishes roles, values, focus and shared geometry
through [AccessKit's winit adapter](https://docs.rs/accesskit_winit/0.33.2/accesskit_winit/struct.Adapter.html).
It supplies native activation/focus actions and avoids building semantic trees while
accessibility is inactive. Full editable-text accessibility is still incomplete.

The host combines ready work, frame demand, repaint and the next deadline. It sleeps
when idle. Worker posts have count/byte limits and wake the event loop. Running a widget does
not require its state or commands to be `Send`; only worker-posted commands require it. Consecutive
pointer moves coalesce before edge events or rendering; paddle position is assigned
directly. Resize reconfigures the surface at paint time. Frame requests are paced to
the monitor refresh rate even when software GL does not enforce swap interval.

Drawing streams through a portable `Painter` on one UI thread. OpenGL is the current
backend. Custom backend painting is an explicit optional capability returning whether
it was handled. Full-window repaint is the current fallback; there is no retained
framebuffer or render thread. Active software rendering is still expensive.

## Verification and remaining work

The workspace tests exercise real framework instances: removal, focus, held keys,
modal recovery, anchor chains, bounded admission, stale task results, frame fairness,
Unicode edits, paragraph reuse, native shaping and 100,000-item virtualization.
The isolated native probe checks counter independence, editing, picker filtering,
animation/pause, visible paddle response and window resizing, with screenshots at
wide/narrow sizes. It also discovers a real AT-SPI button and invokes it, checking
that its command reaches the counter.
[Performance results](performance.md) distinguish measured behavior from targets.

Remaining limitations include variable-height virtualization, complete bidi editing,
platform IME coverage, full screen-reader interaction coverage, and partial repaint for
large/animated documents. Geometry changes and visibility reconciliation still contain
whole-tree passes; clean pointer input avoids them. Physical mouse-to-display latency,
hardware-GPU power use and cross-platform behavior require target-device measurements.
These limits are implementation work, not reasons for another framework rewrite.

## Reference study

I inspected these repositories before implementing the new kernel. No framework source was copied into the implementation.

- [Masonry widget protocol](https://github.com/linebender/xilem/blob/b81d8d7a631849def6eeab282561439b963862e5/masonry_core/src/core/widget.rs). Useful reference for persistent widgets, targeted input, and explicit animation requests. The separation from Xilem reinforces keeping high-level composition optional.
- [Iced widget protocol](https://github.com/iced-rs/iced/blob/f728e14a571e2932bd2bee58d7d78ebd39a8422e/core/src/widget.rs). Useful reference for making custom layout, drawing, interaction, and state available to widget authors.
- [egui repaint scheduling](https://github.com/emilk/egui/blob/7809b4fd9a0f52fdda818e9bf080a62329b5409a/crates/egui/src/context.rs). Useful reference for explicit repaint deadlines. Fire UI keeps widget instances alive between events.

The chosen ownership and API are design decisions for this project. They are not claims that one reference framework is universally faster or better than another.
