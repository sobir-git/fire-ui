# Inspecting and testing Fire UI

Build the native host with the `inspection` Cargo feature. Native screen-reader
support is a separate `accessibility` feature; neither is needed for in-process
semantic tests.

Widgets publish one semantic contract for assistive technology and agent automation.
A snapshot includes runtime IDs, parent IDs, roles, labels, optional app keys,
window-coordinate bounds, values, focus, disabled/selected/checked state and supported
actions. Editors also publish directed selection and IME composition. Hidden widgets
and widgets outside the current modal scope are absent. A clipped node may have empty
bounds; inspect those bounds before relying on pointer coordinates.

## Run an app with inspection

On Unix, opt in with a socket in a private directory. The host never opens a listener
by default. Use temporary application data when testing editing or deletion.

```sh
(
    inspection_dir=
    trap '[ -z "$inspection_dir" ] || rm -rf -- "$inspection_dir"' EXIT
    trap 'exit 129' HUP
    trap 'exit 130' INT
    trap 'exit 143' TERM
    inspection_dir=$(mktemp -d) || exit 1
    FIRE_UI_INSPECT="$inspection_dir/ui.sock" cargo run --release -p fire-ui-studio
)
```

In another terminal, use the actual socket path printed by your shell or obtained
from the variable above:

```sh
python3 tools/fire_ui_inspect.py /path/to/private/ui.sock
python3 tools/fire_ui_inspect.py /path/to/private/ui.sock '{"label":"Find a material study…","action":"focus"}'
```

Discover names in the first snapshot; an example label is not a selector guaranteed
by every app. Fire Notes publishes the `note-body`, `find-query`, and per-note
`rename-note-ID` keys. Its note editor supports these requests:

```json
{"key":"note-body","action":"set_value","value":"Hello שלום"}
{"key":"note-body","action":"set_selection","anchor":10,"caret":6}
{"key":"note-body","action":"replace_selected_text","value":"world"}
{"key":"note-body","action":"replace_text","start":0,"end":5,"value":"Hello"}
```

Selection offsets are UTF-8 bytes, not Python character indices or UTF-16 offsets.
Use `len(prefix.encode("utf-8"))`. Both endpoints must be selectable boundaries.
`anchor` and `caret` preserve selection direction. An empty selection has equal
endpoints. `replace_text` atomically edits a byte range without selecting it first;
its endpoints may be any Unicode scalar boundary. Edits preserve undo history and
report size-limit failures. IME composition is separate from the committed value.
External edits invalidate the old IME session so a late commit cannot change the new text.

Each connection accepts one JSON line, up to 1 MiB, and returns one JSON line.
`{}` inspects. An action requires `id`, `key`, or an exact `label`; combined selectors
must all match. Ambiguous, missing, disabled and unsupported targets return an
`error`. Add `revision` from a snapshot to reject a request if the UI has changed.
IDs last for the widget's mounted lifetime; use app keys across restarts.

Successful action replies contain the resulting snapshot after bounded processing
of queued UI commands. `pending` reports remaining command work; background saves
may complete later. `paint_pending` means pixels have not yet caught up with state.
Animation can keep requesting paint indefinitely. `window_focused` distinguishes a
focused native window from a remembered widget focus request. Activate the native
window before testing keyboard or pointer input.

The socket uses mode 0600 inside a directory with no group/other access, refuses to
replace an existing path, bounds request size and wait time, and removes its path
on orderly exit. It sleeps in blocking accept while idle. Windows uses AccessKit's
native automation bridge; the Unix inspection socket is not implemented there.

## Build widgets that agents can use

Implement `Widget::semantics` with a meaningful role, action name and state. Supply
`Semantics::key` when identity should survive text changes. Advertise actions in
`Semantics::actions`, then handle those actions in `Widget::accessibility` through
the same operations used by input handlers. Return `Result<(), SemanticError>` so
callers can distinguish an accepted edit from a limit or availability failure. The runtime supplies focus for focusable
widgets and validates action availability. Custom widgets have the same interfaces
as built-in controls.

For behavioral tests, construct `Ui`, pump mount work, lay out with a text engine,
inspect `ui.semantics()`, call `ui.accessibility(id, action)`, pump outputs and lay out
again. `TestText` provides deterministic test metrics. Use `fire_ui_text::Text` for shaping,
Unicode selection and geometry tests. Semantic tests do not establish pixel accuracy,
IME platform behavior or native performance.

For native verification:

```sh
python3 tools/native_probe.py --seconds 2 --accessibility --output artifacts/native
python3 tools/linux_input_probe.py --output artifacts/linux-input
# In the sibling Fire Notes checkout:
python3 tools/inspection_probe.py
python3 tools/notes_probe.py
```

These probes use isolated X11 displays and temporary data. Keep screenshots alongside
semantic snapshots: a correct value and selection do not prove that glyphs, focus
indicators, clipping or animation look correct.

## Linux native accessibility

Linux exposes AT-SPI Text, selection/caret actions and all six EditableText methods:
SetTextContents, InsertText, DeleteText, CopyText, CutText and PasteText. They work
without enabling the inspection socket. Edits resolve the current widget on the UI
thread, preserve undo, reject invalid ranges and respect disabled/modal state. Copy
preserves selection. Cut never deletes on clipboard failure. Clipboard reads run
on a worker; a changed or removed target rejects the pending paste. Requests have
bounded payloads, one pending edit per window and a five-second deadline.

AT-SPI offsets count Unicode scalars; InsertText's length counts UTF-8 bytes. This
is different from the inspection protocol's byte offsets. See the
[GNOME contract](https://gnome.pages.gitlab.gnome.org/at-spi2-core/libatspi/method.EditableText.insert_text.html).

The bridge uses [AccessKit 0.25 text runs](https://docs.rs/accesskit/0.25.0/accesskit/struct.Node.html#method.set_character_lengths).
The private Linux transport derives from AccessKit Unix 0.23 under its MIT license;
the released adapter only implements whole-field replacement. The published common
adapter still translates the tree and native events. Windows and macOS keep their
AccessKit platform adapters and use the same Fire UI semantic contract.

`linux_input_probe.py` uses real installed IBus Cangjie5 through XIM and real Orca
with keyboard echo disabled. It checks composition/commit/cancel, focus transfer,
selection replacement, and Orca's generated utterances for focus, content, caret,
editing, selection and buttons. Speech synthesis/audio output is excluded. On Debian
or Ubuntu, its additional packages are `ibus ibus-table-cangjie5 orca x11-utils
x11-apps python3-pil`; the native probe also needs `xvfb xdotool dbus-x11`. Install
`fonts-droid-fallback` or a supported CJK font and `fonts-noto-color-emoji` for glyphs.
Wayland and Windows/macOS native verification remain outside this Linux pass.
