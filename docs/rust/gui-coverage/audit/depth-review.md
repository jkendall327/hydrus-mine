# Independent GUI depth audit

Static review of the preparatory inventories at `5079206d`. No Cargo, GUI,
reference runtime, or oracle execution; Python and Rust sources were read only.
This checklist identifies work the expanded map must expose, not a completion
denominator or a claim that existing tests pass.

## Acceptance checks for the expanded map

- Every exported Slint `Window` has an assessed, navigable feature occurrence
  with substantive children. A flat catalog or a renamed catch-all is insufficient.
- Non-window surfaces also appear: importer controls, popup cards, thumbnail
  context menus, page context menus, and viewer menus. Exported windows alone
  cannot enumerate the client.
- Trace a visible label through its native callback into the opened editor, then
  inspect the matching reference panel. Record hidden child editors, recursive
  choices, confirmation workflows, and documented missing controls.
- Keep preservation/interchange/runtime support distinct from GUI editing and
  execution. A preserved object with no editor is a missing editor; editable date
  conversion fields with no execution are a partial conversion workflow.
- Evidence must describe a specific supported slice. A test/oracle filename or
  callback does not establish the whole family's `first_pass` status.
- Shared-editor links resolve to canonical depth; incoming alternative entrypoints
  do not create additional implementations or a second denominator.

## Source-backed omissions and assessment risks

| Feature | Required depth or correction | Source anchors |
| --- | --- | --- |
| Main-window popups | Queue/card controls, files, errors/traceback, pause/cancel, dismiss-all; expose unsupported yes/no, callable, and general copy controls separately. | Native `ui/popups.slint:38–48`; reference `ClientGUIPopupMessages.py:230–308`; `DIFFERENCES.md:833–846` distinguishes omitted job popups and network wait fields. |
| Download progress control | Show missing cog and error menus beneath the implemented status/gauge/cancel slice. | Native `ui/importing.slint:19–49`; `DIFFERENCES.md:337–338`. |
| Undo/redo and search undo | Reference-only undo manager, search-term additions, and removals must be visible as missing. Closed-page restoration does not cover them. | Native model `main_menu.rs:493`; reference `ClientGUI.py:3170–3187`, `3227–3258`; `DIFFERENCES.md:270–272`. |
| Session backups | Append-backup chooser, saved session, and timestamp children are missing separately from ordinary append. | Reference `ClientGUI.py:2758–2764`; `DIFFERENCES.md:270–272`. |
| Database locations | Media paths, weighting, maximum size, thumbnail override, rebalance duration; nested in-client and offline granularity migration editors. These are missing native GUI workflows. | Reference `ClientGUI.py:5274–5276`; `panels/ClientGUIFilesPhysicalStoragePanels.py:521–542`, `82–93`, `1116`, `1170`, `1308`; native model `main_menu.rs:701`. |
| File-maintenance scheduler | Scheduled-work clear/run/run-all/refresh; new-work file search, all-media/update-file selection, job chooser, description, scheduling. | Reference `panels/ClientGUIScrolledPanelsReview.py:764–767`, `787–838`, `873–874`; native model `main_menu.rs:709`. |
| Deferred-delete and vacuum review | Queue refresh/work-hard plus selected-database readiness/detail/vacuum, rather than one opaque disabled menu item each. | Reference `panels/ClientGUIScrolledPanelsReview.py:1905–1906`, `2115`, `2133–2175`; native model `main_menu.rs:730`, `736`. |
| String conversion dates | Date decode, date encode, and dateparser controls preserve definitions but cannot execute/preview them. Parent conversion family must disclose this limitation and include missing execution children. | Native model `string_editors.rs:1167–1172`; core `url/strings.rs:440`; reference `ClientGUIStringPanels.py:697–699`, `797–809`; `DIFFERENCES.md:682–686`. Regression `tests/model/string_converter_editor.rs:27–31` explicitly treats unsupported/date results loosely. |
| String processor exchange and matcher favourites | Import/export/paste and regex-favourites menus are missing children beneath existing reusable editors. | `DIFFERENCES.md:674–681`; native `src/string_processor_window.rs:706` also refuses editing unknown processing steps. |
| Subscription/folder logs | File/search logs need independent action depth; opening logs mutates immediately, so parent cancellation does not cover those writes. Search-log PNG/new URL/page-object exchange is missing. | `DIFFERENCES.md:740–745`. |

Paths above omit common prefixes: native UI/model/source paths are beneath
`crates/hydrus-gui/` or `crates/hydrus-gui-model/`; reference paths are beneath
`hydrus/client/gui/`; differences are in `docs/rust/DIFFERENCES.md`.

These findings were sent to the corresponding inventory owners. Final review
must check the integrated map, source anchors, conservative statuses, and actual
canonical navigation; this checklist alone does not certify the final artifact.
