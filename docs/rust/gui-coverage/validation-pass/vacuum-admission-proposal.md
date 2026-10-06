# Integration status

This report records a read-only review/proposal before integration. Root applied
the proposal afterward; native compilation and execution remain pending. In the
repair review, both the shutdown-test setup and separate vacuum admission findings
have been addressed. Backend selected-file tests and strict repair Clippy pass.
The reports below do not establish final-source hosted or rendered validation.

# Vacuum admission proposal

`vacuum-admission-proposal.patch` is an unapplied diff against the current root worktree. `git apply --check` passed; no source edits, builds, test runs or commits were performed. The base file hashes are in `vacuum-admission-proposal-base.json`.

The proposal keeps two separate predicates:

- `owner_valid` continues to mean that the binding is active and main is visible. The review timer uses only this lifetime predicate.
- `can_accept` means main has no pending question. It gates creating a vacuum confirmation and answering one. A blocked answer returns before consuming the pending decision, so declining main exit leaves the same review/question usable.

The sole production `open` callsite in `lib.rs` supplies both predicates. The existing unit-test helper remains available under `cfg(test)` with unconditional acceptance, avoiding unrelated changes to tests that have no Main owner.

The native regression opens the review through the real menu, starts its vacuum question, asks main to exit, and invokes vacuum Yes. It asserts synchronously that the slot remains owned, the review remains visible and its question is unchanged; wrong admission closes the review before starting the worker, so this negative check does not depend on waiting for absent writes. It also lets the lifetime timer tick to catch accidental permanent retirement on main's question. After declining exit, it accepts the original vacuum question, awaits the actual production SQLite worker's timestamp and completed popup, and verifies persisted state through a reopened Store. Shutdown work is explicitly disabled in this fixture to isolate main exit confirmation.

Compilation and execution of the proposed regression remain pending for the root/hosted GUI checks.
