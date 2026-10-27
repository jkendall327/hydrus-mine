# Interaction audit (2026-10-27)

The leaves were built from menus, dialogs and options pages, so interactions
inside a widget (clicks, context menus, keys, wheel, drag and drop) were easy
to miss. `scripts/interaction_audit.py` lists every such handler in
`hydrus/client/gui` (129 handlers in 65 classes; `--json` for machine use).
This page records the hand triage. New leaves are in `leaves.json` under the
path `Interactions > ...` (ids start `interaction-`); none is tagged.

## Triage

| Reference widget (handlers) | Verdict | Where |
|---|---|---|
| Thumbnail grid: press (ctrl/shift/right/middle), double-click, context menu, move | click-select, double-click, menu: implemented, untracked; **middle-click: missing**; drag-out: deferred (#26) | `interaction-thumbnail-*` |
| `BetterListCtrlTreeView` keys (Delete, Enter, Ctrl+A, Ctrl+C, arrows) | Delete only; **rest missing** | `interaction-list-keys-*` |
| `BetterListCtrlTreeView` double-click, context menu | implemented, untracked | `interaction-list-pointer` |
| `BetterQListWidget` Delete, Ctrl+C/Insert; `QueueListBox`, `AddEditDeleteListBox` double-click | covered by the owning dialogs' leaves (edit/delete/copy) | existing |
| `TabBar` double-click a tab | **missing** | `interaction-tab-double-click-rename` |
| `PagesNotebook` event filter, `TabBar` middle/right/empty space | implemented, untracked | `interaction-tab-strip-clicks` |
| `TabBar`/`TabWidgetWithDnD` drag and drop | implemented, untracked | `interaction-tab-drag` |
| `TreeViewWithDnD` context menu, drag and drop | **missing** | `interaction-pages-tree-menu-drag` |
| `PopupWindow` right-click | implemented, untracked | `interaction-popup-right-click-dismiss` |
| `MediaCollectControl` middle-click | **missing** | `interaction-collect-control-middle-click` |
| `MediaContainer` embed button | **missing** | `interaction-embed-button` |
| `NotePanel` left/right click | **missing** (middle is native) | `interaction-viewer-note-hover-clicks` |
| `InboxIconClickFilter` | **missing** | `interaction-viewer-inbox-icon-click` |
| `ListBox`/`ListBoxTags` click, keys, middle, wheel; `ListBoxTagsStringsAddRemove` | selection tags box and viewer tag hover: #286. Other tag lists: covered (`GUI.md` tag-list sections, `suggested-tag-list-interactions`) | |
| `IconButtonMultiClickable`, `WindowDragButton`, top-hover nav buttons | viewer top hover strip: #287 | |
| `RatingIncDec`/`RatingLike`/`RatingNumerical` | covered by `audit-media-ratings-*` (middle count editor: `audit-media-ratings-missing-count`) | existing |
| `AnimationBar`, `MediaContainerDragClickReportingFilter`, `CanvasHoverFrameTags` wheel, canvas shortcuts | covered by viewer option leaves and recordings (`viewer_pointer_options.json`, `viewer_tag_wheel.json`) | existing |
| `MouseShortcutButton`, `KeyboardShortcutWidget`, `ShortcutsHandler` | covered by the shortcuts leaves | existing |
| `AutoCompleteDropdown`, `QLocator*`, `DialogPageChooser`, `ListEnterCatcher`, `ThumbnailPairList` | covered (autocomplete, command palette, page chooser, duplicates pair lists; pair-list right-click is `audit-media-preview-context`, missing) | existing |
| `FileDropTarget`, `MediaResultsPanelGraphicsViewTest` drops | import drops covered (`audit-network-import-paths`); the "Test" graphics-view classes are the reference's experimental duplicates of the thumbnail panel | existing / out of scope |
| `ColourPickerButton` menu, tray `activated`, `OpenExternallyPanel` click | covered (`GUI.md` colours, system tray, open externally) | existing |
| `SidebarPetitions`, `ListAccountsPanel` | repositories and servers: out of scope (DECISIONS) | |
| `Dialog`/`NewDialog`/`FrameThatTakesScrollablePanel` key handlers (Escape closes), `EnterCatchingRadioButton`, `TextCatchEnterEventFilter`, `_PredOKPanel` Enter, `_ClickMemoryFilter` | Qt-only details or standard dialog behaviour; `_ClickMemoryFilter` only serves the viewer's window-resize menu (#287) | |
| `FrameSplashPanel` drag | Qt-only (frameless splash window) | |
| `ReviewDownloaderImport`/`ReviewWhatIsThisObject` left-down | a decorative image click (an Easter egg), not tracked | |

## Windows with no recording-backed GUI test of their own name

For the question "does any test compare this window's control list, order and
visibility with a recording?" this is a name-based seed list, produced by
`scripts/interaction_audit.py --unrecorded-windows`: Slint windows for which no
`tests/gui/<file stem>*.rs` loads a fixture. It is not a verdict (a window may
be tested under another file's name, or have only plain strings). Triage by
reading the window's reference code and checking whether a recorder captures
its children.

Hovers: recordings exist for the options that govern the viewer's hovers
(`viewer_hover_options.json`: ideal layout decisions), the duplicate filter's
right hover (`duplicates_hover_pin.json`, `duplicate_filter_canvas.json`:
comparison statements, pin) and the thumbnail menu. No recorder reads the
child controls, order or visibility of the viewer's right (ratings), notes or
left hovers, or the duplicate filter's top hover. The viewer's top strip is
#287.

| File | Window | GUI test files of that name | Leaves they tag |
|---|---|---|---|
| auto_resolution_rules.slint | AutoResolutionRulesWindow, AutoResolutionRuleWindow, ComparatorWindow | 1 | audit-media-comparator-or, audit-media-duplicate-search-count |
| checker_options.slint | CheckerOptionsWindow | 0 | (none tagged) |
| choice_buttons.slint | ChoiceButtonsWindow | 0 | (none tagged) |
| delete_files.slint | DeleteFilesWindow | 0 | (none tagged) |
| domains.slint | LocationsWindow | 0 | (none tagged) |
| edit_value.slint | EditValueWindow | 0 | (none tagged) |
| external_program_entry.slint | ExternalProgramEntryWindow | 0 | (none tagged) |
| filename_tagging.slint | FilenameTaggingWindow | 0 | (none tagged) |
| gui_coloursets.slint | GuiColourPickerWindow | 0 | (none tagged) |
| import_options_favourites.slint | ImportFavouritePromptWindow | 0 | (none tagged) |
| import_options_overwrite.slint | ImportOptionsOverwriteWindow | 0 | (none tagged) |
| importing.slint | NetworkErrorWindow | 0 | (none tagged) |
| login_cookies.slint | LoginCookiesWindow | 0 | (none tagged) |
| login_credentials.slint | LoginCredentialDefinitionWindow, LoginCredentialsWindow | 0 | (none tagged) |
| login_domain_entry.slint | LoginDomainEntryWindow | 0 | (none tagged) |
| login_domains.slint | LoginDomainsWindow | 0 | (none tagged) |
| login_example.slint | LoginExampleDomainWindow | 0 | (none tagged) |
| login_result.slint | LoginTestResultWindow | 0 | (none tagged) |
| login_step.slint | LoginStepWindow | 0 | (none tagged) |
| manage_ratings.slint | ManageRatingsWindow | 1 | audit-media-ratings-clipboard, audit-media-ratings-like, audit-media-ratings-missing-count, audit-media-ratings-numerical |
| manage_urls.slint | ManageUrlsWindow | 1 | audit-media-urls-clipboard, audit-media-urls-edit |
| media_views.slint | MediaViewWindow | 0 | (none tagged) |
| png_export.slint | PngExportWindow, PngsExportWindow | 0 | (none tagged) |
| popup_modal.slint | PopupModalWindow | 0 | (none tagged) |
| predicate_editor.slint | PredicateEditorWindow | 0 | (none tagged) |
| quick_tag_entry.slint | QuickTagEntryWindow | 0 | (none tagged) |
| related_weights.slint | RelatedWeightsWindow | 0 | (none tagged) |
| search_log_import.slint | SearchLogImportWindow | 0 | (none tagged) |
| search_or.slint | SearchOrWindow | 0 | (none tagged) |
| session_dialog.slint | SessionDialog | 0 | (none tagged) |
| shortcuts.slint | ShortcutSetWindow, ShortcutCommandWindow | 0 | (none tagged) |
| tag_banner.slint | TagBannerWindow | 0 | (none tagged) |
| tag_suggestions.slint | MostUsedTagsWindow | 0 | (none tagged) |
| tag_sync_review.slint | TagSyncReviewWindow | 0 | (none tagged) |
| vacuum_review.slint | VacuumReviewWindow | 0 | (none tagged) |
| what_is_this.slint | WhatIsThisWindow | 0 | (none tagged) |
