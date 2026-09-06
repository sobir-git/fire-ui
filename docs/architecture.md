# Fire UI architecture

E2, 2026-09-06. The framework and its reusable widgets are the product. Notes, fire text,
and games must be ordinary consumers of its supported interfaces.

The design centers on **stateful widgets with typed owned children**. Build a component once,
retain its state, and change the affected children. A custom canvas implements the same widget
protocol directly. Composition does not require automatic tree reconciliation, a reactive
engine, a global application reducer, or a native host.

[AGENTS.md](../AGENTS.md) requires no backward compatibility at all. Replace prototype APIs and
update consumers directly. This document supersedes E1's directions and C7's public API choices.
Historical candidates remain evidence, not interfaces to wrap.

## Implementation status

The [E2 Rust experiment](../skeleton/elegant/README.md) implements typed detached construction,
local output mapping, bounded command delivery, removal, coalesced frame demand, and independent
timers. Separate layout algebra exercises intrinsic sizing, baseline alignment, scroll
measurement, and shared appearance. Layout is not connected to rendering or composition yet.

[C7](../skeleton/README.md) remains the reference for lifecycle, input, tasks, overlays,
and geometry. E2 does not yet integrate that router. The running `crates/` framework and studio
are the earlier native prototype. Neither model suite establishes native performance or
completes the framework replacement.

## Public authoring model

A widget declares its `Command` and `Output` types. Commands ask that widget to do something;
outputs report something to its owner. A `Child<W>` names one owned child and fixes its command
type. Type erasure stays private. Ordinary authors need no numeric routing tags, payload downcasts,
receipt matching, or public mutable runtime nodes.

Typed handles and erased type witnesses must be invariant in the widget type. Allowing Rust
function-pointer subtyping to change that type can invalidate the private payload checks.

The [compiled counter consumer](../skeleton/elegant/tests/composition.rs) constructs private children:

```rust
Element::build(|children| Counter {
    button: children.connect(Element::leaf(Button), |()| CounterCommand::Increment),
    label: children.add(Element::leaf(Label(label))),
    count: 0,
})
```

The button output becomes a counter command. The counter changes its own count, sends a typed
label command, and emits a count. A board maps two counters' outputs independently. A picker
handles query changes privately and emits only a selection. Applications need no knowledge of
these components' internal trees.

`Element<W>` is a single-use detached tree. Construction returns typed handles immediately,
without invoking widgets. Publishing a prepared tree cannot expose half its children. E2 installs
synchronously; dynamic insertion and mount notifications remain integration work. The full runtime
must admit a whole prepared subtree, return it intact on rejection, and gate input and work until
Mount completes. Dynamic admission failure belongs at insertion, not routine static child wiring.

Owners retain handles, not mutable child references. `Update` addresses direct owned children,
emits outputs, removes or hides children, and requests work. A stale or foreign handle fails
explicitly. Correct typing alone grants no authority over another owner's children. The root
provides a typed host command entry point and a typed output sink.

## One runtime, restricted phases

There is one ownership tree and one authoritative geometry publication. Internal responsibilities
have narrow owners, within the same runtime:

| Responsibility | Owns | Must not own |
| --- | --- | --- |
| Tree | Identity, parentage, widget storage, mount eligibility, retirement | Rendering policy or app state |
| Delivery | Command/output envelopes, admission, bounded service | A second tree or forwarding queue |
| Input | Focus sessions, per-pointer capture, routes, cancellation, modal scopes | Layout estimates or OS event loops |
| Geometry | Constraints, placement, transforms, clips, published bounds | Independent paint and hit-test trees |
| Scheduler | Frame demand, deadlines, cancellation epochs | Presentation loops or worker execution |

Production storage should use compact slots with stale-handle validation. Identity representation
stays private. Parent links make routes proportional to ancestor depth. The models' maps and global
scans make behavior easy to inspect; they are not suitable performance evidence or prescribed storage.

Widget callbacks receive the capabilities for their phase. Update and input change state and
request effects. Layout measures and places owned children. Paint consumes published layout and
drawing services. Lifecycle reports mount, visibility, focus, capture, and retirement transitions.
Commands do not masquerade as pointer events. Paint cannot mutate topology or trigger layout.

