# Third parallel slate, 2026-10-04

The slate adds native network usage/rules/jobs, cookie/session/header editing,
clipboard URL monitoring, service-to-service tag migration and downloader
definition JSON/PNG interchange. Subscription Add uses the reference-style
separate gallery chooser, populated from saved definitions. The Python reference
was not modified. `GUI.md` and `DIFFERENCES.md` describe the supported slices and
remaining limitations.

## Validation

- The final full `scripts/check.sh` passed formatting, strict workspace Clippy,
  all 16 packages and the parity ratchet. The GUI integration binary passed all
  215 tests; GUI-model passed 34 unit and 140 integration tests.
- Six new real-reference recorders cover clipboard routing, network usage/jobs,
  sessions/headers, tag migration, subscription Add and downloader interchange.
  Interchange includes reference-to-native and native-to-reference JSON/PNG
  round trips, 27 definitions and 22 older-version vectors.
- Rendered chooser, usage/rules/jobs, migration, cookie/header and interchange
  windows were inspected. Inspection caught adjacent interval labels in the
  bandwidth editor; these now have spacing.
- A real Xvfb desktop smoke imported the basic fixture, opened the client,
  observed a fresh daemon network heartbeat, exited through File > exit and
  verified the daemon cleared its runtime snapshot.
- No mutation tests ran, as requested. Hosted CI subsequently passed Linux,
  macOS and Windows after the Windows follow-up described below.

## Review findings

Independent review addressed 16 concrete findings, excluding lint/type errors
and test-harness fixes: invisible clipboard destinations and hidden clipboard
errors; mutable confirmed migration requests and migration reader exhaustion;
cross-owner definition imports, export text/PNG divergence and bare-filename
exports; malformed legacy upgrades, auxiliary parser metadata misassociation,
bundle-limit boundaries and two parser-linking mistakes; mixed-case header
conflicts and retained session owners; and stale exchange/subscription callbacks.
The reference round trip also caught the named page-parser wrapper format.

## Coordination and environment

Feature work used GPT 6.1 Sol at High/Extra High in separate worktrees. Backend
and model slices were integrated and checked as they arrived; GUI validation was
batched. Heavy Cargo commands shared an exclusive queue and third-party cache,
with distinct workspace wrapper/Clippy paths and per-worktree fixture paths.

The branch began at 02:04 UTC; full validation and the desktop smoke completed
at about 04:06 UTC. Feature deliveries were ready in roughly 50 minutes. Agent
authoring estimates total roughly 120–140 minutes, but are not a measured serial
baseline. A conservative estimate is 30–60 minutes saved overall. Compilation,
integration and repeated lint/render corrections consumed much of the gain;
adding agents cannot accelerate the serialized GUI build.

Two simultaneous GUI code-generation targets exhausted the 16 GiB memory limit.
One GUI build worker passed; CI build/test steps now use one worker. Two workers
were usable for smaller crates and metadata checks. The 32 GiB filesystem had
about 11 GiB free at completion, after reclaiming obsolete GUI libraries. The
optional low-disk cache flag now retains both GUI unit and integration
executables, avoiding unnecessary recompilation on targeted reruns.

Git push works through the configured environment. Direct shell GitHub API
access returned a proxy 403; the GitHub connector supports PR/CI operations.
Xvfb and the real Python/Qt reference work. Chromium runs, but managed policy
blocks direct `file://` navigation; an offline HTML preview can be exercised
with Playwright `set_content` without runtime network requests.

For the next slate, retain parallel authoring and independent review, integrate
backend/model slices immediately, and compile the assembled GUI batch once with
strict linting first. Full per-slice GUI compilation wastes time and disk; waiting
to test every backend until the final batch would delay useful feedback.

The separately requested follow-ups are authored Rust test/example domain cleanup
and a scoped hierarchical GUI migration viewer. They are kept off the slate branch.

## Completed follow-ups

The slate and Windows follow-up were merged as PRs #31 and #32. Windows CI caught
a held-open temporary destination in a PNG overwrite test and a production export
filename split that recognized only the native separator. The fixes close the test
handle before replacement and recognize both Windows path separators, retaining
the reference's later directory-sanitization behavior. The fixes passed hosted CI
on all three platforms.

Authored Rust test/example download URLs now use synthetic domains. The search
coverage test derives its expected URL domain from the reference fixture, keeping
positive coverage without hardcoding a real site. Python reference/oracle fixtures,
specification namespaces and actual application/help links remain unchanged.
Full local formatting, strict workspace Clippy, all 16 packages and the parity
ratchet passed after this cleanup and the Windows fixes, including all 215 GUI
integration tests.

The expanded GUI map audits all 66 exported native windows and important
non-window surfaces, with 1,812 reference and 1,277 native nodes. All 38 reference
option tabs and 19 system-predicate groups are exposed; Options has 644 nested
entries. Source audits and an independent depth review replaced the opaque native
window catch-all with concrete controls, workflows and shared targets. Evidence
review downgraded 28 source-only or overbroad native claims to partial; retained
green claims cite the relevant existing assertion/recording scope. This is a
finite semantic inventory, not an enumeration of every raw widget or a client
completion percentage. Source fingerprints, graph/count validation and desktop/
mobile browser checks passed; the inventory audit itself did not execute behavior
tests. The generated viewer also works through a localhost HTTP preview when
managed browser policy blocks direct file navigation.
