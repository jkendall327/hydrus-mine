# Deliberate differences from the reference implementation

hydrus-rs matches the reference implementation's observable behaviour except
where listed here. Each entry says what differs, why, and how it is checked.
Add to this file in the same change that introduces a difference.

## Search predicate parsing (`hydrus-search`)

Checked by `crates/hydrus-search/tests/reference_parity.rs` against
`oracle/fixtures/system_predicates.json`: the first six differences are named
outcomes in that test, each verified specifically rather than just tolerated.
The last three are covered by the crate's unit tests.

- **Time values use a documented grammar instead of `dateparser`.** Dates are
  `YYYY-MM-DD` (or `/`, `.`) with an optional `HH:MM[:SS]`; ages are
  `<n> <unit>` terms with an optional `ago`, or `yesterday`. Natural language
  ("today", "last week", "4 march 2020"), day/month-first dates, fractions,
  number words, future times and the junk `dateparser` happens to accept are
  rejected. The test lists every such corpus input.
- **Ages stay ages.** The reference treats a value without the words
  year/month/day/hour/second/ago (e.g. `2 weeks`, `30 minutes`, `1d`) as a
  wall-clock date fixed at parse time, which inverts `<`/`>` ("imported
  before two weeks ago" for `< 2 weeks`). We keep it an age; the test checks
  that it denotes the same instant at the oracle's pinned "now". "The day
  of"/"the month of" an age is refused.
- **Ages keep calendar units and sub-hour precision.** The reference converts
  years and months to days at parse time and rounds to whole hours; we keep
  `CalendarDelta { years, months, days, hours, minutes, seconds }` and apply
  it when the search runs.
- **Hash lengths are checked.** `system:hash` and `system:similar to` reject a
  hash whose length does not fit its hash type (the reference accepts any
  length and matches nothing).
- **Case is preserved for URL regexes, exact URLs and note names**, which are
  matched case-sensitively; the reference lowercases them, so mixed-case
  values never match (its code to prevent this is unreachable).
- **Numbers must fit in 64 bits** (Python integers are unbounded).
- **An empty tag in `system:has tag` is an error** rather than a search for
  the literal tag `invalid tag`.
- **The Client API `tags` parameter must be a JSON list.** The reference
  iterates a string character by character.
- **Advanced tag statuses are a set**, so a status named twice is stored once.

## File lifecycle (`hydrus-store::content`)

Checked by the property tests in `crates/hydrus-store/src/content/tests.rs`
(the umbrella domains always equal their definitions) and by the Client API
conformance runner, which skips exactly the recorded fields listed in
`crates/hydrus-api/tests/known_differences.toml`.

- **"Deleted from anywhere" is always the union of deletion records.**
  Deleting a file from a domain it isn't in (a pre-emptive delete, which
  blocks a future import) records the deletion; the reference then leaves the
  file out of its combined deleted domain until a full resync. We add it
  straight away.
- **File metadata reflects the database, not a stale cache.** The reference
  serves file metadata from an in-memory cache that some writes update
  incompletely until the next restart: purging a file from local storage
  archives it but the cache has no archive time, and clearing a deletion
  record leaves the cache reporting the combined-local-media deletion and
  "deleted from anywhere" membership that the database no longer has. We
  have no such cache and report what the reference reports after a restart.

## Client API input checking (`hydrus-api`)

- **Booleans are not numbers.** `/edit_ratings/set_rating` rejects `true` or
  `false` for a numerical or inc/dec rating service (Python counts a bool as
  an int, so the reference stores `true` as one star).
