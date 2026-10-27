# Interaction audit (2026-10-27)

The leaves were built from menus, dialogs and options pages, so interactions
inside a widget (clicks, context menus, keys, wheel, drag and drop) were easy
to miss. `scripts/interaction_audit.py` lists every such handler in
`hydrus/client/gui`: 231 handlers in 123 classes, counting each handler a
subclass inherits from a reference-side base class (`--json` for machine use;
`ShortcutsHandler(` and `QShortcut(` attachments are a second table).
This page records the hand triage. New leaves are in `leaves.json` under the
path `Interactions > ...` (ids start `interaction-`); none is tagged.

## Triage

"Live" means the class the reference client really uses; the reference also
has `*GraphicsViewTest` classes (an experimental, unused port of the thumbnail
and duplicates panels) which are ignored.

| Reference widget (handlers) | Verdict | Where |
|---|---|---|
| Thumbnail grid (`MediaResultsPanelThumbnails`: `_InnerWidget` press, `EVT_LEFT_DCLICK`/`EVT_MIDDLE_DOWN` -> `EventMouseFullScreen`, context menu, drag) | click-select, double-click, menu: implemented, untracked; **middle-click: missing**; drag-out: deferred (#26); remote-file download on double/middle-click: out of scope (file repositories) | `interaction-thumbnail-*` |
| Thumbnail keyboard (`ShortcutsHandler` sets `media`, `thumbnails`) | covered by the shortcuts leaves | existing |
| `BetterListCtrlTreeView` keys (Delete, Enter, Ctrl+A, Ctrl+C, arrows) | Delete only; **rest missing** | `interaction-list-keys-*` |
| `BetterListCtrlTreeView` double-click, rows menu (`:881`); header column menu (`:621`) | implemented for `ListTable` only; hand-rolled lists (`auto_resolution_review.slint:107-113`, `favourites.slint:130-136`) lack it; the column menu has no native counterpart | `interaction-list-pointer` (partial) |
| `ListBoxTagsMedia` and subclasses: `ListBoxTagsMediaTagsDialog` (manage tags list), `ListBoxTagsMediaSidebar` (selection tags), `ListBoxTagsMediaHoverFrame` (viewer tag hover), `ListBoxTagsStringsAddRemove`, `ListBoxTagsFilter`, `ListBoxTagsColourOptions`, `ListBoxTagsPredicates` and `ListBoxTagsPredicatesAC` (inherit `ListBoxTags`/`ListBox`: tag menu, middle-click search, ctrl/shift/arrow/Home/End/Page keys, wheel, drag-select) | selection tags and viewer tag hover: #286. **Manage tags list: missing** (`manage_tags.slint:190-207` has left press, double-click, Delete and Return only), tracked with #286. Active predicates and autocomplete result lists: native (`active_predicates.slint`, `read_suggestions.slint`). Tag filter and colour lists: covered by their editors' leaves | `interaction-manage-tags-list-menu-keys` |
| `BetterQListWidget` Delete, Ctrl+C/Insert; `QueueListBox`, `AddEditDeleteListBox` (and its subclasses) double-click | covered by the owning dialogs' leaves (edit/delete/copy) | existing |
| `TabBar` double-click a tab | **missing** | `interaction-tab-double-click-rename` |
| `PagesNotebook` event filter (double-click empty space, right-click) | implemented, untracked (middle-click closing a tab is `audit-options-tabs-close`) | `interaction-tab-strip-clicks` |
| `TabBar`/`TabWidgetWithDnD` (inherited by `PagesNotebook`) drag and drop | implemented, untracked | `interaction-tab-drag` |
| `TreeViewWithDnD` context menu, drag and drop | **missing** | `interaction-pages-tree-menu-drag` |
| `PopupWindow` right-click dismiss | covered by `audit-options-popups-dismiss` | existing |
| `MediaCollectControl` middle-click | **missing** | `interaction-collect-control-middle-click` |
| `MediaContainer` embed button | **missing** | `interaction-embed-button` |
| `NotePanel` left/right click | **missing** (middle is native) | `interaction-viewer-note-hover-clicks` |
| `InboxIconClickFilter` | **missing** | `interaction-viewer-inbox-icon-click` |
| `CanvasHoverFrame*` press/release (inherited by every hover; they accept the event so it does not reach the canvas) | Qt event plumbing | |
| `IconButtonMultiClickable`, `WindowDragButton`, top-hover nav buttons, `_ClickMemoryFilter` (window-resize menu right-click) | viewer top hover strip: #287 | |
| `RatingIncDec` (and `Dialog`/`Example` subclasses), `RatingLike`, `RatingNumerical` | covered by `audit-media-ratings-*` (middle count editor: `audit-media-ratings-missing-count`) | existing |
| `AnimationBar`, `MediaContainerDragClickReportingFilter`, `CanvasHoverFrameTags` wheel, canvas `ShortcutsHandler` | covered by viewer option leaves and recordings (`viewer_pointer_options.json`, `viewer_tag_wheel.json`) | existing |
| `MouseShortcutButton`, `KeyboardShortcutWidget`, `ShortcutsHandler` | covered by the shortcuts leaves | existing |
| `AutoCompleteDropdown` and subclasses, `QLocator*` (`QShortcut` keys), `DialogPageChooser`, `ListEnterCatcher`, `ThumbnailPairList` and subclasses | covered (autocomplete, command palette, page chooser, duplicates pair lists; pair-list right-click is `audit-media-preview-context`, missing) | existing |
| `FileDropTarget` | import drops covered (`audit-network-import-paths`) | existing |
| `ColourPickerButton` menu, tray `activated`, `OpenExternallyPanel` click | covered (`GUI.md` colours, system tray, open externally) | existing |
| `SidebarPetitions`, `ListAccountsPanel` | repositories and servers: out of scope (DECISIONS) | |
| `Dialog`/`NewDialog`/`DialogThatResizes`/`FrameThatTakesScrollablePanel` key handlers and every subclass (Escape closes), `EnterCatchingRadioButton`, `TextCatchEnterEventFilter`, `_PredOKPanel` Enter | Qt-only details or standard dialog behaviour | |
| `FrameSplashPanel` drag | Qt-only (frameless splash window) | |
| `ReviewDownloaderImport`/`ReviewWhatIsThisObject` left-down | a decorative image click (an Easter egg), not tracked | |

## What the script still cannot see

- Menus opened by a button (`ClientGUIMenus`, `MenuButton`, `MenuBitmapButton`
  and the like): the click that opens them is a plain button press, and the
  menu contents are built at run time.
- Handlers attached by name to a callback table (`self._filter.EVT_*` is seen;
  `connect(...)` of generic `clicked`/`activated` signals is not).
- What a handler does beyond the buttons, keys and modifiers named in its
  source: the buttons column is a regular-expression read of the handler,
  not a model of its control flow.
- Shortcut sets: `ShortcutsHandler(` attachments are listed with the set names
  they take, but what each shortcut does lives in the shortcut definitions.
- Inheritance is followed by base-class name within `hydrus/client/gui`; two
  classes with the same name (there are two `Dialog`s) are merged, and Qt's own
  default behaviours (a `QTreeView`'s arrow keys, say) are inferred by hand.

## Windows with no GUI test that mentions them

For the question "does any test compare this window's control list, order and
visibility with a recording?" this is a seed list from
`scripts/interaction_audit.py --unrecorded-windows`: Slint windows for which no
`tests/gui` file is named after the window's file or mentions its window type
(so "none tagged" means no test names it at all), and no such file loads a
fixture. It is a triage aid, not a verdict: a window can be tested through a
parent window. Nothing was fixed.

Hovers: recordings exist for the options that govern the viewer's hovers
(`viewer_hover_options.json`: ideal layout decisions), the duplicate filter's
right hover (`duplicates_hover_pin.json`, `duplicate_filter_canvas.json`:
comparison statements, pin) and the thumbnail menu. No recorder reads the
child controls, order or visibility of the viewer's right (ratings), notes or
left hovers, or the duplicate filter's top hover. The viewer's top strip is
#287.

| File | Window | GUI test files of that name | Leaves they tag |
|---|---|---|---|
| checker_options.slint | CheckerOptionsWindow | 0 | (none tagged) |
| edit_value.slint | EditValueWindow | 0 | (none tagged) |
| import_options_favourites.slint | ImportFavouritePromptWindow | 0 | (none tagged) |
| import_options_overwrite.slint | ImportOptionsOverwriteWindow | 0 | (none tagged) |
| importing.slint | NetworkErrorWindow | 0 | (none tagged) |
| login_cookies.slint | LoginCookiesWindow | 0 | (none tagged) |
| login_credentials.slint | LoginCredentialDefinitionWindow, LoginCredentialsWindow | 0 | (none tagged) |
| login_domain_entry.slint | LoginDomainEntryWindow | 0 | (none tagged) |
| login_domains.slint | LoginDomainsWindow | 0 | (none tagged) |
| login_example.slint | LoginExampleDomainWindow | 0 | (none tagged) |
| login_step.slint | LoginStepWindow | 0 | (none tagged) |
| manage_ratings.slint | ManageRatingsWindow | 1 | audit-media-ratings-clipboard, audit-media-ratings-like, audit-media-ratings-missing-count, audit-media-ratings-numerical |
| media_views.slint | MediaViewWindow | 0 | (none tagged) |
| png_export.slint | PngExportWindow, PngsExportWindow | 0 | (none tagged) |
| popup_modal.slint | PopupModalWindow | 0 | (none tagged) |
| quick_tag_entry.slint | QuickTagEntryWindow | 0 | (none tagged) |
| search_log_import.slint | SearchLogImportWindow | 0 | (none tagged) |
| tag_suggestions.slint | MostUsedTagsWindow | 0 | (none tagged) |
| tag_sync_review.slint | TagSyncReviewWindow | 0 | (none tagged) |
| vacuum_review.slint | VacuumReviewWindow | 0 | (none tagged) |
