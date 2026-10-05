# Continuation handoff — 2026-10-05

The owner is moving to a new chat. Stop starting additional work in this chat;
the current integration contains the finished in-flight additions.

## Current owner policy

Prioritize implementing the remaining concrete GUI leaves. Use cheap validation:
formatting, strict Clippy/type checking, simple builds and fast targeted tests.
Defer full workspace/native GUI suites, cross-platform runs, rendered review,
mutation testing and exhaustive audits until the owner requests the final project
validation pass. Fix bugs that block ongoing work; record other defects for that
pass. The reference Hydrus remains the specification. Avoid invented convenience
flows, authored real-site URLs, and inert preference controls.

Use GPT 6.1 Sol on High or Extra High for requested parallel subagents, with their
own worktrees. Root integrates and pushes. Keep build artifacts small. Reuse one
current integration CI run, cancelling superseded heads. Remote repositories,
IPFS and account administration remain outside the owner's priorities.

## Merge chain

1. [PR #37](https://github.com/jkendall327/hydrus-mine/pull/37)
   is already merged by the owner into `master` as `4d537a23679dbae5c810b66ecfe2245f021b9a24`.
   Its code source
   `028fd72f4a3cb747e7588d0987bbb99dd83d53f7` passed all four old full CI jobs
   in [run482](https://github.com/jkendall327/hydrus-mine/actions/runs/37242969868).
   The later head `215814c2b6358f66a2b00920359d617e1a5d6c7b` changes only
   inventories, evidence, accounting and offline HTML.
2. [PR #57](https://github.com/jkendall327/hydrus-mine/pull/57),
   `codex/parity-prefetch-and-selected-records`, consolidates all continuation
   work and now directly targets `master`. It is the only remaining PR to merge.
   Its final cheap-check result is linked in the PR description.
   Full validation is deliberately deferred under the policy above.

Historical drafts #43–#56 are superseded; do not merge them separately. Original
feature branches, authored source objects and immutable `gui-evidence/*` tags are
retained. No agent merge into `master` has been performed.

The integration worktree is
`/workspace/parallel/integrate-prefetch-and-selected-records`. The main checkout
is `/workspace/hydrus-mine`; confirm its branch and fetch the owner's merged
`master` before starting new work. `/workspace/HANDOFF.md` is a copy of this note
for the new chat. The PR description records the final source SHA and cheap-check
results.

## Honest accounting

Run `python scripts/gui_burndown.py --commit HEAD` in the current integration.
The frozen original baseline has 862 Missing and 378 Partial entries. Count only
distinct original concrete leaves; exclude parents, shared-editor aliases and
evidence-only changes.

- Fully validated original leaves: **240** (222 originally Missing, 18 Partial).
- Additional implemented original leaf proposals: **134**.
- Combined implementation total: **374**, with those 134 awaiting the final
  full validation/audit; never describe all 374 as fully validated.
- Additional Partial improvements: **63**; excluded promotions: **15**.
- Canonical published map remains 581 Missing, 390 Partial and 841 First pass
  reference nodes. It contains 1,812 reference nodes and 1,733 native nodes/
  97 exported windows. These inventory counts include historical parent/alias
  assessments and are not the concrete implementation count.

The measured ten-hour interval 06:21–16:21 UTC added **59** integrated original
implementation proposals (73→132; 5.9/hour), **8** Partial improvements, and
**0** newly fully validated leaves. The final two implementation proposals and
one Partial improvement landed after that interval.

## Latest completed work and remaining boundaries

Selected-file deletion-record clearing captures eligible physical deletion
records, asks the reference question, and commits independent 64-record batches.
It rejects retained callbacks after owner/page/child changes and preserves
unselected/current/trash/newly deleted files and physical deletion queues.

Viewer prefetch consumes saved previous/next counts and cache percentage through
owned actual static-image consumers. Candidate order, budget admission, pending
readiness and cache cleanup follow recorded behavior. Current-image asynchronous
rendering, video/tile caches and delay controls remain Partial.

Help debug URL fetching uses ordinary network policy, owned network jobs and the
toaster. Responses support binary save, charset-aware clipboard text and forget.
The embedded network widget and shared global scheduling breadth remain Partial.

Sidecar-router import accepts compatible subsets with recorded warnings and
ordered multi-PNG selection, preserving an earlier successful prefix when a
later file fails. Existing export gets no extra completion credit. Native staged
review/Cancel behavior and unsupported/lossy codec cases remain explicit.

Current-session reload saves a private durable snapshot, restores fresh pages and
queues, resets current notebook/thumbnail selection as Qt does, preserves old
Undo pages, and clears temporary snapshots even when delivery retires.

Scheduled file-maintenance review opens real persisted job rows, clears captured
types, runs existing physical jobs through a cancellable worker and publishes
committed results. Integrity redownloads reach the named real importer page.
Ordinary contended passes defer; same-owner Clear/Refresh run between batches.
Mid-batch Refresh, independent-process full-pass locking and popup admission
timing keep this original feature Partial with zero completion credit.

Windows headless cleanup no longer runs blocking Store teardown from TLS
destruction; all initializer guards are retained through their UI scopes. The
watcher fixture now waits for persisted final checker state. Old full Windows
runs stalled; logs do not prove the exact blocked test or exact hang cause.
These repairs have no independent feature credit.

## Environment and validation

Automatic `.github/workflows/rust.yml` now has one cached Linux `quick-check`:
formatting, strict workspace/all-target Clippy excluding `hydrus-gui`, and fast
core/model tests. Generated native GUI metadata checking was stopped after over
12 minutes at about9GiB in one compiler process; it exceeded the cheap budget.
Known GUI API compile blockers discovered by the initial probe were repaired,
but no completed native GUI Clippy/type-check result is claimed. That target
remains in the manual full lane for the final validation pass.
Expensive jobs remain available only via manual dispatch with
`full_validation=true`. Cached Slint source compilation can be run with
`/workspace/parallel/parity-slint-syntax.sh ABS_WORKTREE`; it is not a runtime test.
Serialize actual reference recorders through `/workspace/parallel/with-oracle`.

Git push and the GitHub connector work. Shell `gh api` returned Forbidden even
for an Actions read; the connector lacks a cancellation action. The owner
cancelled queued expensive runs, including720/722. Some obsolete running jobs
may still finish; no new expensive runs should be launched. App PR attachment
returned an unavailable-tool error; ordinary GitHub PR links are usable.

Completed source-only worktrees were removed after clean-state and independently
fetched remote-tag checks; all branches/tags remain. Wrapup uses local strict
backend/model Clippy/type checks and fast core/model tests against the shared cached target,
with isolated workspace compiler wrappers and the current `HYDRUS_FIXTURE_DIR`.
Full native GUI compilation and suites remain deferred. Disk was about6GiB free
at wrapup; check `df -h /workspace` before creating targets/worktrees.

Start the next chat with this file, `AGENTS.md`, `docs/rust/ROADMAP.md` and
`docs/rust/DIFFERENCES.md`. Resume distinct missing leaves from the map while
maintaining the separate implementation and fully validated ledgers.
