# Changelog

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