No callback re-enters another mutably borrowed widget. A child output is a distinct envelope;
its typed mapping enqueues an owner command using the vacated queue slot. The owner runs in a
later delivery. Root outputs go to the host sink, without an unbounded core output history.

FIFO orders envelopes, not an entire chain of descendant effects. Owner-level setters must issue
dependent child commands in order; they must not wait for a programmatic update to echo back as
user input. The picker updates search and results directly before issuing a later selection.
Programmatic search updates are silent; actual edits emit query changes. A decision requiring a
child's future result must carry its relevant value/revision or explicitly await that result.
Do not hide this dependency by draining an unlimited callback chain before accepting more input.

Queue admission returns `Full` with the original payload. Stale and ownership failures also return
it. Successful admission does not promise delivery after removal. Widget-local state is not rolled
back if a later send fails. Consumers need a bounded overload policy, such as retaining one latest
value for retry or disabling an unavailable action. Happy-path test assertions are not that policy.
Queue count does not bound payload bytes; byte admission and producer quotas remain implementation gates.

E2 records coalesced callback effects. After the callback it restores the widget, publishes admitted
messages, retires removed children, applies final visibility, then updates work demand. Delivery
resumes afterward, so a command queued for a removed child is cancelled. Repeated visibility or
timer assignments within one callback retain the final value. This is not an ordered transaction
log and cannot undo widget state. Lifecycle integration must publish real transitions once, using
each node's last published state as in C7.

## Layout and visual composition

Sizing belongs to a child's placement. A reusable button can be natural-sized in a toolbar and
fill a sidebar slot. Widgets report intrinsic metrics, including an optional text baseline.
Parent layout negotiates constraints and places children. Decoration contributes insets to
measurement. Overflow and clipping are explicit policies.

Layout helpers borrow child measurements through `LayoutCx`; they do not own another widget tree.
E2's `Measure` trait tests the algebra for that boundary, not the final layout API. A normal scroller
measures its content height at viewport width. A virtual list gets extent from its data/index and
mounts visible rows plus overscan. Callers do not maintain a duplicate ordinary content height.

`Button` owns arbitrary content and adds activation, focus, semantics, and appearance. Icon/text,
spinner, and custom-painted content must work without new enum cases. Decorative children leave
activation to the button; nested interactive content needs an explicit event policy. A keyed list
owns selection/navigation while a row factory supplies arbitrary content. Rows containing action
buttons must distinguish that action from row activation. These integrated controls remain to be built.

An immutable shared theme supplies typography, spacing, and colors. Subtree appearance scopes resolve
those values while preserving narrow local overrides. Controls consume resolved values; custom
widgets can use them or supply their own paint. Colors invalidate paint; metrics and insets invalidate
dependent layout. Theme changes propagate through the affected scope, never a per-frame global lookup.
E2 tests resolution and change classification; actual propagation and caching remain integration work.

Style belongs in the optional widget layer. The core needs no CSS interpreter or universal property
registry. Start with a coherent default appearance and small typed control styles. Custom composition
remains available for applications and game canvases.

## Geometry, input, and text

Ownership and placement differ. A popup belongs to its picker even when placed in a viewport overlay.
Ownership determines cleanup and output routing. Layout publishes accumulated transforms, clips,
bounds, and anchor availability. Paint, pointer conversion, damage, and accessibility use that same
publication. Internal paint transforms affect only local drawing unless child placement declares them.

Re-express C7's useful counterexamples through the replacement API: mount before input or work;
cancel held keys and capture on loss; track each pointer independently; stop an old keyboard route
when focus changes; restore modal focus on close, hide, and removal; publish anchor loss/recovery
once. Retirement first makes a subtree ineligible, cancels its work and input ownership, then
unmounts children before owners, only for widgets whose mount actually ran.

Text uses a shared immutable paragraph result with a revision. Painting, caret navigation, selection,
hit testing, and accessibility consume that result. Editing commands express insertion, deletion,
selection, composition, undo, and redo. A document adapter owns storage; the kernel requires neither
a rope nor Fire Notes knowledge. IME preedit stays separate from committed text and belongs to a focus
session. The host bridges shaping, clipboard, IME, and native semantics to these portable contracts.

## Work and responsiveness

Each widget has one coalesced next-frame request. Two helpers requesting a frame produce one callback
for that widget per presentation opportunity. Parent and child demands remain independent. Requests
made during a frame apply to the next frame. Hidden widgets lose visual demand; visibility recovery
is a lifecycle opportunity to request work again.

