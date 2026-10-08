# Plan of attack

How the remaining work is ordered and parallelized. Revise it as evidence
comes in; `docs/rust/WORKPLAN.md` records the evidence.

## Where things stand (2026-10-08)

Of 1,274 leaves: 385 done (carried over), 458 `implemented` (written but never
verified against the reference), 116 `partial`, 308 `missing` (77 of them
Help > debug, low priority), 7 out of scope. The states predate several days
of work: the first leaf checked (`audit-media-context-missing-stats`) said
"missing" and was fully implemented, lacking only an end-to-end test.

## Order

1. **Verification sweep, every workstream at once.** For each `implemented`
   leaf, and each `missing`/`partial` leaf whose code turns out to exist: find
   or write the test that proves it against the reference, fix what is wrong,
   tag. This is mostly test code in new files, so agents rarely collide, and it
   converts existing work into counted, trustworthy progress. It is also where
   real bugs surface (the old review demoted half of one batch's claims).
2. **Daily-use gaps next**, by user value: search pages and their sidebar
   (`search-pages` partials), per-file actions (`media`), `duplicates`,
   `files-io` (import/export folders), `tags-services` (siblings, parents,
   migration, service review).
3. **Options pages with real consumers** (`options-system`, `options-gui`,
   `options-media`). An option counts only when the setting changes behaviour
   (its consumer), not when the control exists. All options share
   `hydrus-gui-model/src/options.rs` (`pages()`) and the store's `Settings`;
   run at most two options agents at once, on different pages.
4. **Database menu** (`database`). Much is backend work in `hydrus-store`. The
   native store has no copies of several reference caches (ADR-1 in
   `ARCHITECTURE.md`); for those, the native action may be different or not
   applicable. Decide per leaf; record "not applicable" leaves by setting
   their `priority` to `out-of-scope` in `leaves.json` with a note, and say why
   in `DIFFERENCES.md`.
5. **Help > debug** (`help-debug`, low priority) last.

## Parallelism

- One workstream per agent. Workstreams are drawn so their files rarely
  overlap; `README.md` lists their main files.
- **Memory is the binding limit on one 16 GB machine**: the generated UI crate
  needs ~14 GB to build. Agents on one machine share a checkout (or at least
  one `target/`), so Cargo's lock serializes builds; batch `.slint` edits,
  since each one rebuilds that crate for ~7 minutes while everyone waits.
  Model-only work never needs it.
- Three agents per machine is a reasonable start. More need more machines
  (separate cloud sessions, each with its own checkout and ~25 minutes of
  first build).
- Shared hot spots take line-sized edits only (`AGENTS.md`).

## Review

One independent review per merged batch (see `AGENTS.md`): diff against the
reference, honest tags, renders of changed windows.
