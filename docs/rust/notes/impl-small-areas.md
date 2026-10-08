# Small workstreams: tags-services, media, shell, editors, network, duplicates, files-io

Branch `claude/impl-small-areas`. Of the 55 listed leaves, 14 are now tagged.
Most of the "missing" states were stale: the code existed and lacked a test
that drives it as a user would. Four needed real code (the first four below).

## Tagged (test drives it as a user would)

Implemented in this session:

- `session-create`: the new-session editor offers web domain or hydrus
  service (repository choice, reference notes); test in `network_sessions.rs`.
- `audit-media-services-missing-api-capture`: "add from api request" waiting
  window, request taken from the daemon via the store, "Got request!", the
  permissions editor on the tool's key. `Registration` moved to hydrus-store.
  Test in `client_api_admin.rs`.
- `audit-network-gugs-delete-dependencies`: the reference's sequential
  questions (`Remove all selected?`, then each `_DeleteGUG` warning; a "no"
  stops there). It had been one combined question. Model + GUI tests.
- `audit-media-rules-exchange`: rule-list exchange was already there (tested
  now); the rule's comparator list gained export/import/duplicate, including
  reference-serialised comparators. Model + GUI tests.

Already implemented, now tested:

- `audit-media-preparation-scheduling`, `audit-media-rule-sidebar-scheduling`
  (switches gate by the published idle state; the daemon loops already did).
  The test replays the daemon's decision (`pace(is_idle(..))`), not the loop.
- `audit-media-filter-sidebar-random`, `audit-media-review-progress` (popup
  after 4 s, progress text, dismissal; the reference's job is not
  cancellable; `action_pairs_after` makes the wait injectable),
  `audit-media-ratings-missing-count`, `audit-media-force-rename` (copy
  fallback queues the cleanup job), `audit-network-import-folder-filetypes`
  (with the existing skip-period test), `audit-network-pause-nudge` (GUI click
  plus `QueueRunner::take_nudges`, moved out of `main.rs` to be testable),
  `audit-shared-sidecar-export` (export + duplicate), `url-links-api`.

## Not tagged, and why

- Platform (Qt/Python/OS): `audit-media-viewer-drag` (Slint cannot start a
  drag), `audit-options-file-tray` (no tray), `audit-options-about-*`,
  `audit-options-menu-menu-help-about` (Python/Qt census),
  `audit-options-popups-freeze` (minimised part works and is tested under its
  options leaf; "mouse on other monitor" needs a global cursor position Slint
  lacks), `audit-shared-tag-tooltips` (no tooltip element in Slint; an
  app-wide decision), `audit-options-geometry` (screen fitting needs monitor
  info), `subscriptions-exchange` (clipboard image/drag-drop).
- By design: `tag-display-sync`, `parents-async`, `siblings-async` (native
  rebuilds display immediately; there is no background manager or slow fetch).
- Too large (over half a day): `audit-media-tags-missing-cog` (needs the
  tag list's selection, a remove/petition button and the cog),
  `-repository`, `-viewer-follow`, `-suggestions`,
  `import-external-program-entry` (typed model, editor and executor),
  `exchange-domain`, `audit-network-exchange-unsupported` (domain metadata
  and login script packages), `audit-media-duplicate-search-count`,
  `audit-options-popups-modal`, `audit-options-menu-menu-file-options`
  (18 Options tabs), `audit-media-permission-commit-pending` (no repository
  upload), `audit-media-services-missing-listener-unsupported`.
- Partial and left so: `audit-options-status-activity` (idle and CPU-busy
  fields exist; the DB-activity field has no source), `audit-media-viewer-playback`
  (video needs libmpv; not verified here), the autocomplete, dateparser,
  import-locations, location-deleted, services-counts, star-appearance,
  relationship-reasons and export-examples leaves.

## Workflow friction

- First build: deps ~13 min plus the UI crate ~7 min. Three `.slint` batches
  cost three UI rebuilds (12, ~7 and 5.6 min). One was wasted: I edited
  `.slint` while a build ran, so it had to be redone. Finish all Slint edits
  first.
- `dev.sh gui` after that: 10-60 s. First Clippy ~10 min.
- `dev.sh lint` with no crates says "no changed crates" once committed; name them.
- A destructured `let Opened { store, .. } = opened();` drops the temp dir at
  once, so `store.dir()` points at nothing; bind `_dir`.
- Many leaf states in `leaves.json` are stale; check the code first.

## Verification

Full GUI suite: 925 of 926 pass. `emoji_fonts::outline_fox_is_selected_...`
fails here on a font family id (this container's fonts); none of my changes
touch fonts. Model, download, api, store, legacy and cli tests pass; strict
Clippy and `cargo fmt --check` are clean.

## Final section (after the coordinator's review and change of plan)

Finished since the last section:
- Merged `claude/pensive-darwin-kvjhpq` (clean).
- Review findings 1 and 2: a hydrus-service session with no repository chosen
  is refused ("Choose a hydrus service for this context."), with a test;
  `context_type_info` moved off `validate_context_domain`'s doc comment.

Not done (the coordinator stopped new work; each is a small slice):
- Finding 3 (~1 h): the API-request registration opens for a fixed hour. Make
  it a short expiry renewed by the waiting window's timer so a crashed GUI
  can't leave it open; mind `hydrus api-keys listen`, which shares the row.
- Finding 4 (~30 min): the comparator png import (mode 3) should use the
  reference's wording for a png source, not "clipboard". I checked
  `COMPARATOR_TYPES` against `HydrusSerialisable.py` (130, 131, 137, 138, 140,
  141, 152 are right); the test helper still reuses that list and should parse
  the Python constants instead.
- Finding 5 (~half a day): `audit-media-review-progress` should be driven from
  the sidebar approve/deny buttons (or untagged); the two scheduling tags
  replay `pace(is_idle(..))` and should run one daemon pass with an injected
  idle state (or be untagged); the sidecar-export test name claims a read-back
  it doesn't do (rename or add the import). Until then treat those four tags
  as weak.

Remaining, not started: the manage-tags cluster (`audit-media-tags-missing-cog`,
`-repository`, `-viewer-follow`, `-suggestions`, `-autocomplete`; 2-3 days with
one or two UI rebuilds, needs a selectable tag list and a remove button first)
and `audit-media-duplicate-search-count` (~1 day). Nothing was started and
dropped.
