# hydrus-search

File search for hydrus-rs: a typed model of everything a search can contain,
and parsers for the Client API's search syntax. Query execution will be built
here later, against the native store.

- `Predicate` / `SystemPredicate` (`src/predicate.rs`): the model. See the
  crate docs (`src/lib.rs`) for the design.
- `parse_system_predicate(&str)`: one `system:` predicate.
- `parse_api_search(&serde_json::Value)`: the `tags` list of
  `/get_files/search_files`.
- `FileSearchContext`, `LocationContext`, `TagContext`: the search domain.

## How it is checked

`oracle/dump_system_predicates.py` runs the reference parser over ~8,000
inputs (every example in the reference tests and the Client API docs, plus
systematic variations of every predicate: operator spellings, units, values,
spacing, underscores, case and invalid input) and ~60 Client API tag lists,
recording what the reference produces. `tests/reference_parity.rs` parses the
same inputs and requires each result to be identical, or to differ in one of
the documented ways below, each of which is checked specifically. Run it with
`HYDRUS_PARITY_VERBOSE=1 cargo test -p hydrus-search --test reference_parity
-- --nocapture` to list every deviation.

Current result: 7,965 inputs; 7,518 identical (4,296 same predicate, 3,222
rejected by both), 447 documented deviations, 0 unexplained. All 60 Client
API tag lists identical.

## Writing predicates

