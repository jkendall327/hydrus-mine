# Reusable editor and integration audit

The shared patch inspects the native tag-filter, multi-location and sidecar-node
editors against their Python panels at `5079206d`. Existing recordings and test
bodies were read for their asserted scope; no Cargo, reference GUI, oracle or
mutation execution occurred during this audit.

- Tag filtering expands global/namespace whitelist and blacklist interlocks,
  slice entry/removal, advanced exclude/except rules, testing and draft handoff.
  Missing favourites operations, bulk paste, blacklist-only extra panels and
  button tooltips are separate children. The rule-state replay directly compares
  recorded transitions and values; it does not certify all window-lifetime
  boundaries, which remain described in the assessment.
- The shared location selector expands current/deleted domain ticks, combined
  domain exclusion, autocomplete's all-known mode and detached Apply/Cancel.
  Its generic ordinary/autocomplete modes do not represent every independently
  restricted Qt caller flag. Search and migration reference this canonical
  editor instead of copying its subtree.
- Sidecar sources and destinations expand context-restricted kinds, tag-service
  and display selection, timestamp detail, separators, naming/path previews,
  forced note names and JSON object structure. Formula/converter/processor
  buttons link to their assessed shared editors. Router-list exchange, router
  example-file testing and source-processor starting strings remain explicit
  missing or partial work; preserved definitions are not execution claims.

Existing canonical sidecar naming/separator/formula/processing IDs are retained;
new occurrences resolve shared targets. This avoids counting the same editor as
an independent implementation at every entrypoint.

Integration checks examine all exported native Slint Windows, plus the domain
audits' non-window popup, importer, page and media controls. Window exports are
only a completeness check for known native windows, not a reference coverage
denominator. Per-node rationale and evidence relevance are displayed in the
viewer. Truthful unsupported or unassessed leaf work is not promoted merely to
satisfy an inventory check.

Every source/evidence anchor is compared between Git blobs before snapshot
repinning. Identical files or uniquely unchanged source lines/context can be
remapped automatically; changed or ambiguous anchors require human review.
The resulting one-based anchors carry source-line hashes, without copying source
excerpts or real-site example URLs into authored data. Recorded nested/shared
metrics and candidate status comparisons are recomputed from the merged graph.
