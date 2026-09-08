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

When unsure about Rust, crate APIs, or platform behavior, research current primary
sources on the web. Check the versions used by this repository; do not rely on
remembered APIs when the ecosystem may have changed.

The UI framework and reusable widgets are the product. Fire Notes, animated text,
and games are consumers that test its usefulness.

Aim for an elegant implementation and an elegant public API. Keep low-level control
available and make higher-level composition optional. Common tasks should read
clearly without exposing runtime bookkeeping. Custom widgets must use the same
supported interfaces as built-in widgets.

Composability and modularity are product requirements. Keep core UI, reusable
widgets and native services independently usable. Applications compose the pieces
they need; convenience APIs must not force unrelated services or policy. Custom
implementations use the same public interfaces as built-ins. Prefer a small set of
coherent components over feature flags for every internal detail.

Keep the framework lean by default. App authors choose optional integrations,
font coverage and retained rendering resources explicitly. Do not impose broad
font loading, platform services, background work or optional dependency stacks
on minimal apps. Keep core semantics available for custom widgets and agent tests.
Measure minimal consumers separately from feature-rich workspace builds; Cargo
feature unification must not hide the cost of optional capabilities.

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

## Repository boundary

This repository owns the framework crates, reusable widgets, native host, studio,
architecture docs and framework probes. Fire Notes lives in `../fire-notes` and
uses these crates through path dependencies. The framework must build and test
without the app checkout. Keep app persistence, actions and visual tokens in the
app repository. Use its original Git history for original Fire Notes references.

Run framework checks here and the app checks when a public capability changes.

## Distribution

Fire UI is MIT licensed. Library versions and the Rust baseline are inherited from
`workspace.package`; internal dependency versions must match. Never publish the
studio. Each crate includes its own README and an exact copy of the root LICENSE.
Breaking design changes remain welcome; increase the 0.x minor version instead of
preserving an obsolete API. Test the actual packages. Keep docs concise; this is early ideation and the design
may change radically. Document usage and constraints without adding process manuals.
