# Integration status

This report records a read-only review/proposal before integration. Root applied
the proposal afterward; native compilation and execution remain pending. In the
repair review, both the shutdown-test setup and separate vacuum admission findings
have been addressed. Backend selected-file tests and strict repair Clippy pass.
The reports below do not establish final-source hosted or rendered validation.

# Independent repair source review

Reviewed the current root working-tree diff and relevant consumers/tests read-only. No builds, test executions, source edits or Git mutations were performed. Root reports backend focused tests and strict Clippy passing; this review does not independently certify those runs. GUI hosted compilation/tests and render review remain pending.

No definite new compile/API blocker was identified from source inspection. The bounded source-copy/lease repair and move-journal repair are coherent. Two GUI points should be resolved or explicitly accepted before interpreting hosted results.

## Findings requiring a decision

1. **P2 — isolate shutdown settings in the new Database maintenance exit test.** `crates/hydrus-gui/tests/gui/database_maintenance.rs:322` sets `GuiSettings.confirm_exit=true`, but leaves `ShutdownWork` at imported basic-fixture values. `oracle/fixtures/legacy_db/basic.expected.json:1969` records `idle_shutdown=2`; `crates/hydrus-store/src/import/decode.rs:552` imports that setting, with `last_done=0`. If `shutdown_work::work_due` reports any table without planner stats (`crates/hydrus-store/src/db_maintenance.rs:176`), `ui.invoke_answer(true)` opens the maintenance question rather than finishing exit. The assertions immediately afterward that the maintenance child retired are then premature. The vacuum ownership test explicitly sets `ShutdownWork.action=0`, and the comparable client-exit unit setup does too. Set action zero here to isolate the accepted-exit boundary, or explicitly answer the maintenance child. This is a source-identified setup risk; hosted execution is still pending.

2. **P2 — vacuum admission permits a pending main exit question.** `crates/hydrus-gui/src/lib.rs:3006` supplies a valid-owner predicate that checks binding activity and main visibility, while Database maintenance `State::permits` at `database_maintenance_window.rs:44` also requires `main.get_question().is_empty()`. Therefore a visible vacuum review with a pending vacuum question can accept Yes and admit a fresh worker while main exit confirmation is pending. The vacuum integration test at `vacuum_review_window.rs:604` declines main exit before interacting with the review, so this interval is not covered. If maintenance admission should follow the Database policy, separately gate input/answers while main has a question. Do not put the pending-question condition into the timer's permanent-retirement predicate: that would close the child immediately and violate the existing intended behavior that declining exit preserves it. Add an assertion for a child answer during pending main exit, then decline exit and verify the original review remains usable. This is a consistency/policy gap in the new owner checks, not evidence of a failed hosted run.

## Bounded safety and verification caveats

- **Restore is still non-atomic after source staging, as documented.** `backup.rs:262` stages/syncs the database before touching the original database, fixing the initial-copy failure path. `backup.rs:267` then mirrors directly into existing media, and `backup.rs:269` removes sidecars before `NamedTempFile::persist` at line 277. A later mirror/install failure can leave changed media or lose old WAL recovery state. `a_failed_restore_copy_preserves_the_database_sidecars_media_and_request` at line 405 injects only the initial staged-copy failure; it does not establish later failure atomicity. Root explicitly acknowledged this limit; `docs/rust/DIFFERENCES.md:3581` documents manual recovery for later failures/crashes, and restore remains Partial with no completed-leaf credit. I found no new regression versus the old destructive restore implementation. A future practical repair can stage media beside its destination and retain original media/database/sidecars through a journaled replacement, but a transactional media restore is outside this bounded batch.
- **Granularity rollback can itself fail.** The forward copy path cleans a partial destination; `undo_moves` at `granularity.rs:282` still uses plain copy/remove at lines 294–295. Failed undo copying can leave a partial original path while the complete migrated copy remains at the new path; failure stops remaining undos. Failed remove can leave duplicates. This is now explicitly covered by the manual-recovery limitation in DIFFERENCES, not a guarantee of complete recovery under a second I/O fault. A small future improvement is to stage each cross-device undo copy, remove its partial staging file on failure and publish only a complete original. Existing split-location/cancel/publication tests use normal local filesystem moves, not forced cross-device undo failures.
- **Negative asynchronous assertions are time-bounded.** New `database_maintenance.rs:46` waits 100 ms before asserting absence of Store writes/jobs. A wrongly admitted but delayed worker can outlive that observation. The source admission guards are meaningful, and positive tests await actual persisted consumers; do not treat a short no-output wait alone as exhaustive cancellation proof. A deterministic admission observer or worker completion barrier would provide stronger negative evidence.
- **Vacuum positive unit test uses an injected synchronous accepted consumer.** It runs real `vacuum::vacuum` and verifies persistence, but bypasses production `run`'s thread/popup completion at `vacuum_review_window.rs:183`. The menu test verifies owner retirement and no persisted timestamp after stale input. Neither establishes every production worker/popup failure path; retain the existing Partial scope.
- **Auto-resolution failure test covers denial before any commit.** The injected trigger in `tests/gui/auto_resolution_review.rs:443` rejects the first denial update. The new code also sensibly reloads actual pending state after partial approval/denial chunk failures, but multi-chunk partial commits and failure during reload are not established by that single assertion.