`predicate_text` writes a predicate as the reference writes it (its
`Predicate.ToString` without a count, as a search page lists it):
`system:width > 1,920`, `system:import time: since 60 days ago`,
`character:*anything*`, `a OR b` with the members in the reference's order.
`tests/predicate_text.rs` checks it against the reference's text for every
predicate the reference parsed (decoded from the form it stored, so parsing
differences don't enter into it; 4,604 predicates, all identical) and for
the Client API tag lists. It needs the client's services and viewing options
(`TextContext`) to name services and default canvases. Where our predicates
hold what the reference's cannot, the text extends its wording: an age in
minutes or seconds, a service name no service has (written as named), and
`num file relationships ≠ n` (the reference fails to write it; we write
"has not n").

## Supported syntax

Everything the reference's `SystemPredicateParser` accepts, with the same
leniencies: names are case-insensitive, a space in a name may be written as
any run of spaces and underscores (`system:has_audio`), a colon may follow the
name (`system:width: > 5`), and many operator spellings are accepted.

| predicate | examples |
|---|---|
| flags | `everything`, `inbox`, `archive(d)`, `has/no audio`, `has/no transparency` (`alpha`), `has/no exif`, `xmp`, `iptc`, `human-readable (embedded) metadata`, `software/source metadata`, `icc profile`, `forced filetype` |
| presence | `has/no duration`, `framerate`, `frames`, `width`, `height`, `notes`, `urls`, `words`; `has tags`, `untagged`/`no tags` |
| number tests | `width`, `height`, `num words`, `num frames`, `duration` (`5m30s`, `1 hour`, `600 ms`), `num urls`, `framerate` (`fps`), `num notes` |
| counts | `number of tags > 5`, `number of character tags ~= 3`, `num pixels < 2 megapixels`, `filesize ~= 50 kilobytes`, `num file relationships > 3 alternates` |
| ratio | `ratio = 16:9`, `ratio wider than 4:3`, `ratio is square/portrait/landscape` |
| time | `import time < 7 days`, `modified date > 2011-06-04`, `last viewed time: since 3 days ago`, `archived time: the month of 2020-01-03` |
| views | `media views ~= 10`, `all viewtime > 1 hour`, `views in client api, preview = 5` |
| hashes | `hash = <hex> ...` (sha256, or `md5`/`sha1`/`sha512` anywhere), `similar to <sha256> ... distance 4`, `similar to data <pixel/perceptual hashes>` |
| filetype | `filetype = jpeg, image/png, video` (any word from the reference's table) |
| urls | `has url matching regex …`, `has url https://…`, `has domain …`, `has url with class …` and their negations |
| notes | `has note with name …`, `no note named "…"` |
| ratings | `has rating for <service>`, `rating for <service> > 3/5`, `… = like`, `… = 123` |
| advanced | `has tag in "my tags", ignoring siblings/parents, status pending: "tag"`, `all/any/only <services> (amongst …) (not) rated` |
| other | `limit = 100`, `file service is (not) currently in/pending to <service>`, `tag as number page < 5`, `is (not) the best quality file of its group` |

Time values are a date `YYYY-MM-DD` (or with `/` or `.`), optionally with
`HH:MM[:SS]` after a space or `T`; or an age made of `<n> <unit>` terms
(`year(s)/yr/y`, `month(s)/mo`, `week(s)/wk`, `day(s)/d`,
`hour(s)/hr(s)/h`, `minute(s)/min(s)/m`, `second(s)/sec(s)/s`, `a`/`an` for
1), separated by spaces, commas or "and", optionally followed by `ago`; or
`yesterday`.

Services, URL classes and the default view canvases are kept by name
(`ServiceRef::Name`, `UrlRule::UrlClass`, `ViewCanvases::Default`) and resolved
when the search runs, so parsing needs no client state.

## Deliberate differences from the reference

These are also listed in `docs/rust/DIFFERENCES.md`.

1. **Time values are a documented grammar, not `dateparser`.** The reference
   hands time values to the `dateparser` library, which accepts almost
   anything ("last week", "4 march 2020", "x", other languages). We reject forms
   outside the grammar above: natural language (`today`, `tomorrow`,
   `last week`), day-first/month-first dates (`03/04/2020`), fractions and
   number words (`1.5 days`, `two days`), future times (`in 2 days`),
   `1 decade`, and junk `dateparser` happens to read (`x`, `2011-06-04x`,
   `ago`, `1,000 days` which it reads as 0 days).
2. **Ages stay ages.** The reference decides whether a value is an age or a
   date by looking for the words "year", "month", "day", "hour", "second",
   "ago". So `2 weeks`, `30 minutes`, `1d` or `7h` become the wall-clock time
   at parse time, and `import time < 2 weeks` means "imported *before* two
   weeks ago", the opposite of `< 14 days`. We parse these as ages, so the
   operators mean what they mean for ages. The resulting instant at parse time
   is the same (checked against the reference at a pinned "now").
   `the day of`/`the month of` an age is refused rather than frozen.
3. **Ages are calendar quantities with minutes and seconds.** The reference
   turns `1 month` into a number of days at parse time and rounds to whole
   hours; we keep years, months, minutes and seconds, and the executor
   applies them when the search runs.
4. **Hashes must have the right length.** `system:hash` and
   `system:similar to` accept hashes of any length in the reference (which
   then match nothing); we reject a hash whose length does not fit its type,
   e.g. an md5 without the `md5` keyword. The Client API docs' own examples
   with 8-character hashes are therefore rejected.
5. **Case is kept where the search is case-sensitive.** The reference
   lowercases the whole predicate, so URL regexes, exact URLs and note names
   lose their capitals and can never match mixed-case URLs or note names (the
   reference has dead code intended to prevent this). We keep them as typed.
   Service names, domains and URL class names, which are matched
   case-insensitively, are lowercased as in the reference.
6. **Numbers must fit in 64 bits.** Python integers are unbounded; we reject
   numbers that do not fit (`width > 99999999999999999999`).
7. **An advanced tag predicate with an empty tag is an error.** The reference
   searches for the literal tag `invalid tag` instead.
8. **The Client API `tags` parameter must be a list.** The reference iterates
   a JSON string character by character.
9. **Advanced tag statuses are a set.** A status named twice is stored once.

Not differences, but worth knowing (the reference behaves the same):
`system:filesize > 5 bytes` is rejected (`b` matches first, leaving `ytes`);
filetype words are regex fragments (`image/svg+xml` does not match itself);
whitespace-separated filetypes are silently dropped; `<=`/`>=` on time
predicates mean `<`/`>`; the `rating` and `tag as number` parsers take an
operator word at the end of the service name or namespace as the operator
(`rating for this 3/5` is service "th" with `is`).

## Not supported

- `system:local` / `system:not local` exist in the model but, as in the
  reference, have no text syntax.
- Query execution.