Opaque `Timer` handles represent independent delayed operations such as blink and debounce. Rearming
an owner/handle replaces its deadline. Timers default to visible lifetime; mounted lifetime is explicit
for nonvisual work. Removal cancels both. Selected timers are revalidated immediately before delivery,
including their cancellation epochs. Task replacement tickets remain a separate concept and retain
C7's stale-result checks at admission and delivery.

The host receives ready work, frame demand, and the earliest deadline together. It services bounded
work and sleeps when none remain. Frame elapsed time is the presentation interval, not the age of
a restarted animation; games reset or clamp their own simulation clocks on resume. Neither task
completion nor timers require a permanent frame loop.

Drain pending native input before simulation and drawing. Coalesce consecutive pointer moves within
a route session, preserving button/key edges and cancellation order. Set the paddle position from
the latest pointer position immediately; interpolation must not delay direct manipulation. Coalesce
resize bursts to the latest constraints before layout and surface configuration. Reuse shaped
paragraphs while their inputs remain valid. Propagate dirty geometry through affected branches.

Start with one UI thread and streamed painting. Full repaint remains a portable fallback. Retained
pixels are optional and have an explicit memory budget. Add a render thread only when measurements
justify it. Scheduler indexing, total callback service budgets, paragraph cache limits, and producer
byte limits must be enforced and measured in the native implementation.

## Packages and extension boundaries

| Package | Responsibility |
| --- | --- |
| `fire-ui` | Typed widget/child protocol, private runtime, portable layout/drawing/text/semantics contracts |
| `fire-ui-widgets` | Layout composition, content controls, styles, editing adapters, virtual lists |
| `fire-ui-native` | Window/events, presentation, shaping/fonts, clipboard, IME, accessibility bridges |
| Consumers | Notes data/persistence, fire effects, games, optional authoring syntax |

A host or engine can drive the core without the native crate. Backend-specific rendering is an
explicit optional capability with a declared fallback. Ordinary controls do not downcast native
canvases. Built-ins have no privileged route around ownership, input, geometry, or scheduling.

## Completion criteria

E2 settles a smaller set of decisions than a finished toolkit needs. Connect the boundaries in order:

1. Integrate typed construction and delivery with lifecycle, input, and geometry. Re-express C7's
   removal, overlay, focus, capture, and held-key counterexamples through the replacement API.
2. Connect measured layout, shared appearance, arbitrary-content buttons, and keyed rich lists.
   Render the counter and picker. Test width/style changes through actual invalidation.
3. Integrate shared paragraphs and editing, a transformed canvas, fire text, and the paddle game.
   Exercise native IME and semantics adapters rather than only host-neutral types.
4. Repeat idle, resize, pointer response, large-tree, document, and memory measurements. Ordinary idle
   must schedule no periodic wakeup. A 16.7 ms frame at 60 Hz is an initial target, not a result.
   Physical input-to-display latency and target-hardware RAM remain acceptance gates.

The [previous native baseline](performance.md) measured zero CPU ticks in short idle samples
and roughly 125–140 MiB process RSS under software GL. Those numbers describe the existing host,
not E2. Keep useful counterexamples and measurements; remove superseded implementations during
replacement. No compatibility layer is part of this plan.

## Reference study

I inspected these repositories before implementing the new kernel. No framework source was copied into the implementation.

- [Masonry widget protocol](https://github.com/linebender/xilem/blob/b81d8d7a631849def6eeab282561439b963862e5/masonry_core/src/core/widget.rs). Useful reference for persistent widgets, targeted input, and explicit animation requests. The separation from Xilem reinforces keeping high-level composition optional.
- [Iced widget protocol](https://github.com/iced-rs/iced/blob/f728e14a571e2932bd2bee58d7d78ebd39a8422e/core/src/widget.rs). Useful reference for making custom layout, drawing, interaction, and state available to widget authors.
- [egui repaint scheduling](https://github.com/emilk/egui/blob/7809b4fd9a0f52fdda818e9bf080a62329b5409a/crates/egui/src/context.rs). Useful reference for explicit repaint deadlines. Fire UI keeps widget instances alive between events.

The chosen ownership and API are design decisions for this project. They are not claims that one reference framework is universally faster or better than another.