## Reviewed file outcomes

| File | Source assessment |
|---|---|
| `backup.rs` | Initial source-copy preservation, retained request, same-store rejection and serving-lease refusal are coherent; later destructive stages remain documented Partial. |
| `granularity.rs` | Physical outcome map wins; old map fills only missing prefixes. Exact reverse journal handles pre-publication filesystem errors and transactional publication errors. UTF-8 directory-name panic is fixed. |
| GUI `main.rs` | Creates directory and takes GUI lease before pending restore or Store opening. Existing lifetime through startup/restart remains intact. |
| `auto_resolution_review_window.rs` | Worker error is synchronized before release/acquire completion, reloads pending state and marks completed/denied tabs stale instead of deleting all requested rows as successes. |
| `client_exit.rs` | Weak child backedges plus owned timer avoid the previous forgotten timer. Cancel/X abandon exit; No/timeout register and continue; current-owner and visibility checks reject stale answers. |
| `database_maintenance_window.rs` | Serial generations, weak Store/state backedges and current visible-owner checks prevent predecessor callbacks from closing/admitting successor decisions. One-choice auto-selection preserves existing semantics. |
| `vacuum_review_window.rs` | Binding-scoped family and final shared Bound owner replace global ownership; pending/eligible question consumption prevents duplicate admission. Pending main-question policy gap remains above. |
| `set_password_window.rs` | Current slot identity, active latch and mode checks prevent out-of-phase answers and hidden/replaced/closed retained callbacks from saving. No new API or obvious RefCell reentrancy defect found. |
| `network_job_control.rs` | Captured window identity prevents an old error close callback from taking the successor slot; callbacks hide only their own window. |
| GUI `lib.rs` / `menu_bar.rs` | The only Hooks constructor supplies the new vacuum hook; rebind/accepted exit and last shared Bound owner retire the new slots. No unresolved old Slot/default or vacuum-open callsite found. |
| model `main_menu.rs` | Only test expectations changed: production restart was already enabled, and closed-page offsets now account for content undo/redo entries. New disabled/unsupported cases remain explicit. |

## Reviewed source fingerprints

These identify the inspected worktree contents; they are not publication Git anchors. Later root edits require a delta review. GUI CI/render completion is still required.

- `crates/hydrus-store/src/backup.rs`: `67df24906a96cc7ac5905a2484b9f6c2aacf0e16a8eb0889adbf079c0234c089`
- `crates/hydrus-store/src/granularity.rs`: `996e46ec0d01d6748227c4234f866926d239e786ef362d6f892e2c92e61c15c7`
- `crates/hydrus-gui/src/main.rs`: `ac34356039c9b71d7d25f26efb27ab0683b1d2b76b9c0ce1a03e029a1e70d9d2`
- `crates/hydrus-gui/src/auto_resolution_review_window.rs`: `70763776a2a7e3d12bf205f4aef25af1357155b59d17725ddeacaa97eb01357f`
- `crates/hydrus-gui/src/client_exit.rs`: `10c06520fb9108c24a22c4efbcaf5158f6cb672b968ddf551864f7cffaa6b2fd`
- `crates/hydrus-gui/src/database_maintenance_window.rs`: `cb31974782b82a79387c192dd7505f351985df965b436d9e2c26869d3bb3f29e`
- `crates/hydrus-gui/src/vacuum_review_window.rs`: `1dfee1e33836fc9659b9493e15bcd492617bb6552e336a6cbbcfa44b35678d6f`
- `crates/hydrus-gui/src/set_password_window.rs`: `32c8a42426d16d5cd5033ba9fd7c5f4e004ee0bb1344aab0de16e11f5ac4d48b`
- `crates/hydrus-gui/src/network_job_control.rs`: `58f5103bd7b42a0fdc8cfdc7bf6cc48a01fa3a92a29b8c6b5c3a7bdadde885da`
- `crates/hydrus-gui/src/lib.rs`: `296bab006e6e6d543bb3b35a724edc7146a6b2b99ad8b747e02350388d5f9c28`
- `crates/hydrus-gui/src/menu_bar.rs`: `0c25eeb1e01301c6654a5b4c78c009a18e5943864fc0125437f474f2dbe5df03`
- `crates/hydrus-gui-model/src/main_menu.rs`: `b343cd00b1b985e76575905e3e6163ca9d4e4636e51d839b0388e6adc724b662`
