# Network, downloader and import/export GUI source audit

Primary source snapshot: `5079206dc1d0c2d2614b428632dc9499cb9ad3f8`.
Supplemental export source snapshot: `98f32b714a772d84b703100e0972fd6e32f97f5e`.

`network-patch.json` contains full updated nodes and complete additions against the
original canonical inventories: **119 native updates + 153 additions**, and
**135 reference updates + 132 additions**, with no deletions. It preserves existing
IDs, snapshot metadata, global totals, `catalog.other`, and original provenance.
The integration owner applies the patch and refreshes aggregate counts and source
pins. Every touched node has a plain-string assessment and specific source anchors.

The audit traces reference controls, native widgets/callbacks, drafts, storage writes,
and relevant workers or engines. Existing oracle/regression assertions were read;
no Cargo, Qt, GUI, oracle recording or mutation tests were executed. `first_pass`
describes the stated finite slice, with limitations retained on its node. A test
filename, visible button, stored field or preserved serialized object does not
establish the complete workflow.

| Area | Expanded assessment |
| --- | --- |
| Network menu and clipboard | Persistent network/subscription/file/gallery/watcher gates reach worker scheduling; advanced nudge reaches the daemon. Clipboard switches persist independently and route changed recognized URLs. Boot-paused preference, display settings, lookup scripts and login editors are commandless placeholders. |
| Bandwidth and live jobs | Context sorting, history spans, inherited rules, positive byte/request limits, rolling/monthly intervals, conflict-safe Apply, confirmed default reset, live refresh, cancellation and bandwidth override are distinct slices. Job commands identify daemon epoch and request ID. Monthly charts, history deletion and reference usage/rule filters are missing. |
| Sessions, cookies and headers | Browser, cookie draft, header draft and reusable value editor now have separate children. Cookie identity/expiry/security and header context/value/approval/reason cite persistence and real outgoing-request assertions. Cookie clipboard/Netscape exchange and automatic pending-header approval are missing. Session creation is partial because reference creation also allows Hydrus-service contexts. |
| Subscriptions and logs | Add chooser, merge/primary/name, separation, dedupe survivor choices, pause/check decisions, retries/reset, overwrite and checker controls are explicit. Grouped merge cancellation differs from Qt. Quality report/CSV, serialized exchange and import-options clipboard operations are missing. Existing file/search logs write immediately, so parent Cancel cannot discard those changes. |
| File-log menus | Whole-log, selection and metadata submenus distinguish working retry/skip/delete/reverse/show/open/copy actions from active menu entries that do nothing. Clipboard import and URL search reach the native no-op arm; PNG/object exchange and re-normalization remain unavailable. The gallery/search log links to the shared viewer shell with its own limitations. |
| Definitions, parser and formula editors | URL masks/rules/default processing, normalization/paging previews, GUG members/repair, page content ordering/context and all nine content-kind fields are source-backed. Content fixtures assert selected kind and parsed text/error, not every metadata value or downstream combination. Subsidiary authoring, fetching/multiple examples, auto-link/API-pair review and non-HTML/JSON formula editors are missing; preserved data is identified separately. |
| Shared strings and import options | Processor ordering/results and matcher/converter/step controls use canonical links. Processor exchange/paste and matcher regex favourites are missing. Date decode, encode and dateparser forms preserve settings but return `Conversion::Unsupported`; converter parents are partial and date execution/preview children are missing. Import-option default/override, prefetch, file restrictions, tags, notes and presentation fields are separated from missing favourites, option exchange and external-program execution. |
| Disk import, tagging and export | Manual review has actual file/folder pickers, bounded recursive parsing, pause/stop/removal, import options and tagging children. Filename tagging covers service/all/selected tags, namespaces, regex, numbering and sidecars, with typed-input limitations. Export separates previews/collisions/safety, metadata routers, copy/cancel/trash behavior and preferences. Folder schedule/outcome/search/type/filename controls cite saved runtime consumers. Example filename menus/panels and full reference autocomplete remain absent. |

All nine owned catalog windows are reparented to their network, manual-transfer or
simple-downloader workflows. `dialog.subscription.file-log` is assessed and expanded.
All **107 original native nodes beneath `menu.network`** are assessed. The four owned
reference `unassessed` leaves are resolved: downloader display and subscription
quality are missing; bandwidth default reset has a bounded first pass; filename
tagging is partial. Shared TagFilter, Locations and Sidecar canonical subtrees remain
with the integration owner; caller-specific links preserve their reuse.

Two source details prevent stale claims. Native manual import has real picker
callbacks despite an older differences-document statement. The supplemental Windows
export fix uses both slash styles for the final filename split, while earlier
directory sanitization retains native-separator behavior. Those supplemental source
records carry explicit `anchor_git_head` values; all other new anchors target the
primary snapshot.

Validation checked the patch applied to in-memory copies using
`scripts/gui_coverage.py`: IDs, parents, cycles, shared targets, status counts, and
candidate native mappings resolve. All **2,654 touched anchor occurrences** resolve
to valid one-based lines across **677 distinct commit/path/line anchors**, with no
absolute paths or line-one placeholders. Every touched first-pass node has its own
evidence. The unchanged canonical inventories also pass `--validate`. No canonical
JSON or production source is changed by this audit commit.
