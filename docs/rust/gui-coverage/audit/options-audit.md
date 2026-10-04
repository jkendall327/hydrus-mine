# Options, search, pages and main-shell assessment

Source snapshot: `5079206dc1d0c2d2614b428632dc9499cb9ad3f8`.

This patch assesses the File, Pages, Undo and Help menus and the existing About,
Options, PredicateEditor, Session, Unlock, MainWindow and favourite-search
windows. It follows native Slint callbacks through the model, persisted Store
and live Pages, and compares the corresponding reference callbacks and recorded
Qt controls. Existing regressions and oracle recorders were read as evidence;
no GUI, recorder, Cargo command, runtime test or mutation test was executed.

`options-patch.json` is a diff against the baseline canonical inventories:
88 native updates and 560 additions; 43 reference updates and 1,114 additions.
Updates contain complete nodes and can be merged by ID. Canonical inventories,
snapshot pins, totals and shared provenance remain untouched. No deletions are
requested. All owned unassessed nodes receive specific assessments. The larger
reference addition count comes mainly from exposing the meaningful controls of
all 38 recorded options pages, rather than treating a page name as a completed
feature. Hidden widgets, wrappers and explanatory labels are excluded.

The native options registry exposes 20 pages. The patch follows their actual
page/box/field containment and names the Settings fields read and written by
each control. First-pass assessments cover the exact represented control's
recorded label, default, constraints and setter/Apply flow; they do not declare
its entire containing Qt page complete. Eighteen absent tabs and missing rows
within implemented tabs remain explicit gaps. Composite sort/collect rows are
partial because they omit independent tag-context cog controls. Nested external
call pipelines, process argument/input-rule/test panels, frame-location forms,
shortcut/command capture, namespace sort editing, tag-banner generation and
suggested-tag configuration also have concrete missing descendants.

System predicates expand into 19 groups, the actual named editor pages,
preset actions, editor panels and compound field families. Blank autocomplete
suggestions really open the editor; OK/presets reach SearchPage and recent
predicate persistence. Custom defaults, active-predicate re-editing, interactive
OR construction and the advanced OR form remain missing. Filetype tri-state,
calendar inputs, bitmap clipboard paste, hash cleanup confirmation and viewing
seconds have specific partial assessments. Search autocomplete lacks the Qt
children and favourite-tag tabs; this source snapshot has no recent-tag
autocomplete tab, so none is invented.

Menu actions have separate label and callback evidence. Named/current session
save/load/append/delete reach Store; session-name > timestamp backup append is
missing. Closed-page restoration works, while content Undo/Redo and predicate
addition/removal history do not. The total session weight is computed and shown,
so its node is partial: only its explanatory breakdown dialog is missing.
Sidebar sizing, tray, restart/forced-maintenance exit and debug routes are
explicit gaps. Reference Help Debug and the tab context menu retain their actual
submenu/action nesting, including batch close, navigation, movement, sorting,
collapse, notebook grouping and notebook-specific session operations.

Favourite management's detached list transaction, selection, sorting,
confirmation and Apply/Cancel work. The child editor can edit name/predicates,
synchronization and save flags, but file/tag domains and sort/collect values are
read-only; these four missing editing paths are children. Startup unlock checks
the persisted password before client/daemon startup, with wrong-password retry
and Cancel. Creating/changing the password is the separate missing Database
menu workflow. About exposes live Rust/platform/SQLite facts and four tabs but
omits much of Qt's runtime/library/locale census.

MainWindow now has explicit notebook, search, geometry, status and PopupStack
branches. PopupStack is a component, not an exported Window: its six callbacks
update persisted jobs, open attached files through Pages and copy complete
tracebacks. Yes/No answers, attached user callables, arbitrary clipboard payloads,
modal progress dialogs, focus/minimization freeze policy and broader producers
remain missing. Its embedded download-control limitations are scoped rather
than claiming the network control complete.

Validation applied this patch in memory only: IDs, parent existence/cycles,
shared references, native candidates, owned status coverage and all 1,451 unique
source anchors were checked against the full baseline Git commit. The only
external reference dependency is `audit-shared-locations`, supplied by the
integration audit; native checker controls reuse `dialog.subscription.checker`.
The first-pass badge means this stated source/recording scope, not fresh runtime
verification or parity for a whole window.

Independent review also checked repeated option labels against their real
page/box rows. Subscription/gallery wait links and repeated maintenance, player,
page and thumbnail rows now point to the matching setting. Eight connection
limits/downloader error delays are partial: native always enforces normal-mode
bounds, while Qt advanced mode permits larger limits or shorter delays. The
CPU-busy percentage and nullable core count now have meaningful labels and
constructor/initialization/setter/consumer anchors. Generic list names were
replaced with their actual frame, MIME, profile, callable or ordering domain.
