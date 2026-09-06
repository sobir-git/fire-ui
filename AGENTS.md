# Fire UI project rules

## No backward compatibility

This project is in design and prototype development. There is **no backward
compatibility requirement at all**. Break, replace, rename, or remove prototype
APIs, widgets, formats, crate boundaries, and examples when the better design
requires it.

Do not add compatibility shims, deprecated aliases, parallel old/new APIs, or
migration machinery to preserve an earlier prototype. Update consumers directly.
Keeping an existing demo running is not a constraint on architecture decisions.
Remove superseded frameworks from the working tree. Git history is their archive;
do not keep legacy directories, parallel implementations, or executable design skeletons.
Previous implementations are evidence to examine in Git history, not contracts
that future work must preserve. Preserve product requirements and useful behavioral
counterexamples, not historical implementation choices.

## Product and quality

The UI framework and reusable widgets are the product. Fire Notes, animated text,
and games are consumers that test its usefulness.

Aim for an elegant implementation and an elegant public API. Keep low-level control
available and make higher-level composition optional. Common tasks should read
clearly without exposing runtime bookkeeping. Custom widgets must use the same
supported interfaces as built-in widgets.

Treat fast pointer response, fast resizing, low idle CPU, and low memory use as
design requirements. Measure them; a passing contract model does not prove native
performance or platform correctness.

Use architectural experiments and independent review when they answer a concrete
question. Adapt the dry-run skill's reasoning; do not follow its ceremony blindly.
Prefer a coherent smaller design over accumulated special cases or speculative
generality. Passing correctness tests alone does not establish design elegance.

Fire Notes must reproduce the original app's compact black-and-orange UI and useful
behavior on the new framework. Animated fire on typed and selected text is mandatory.
Use the original in Git history as visual and interaction evidence. Close framework
gaps revealed by the app through reusable public capabilities, without restoring
old framework implementations. Verify real native interactions and screenshots.
