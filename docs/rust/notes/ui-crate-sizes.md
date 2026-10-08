# Generated UI size per window (2026-10-08)

How much generated Rust each top-level Slint window produces when compiled on
its own, to decide whether and how to split the generated UI crate
(`generated/hydrus-gui-ui`, built from `crates/hydrus-gui/ui/main.slint`).
Measured with `target/tools/debug/slint-check ENTRY.slint OUT.rs` on a
one-line entry file per window (`import { W } from "<ui>/file.slint";
export { W }`). The generated Rust is what rustc compiles, and a `.slint` edit
rebuilds the whole crate it is in (~6 minutes for today's crate).

- The whole UI today (`main.slint`): **87.9 MB** in one crate.
- 122 windows. Compiled one by one they total 196.7 MB: a window compiled
  alone repeats the shared widgets it uses (~0.9 MB per window on
  average), which one crate generates once.
- The main window alone: 13.8 MB (16% of today's crate).
- Median window: 1.39 MB.

| Window | File | MB |
|---|---|---|
| `MainWindow` | `main.slint` | 13.78 |
| `OptionsWindow` | `options.slint` | 7.13 |
| `MediaViewerWindow` | `viewer.slint` | 4.19 |
| `ImportOptionsWindow` | `import_options.slint` | 4.15 |
| `TagRelationshipsWindow` | `tag_relationships.slint` | 3.53 |
| `StringStepWindow` | `string_processor.slint` | 3.48 |
| `PredicateEditorWindow` | `predicate_editor.slint` | 3.44 |
| `AutoResolutionRuleWindow` | `auto_resolution_rules.slint` | 3.13 |
| `ManageTagsWindow` | `manage_tags.slint` | 3.12 |
| `ParserEditWindow` | `parser_editors.slint` | 3.04 |
| `DownloaderDefinitionEditWindow` | `downloader_definitions.slint` | 3.00 |
| `FormulaWindow` | `formula_editors.slint` | 2.89 |
| `FilenameTaggingWindow` | `filename_tagging.slint` | 2.82 |
| `TagFilterWindow` | `tag_filter.slint` | 2.65 |
| `EditSubscriptionWindow` | `edit_subscription.slint` | 2.65 |
| `SidecarNodeWindow` | `sidecars.slint` | 2.60 |
| `ExportFilesWindow` | `export_files.slint` | 2.58 |
| `SubscriptionsWindow` | `subscriptions.slint` | 2.51 |
| `ConversionWindow` | `string_processor.slint` | 2.44 |
| `ImportFolderWindow` | `folders.slint` | 2.29 |
| `EditServiceWindow` | `services_editor.slint` | 2.28 |
| `ExportFolderWindow` | `folders.slint` | 2.26 |
| `ComparatorWindow` | `auto_resolution_rules.slint` | 2.24 |
| `FavouriteEditWindow` | `favourites.slint` | 2.23 |
| `LoginStepWindow` | `login_step.slint` | 2.17 |
| `MergeOptionsWindow` | `merge_options.slint` | 2.14 |
| `SearchOrWindow` | `search_or.slint` | 2.11 |
| `LoginScriptWindow` | `login_workflows.slint` | 2.07 |
| `BandwidthWindow` | `network_data.slint` | 2.01 |
| `FileMaintenanceWindow` | `file_maintenance_current.slint` | 1.98 |
| `WriteTagsWindow` | `write_autocomplete.slint` | 1.96 |
| `ImportOptionsPanelWindow` | `import_options_panel.slint` | 1.94 |
| `FormulaRuleWindow` | `formula_editors.slint` | 1.94 |
| `TagBannerWindow` | `tag_banner.slint` | 1.91 |
| `SidecarRouterWindow` | `sidecars.slint` | 1.90 |
| `TagMigrationWindow` | `tag_migration.slint` | 1.88 |
| `DownloaderExchangeWindow` | `downloader_interchange.slint` | 1.84 |
| `ExternalCallWindow` | `external_calls.slint` | 1.83 |
| `FileHistoryWindow` | `file_history.slint` | 1.78 |
| `SessionDialog` | `session_dialog.slint` | 1.74 |
| `DuplicateFilterWindow` | `duplicate_filter.slint` | 1.74 |
| `ManageTimesWindow` | `manage_times.slint` | 1.69 |
| `HowBonedWindow` | `how_boned.slint` | 1.69 |
| `StringProcessorWindow` | `string_processor.slint` | 1.68 |
| `AutoResolutionReviewWindow` | `auto_resolution_review.slint` | 1.67 |
| `EditNetworkValueWindow` | `network_sessions.slint` | 1.63 |
| `NamespaceSortsWindow` | `namespace_sorts.slint` | 1.59 |
| `TagDisplayWindow` | `tag_display.slint` | 1.58 |
| `AutoResolutionRulesWindow` | `auto_resolution_rules.slint` | 1.57 |
| `NetworkDataWindow` | `network_sessions.slint` | 1.50 |
| `SidecarRoutersWindow` | `sidecars.slint` | 1.50 |
| `RegexFavouritesWindow` | `regex_favourites.slint` | 1.49 |
| `FileLogWindow` | `file_log.slint` | 1.48 |
| `ManageNotesWindow` | `manage_notes.slint` | 1.48 |
| `SimpleFormulaeWindow` | `simple_formulae.slint` | 1.47 |
| `StringConverterWindow` | `string_processor.slint` | 1.45 |
| `CheckerOptionsWindow` | `checker_options.slint` | 1.43 |
| `RelatedWeightsWindow` | `related_weights.slint` | 1.41 |
| `FoldersWindow` | `folders.slint` | 1.40 |
| `EmbeddedMetadataWindow` | `embedded_metadata.slint` | 1.40 |
| `ManageUrlsWindow` | `manage_urls.slint` | 1.39 |
| `ShortcutCommandWindow` | `shortcuts.slint` | 1.36 |
| `DownloaderDefinitionsWindow` | `downloader_definitions.slint` | 1.36 |
| `ReviewImportsWindow` | `import_files.slint` | 1.34 |
| `LoginExampleDomainWindow` | `login_example.slint` | 1.34 |
| `NetworkJobsWindow` | `network_data.slint` | 1.30 |
| `ExternalCommandWindow` | `external_calls.slint` | 1.29 |
| `EditBandwidthRulesWindow` | `network_data.slint` | 1.28 |
| `DateTimeEditorWindow` | `manage_times.slint` | 1.25 |
| `EditApiPermissionsWindow` | `client_api_admin.slint` | 1.24 |
| `ShortcutSetWindow` | `shortcuts.slint` | 1.22 |
| `LoginCredentialDefinitionWindow` | `login_credentials.slint` | 1.20 |
| `IncrementalTaggingWindow` | `incremental_tagging.slint` | 1.20 |
| `ManageRatingsWindow` | `manage_ratings.slint` | 1.19 |
| `LoginDomainEntryWindow` | `login_domain_entry.slint` | 1.15 |
| `LoginTestResultWindow` | `login_result.slint` | 1.13 |
| `DownloaderDisplayWindow` | `downloader_display.slint` | 1.07 |
| `DeleteFilesWindow` | `delete_files.slint` | 1.06 |
| `ParserListWindow` | `parser_editors.slint` | 1.06 |
| `PngExportWindow` | `png_export.slint` | 1.05 |
| `LoginCredentialsWindow` | `login_credentials.slint` | 1.04 |
| `ServicesReviewWindow` | `services_review.slint` | 0.99 |
| `FrameLocationWindow` | `frame_locations.slint` | 0.98 |
| `MediaViewWindow` | `media_views.slint` | 0.97 |
| `ServicesEditorWindow` | `services_editor.slint` | 0.97 |
| `ImportOptionsOverwriteWindow` | `import_options_overwrite.slint` | 0.94 |
| `SearchLogImportWindow` | `search_log_import.slint` | 0.93 |
| `DebugFetchWindow` | `debug_fetch.slint` | 0.91 |
| `CommandPaletteWindow` | `command_palette.slint` | 0.86 |
| `AboutWindow` | `about.slint` | 0.82 |
| `ImportFavouritePromptWindow` | `import_options_favourites.slint` | 0.82 |
| `LocationMaxSizeWindow` | `database_locations.slint` | 0.81 |
| `UnlockWindow` | `unlock.slint` | 0.80 |
| `LoginDomainsWindow` | `login_domains.slint` | 0.79 |
| `DatabaseLocationsWindow` | `database_locations.slint` | 0.73 |
| `MostUsedTagsWindow` | `tag_suggestions.slint` | 0.73 |
| `ClientApiKeysWindow` | `client_api_admin.slint` | 0.73 |
| `LoginScriptsWindow` | `login_workflows.slint` | 0.66 |
| `LoginCookiesWindow` | `login_cookies.slint` | 0.62 |
| `OpenFileCallsWindow` | `open_externally.slint` | 0.62 |
| `FavouritesWindow` | `favourites.slint` | 0.59 |
| `ArchiveDeleteWindow` | `archive_delete.slint` | 0.57 |
| `VacuumReviewWindow` | `vacuum_review.slint` | 0.55 |
| `ArchiveRepairWindow` | `archive_repair.slint` | 0.55 |
| `ForceFiletypeWindow` | `force_filetype.slint` | 0.55 |
| `TagSyncReviewWindow` | `tag_sync_review.slint` | 0.49 |
| `ParserPickerWindow` | `parser_editors.slint` | 0.48 |
| `ExternalDefaultsWindow` | `external_calls.slint` | 0.48 |
| `GallerySourceWindow` | `gallery_source.slint` | 0.47 |
| `LocationsWindow` | `domains.slint` | 0.46 |
| `GuiColourPickerWindow` | `gui_coloursets.slint` | 0.45 |
| `SubscriptionGalleryWindow` | `subscriptions.slint` | 0.44 |
| `ExternalRoutingChoiceWindow` | `open_externally.slint` | 0.36 |
| `EditValueWindow` | `edit_value.slint` | 0.35 |
| `GranularityWindow` | `database_locations.slint` | 0.34 |
| `NetworkErrorWindow` | `importing.slint` | 0.32 |
| `TagMigrationProgressWindow` | `tag_migration.slint` | 0.31 |
| `CookieImportWindow` | `network_sessions.slint` | 0.29 |
| `ChoiceButtonsWindow` | `choice_buttons.slint` | 0.27 |
| `HeaderApprovalWindow` | `network_sessions.slint` | 0.25 |
| `LocalTransferWindow` | `local_transfer.slint` | 0.25 |
| `ApiRequestWindow` | `client_api_admin.slint` | 0.20 |
