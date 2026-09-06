---
name: fire-ui-dry-run
description: Pressure-test Fire UI architecture with concrete interaction traces, small executable models, and adversarial review before committing to expensive framework changes.
---

# Fire UI architecture experiments

Inspired by ai-power-tools' archived dry-run-development skill at
`ff4038d3068d2d63fb686457d8895ad76b59d62f`. The user explicitly asked to adopt
its idea, not its fixed ceremony, reviewer sequence, file checklist, or line budgets.

Treat the framework as the product and notes/games as consumers.
For a disputed boundary, write a concrete interaction that the design must support.
Name who owns state, what calls what, what changes, and what happens on failure.
Build the smallest executable experiment that resolves the uncertainty. A compiled
trait alone does not demonstrate useful composition or correct lifecycle behavior.

Try to break the candidate with nested controls, transformed input, held keys,
removal during queued work, resizing, and scale. Revise the contract and affected
experiments together. Challenge whether each added mechanism can be removed or
replaced with a simpler one that still passes the scenarios.

Use a fresh-context reviewer when an independent attempt to implement or break the
candidate would add evidence. This workflow authorizes bounded review subagents;
it does not require a fixed number of roles or reviews. Inspect relevant Rust
reference mechanisms with exact source pointers. Do not inspect credentials/env files.

Keep the notes needed to explain decisions and reproduce failures. Reuse existing
files; do not create a document taxonomy for its own sake. Preserve rejected
counterexamples and distinguish resolved problems from deferred implementation work.

Convergence requires a complete pass over the chosen product scenarios and a fresh
adversarial pass that produces no unresolved structural problem. New findings reopen
only affected decisions. This is confidence under those scenarios, not universal proof.
Model tests never establish native GPU/IME/accessibility correctness or resource use.
Report implementation and measured-performance limits separately. Do not start another
rewrite merely because a simpler experiment could have answered the question.

Project policy: there is no backward compatibility requirement. Follow `AGENTS.md`.
Do not preserve a prototype API, demo, or earlier candidate merely because it exists.
Retain useful behavioral counterexamples, and change the implementation/API directly.
Correctness convergence does not certify public API elegance or implementation quality.
