//! hydrus-gui-model's integration tests, one module per part of the GUI's
//! workings, against the reference's recordings where it has one. They
//! build without the windows, so they (and mutation testing them) run in
//! seconds; the windows' own tests are hydrus-gui's.

mod about;
mod auto_resolution_review;
mod auto_resolution_rules;
mod checker_options;
mod clipboard_urls;
mod datetime_editor;
mod downloader_definitions;
mod duplicates_page;
mod edit_subscription;
mod embedded_metadata;
mod file_log;
mod filename_tagging;
mod filetype_tree;
mod folders;
mod force_filetype;
mod formula_editors;
mod import_options_editor;
mod import_options_overwrite;
mod importer_menu;
mod local_import_dialog;
mod login_workflows;
mod main_menu;
mod manage_notes;
mod merge_options_editor;
mod merge_summaries;
mod options_dialog;
mod ratings_editor;
mod recent_predicates;
mod search_log;
mod selection;
mod session_lifecycle;
mod session_saving;
mod sidecar_descriptions;
mod sidecar_editors;
mod sidecar_previews;
mod string_converter_editor;
mod string_match_editor;
mod string_processor_editor;
mod string_tag_filter_tests;
mod subscription_import_options;
mod subscriptions_buttons;
mod subscriptions_dedupe;
mod subscriptions_list;
mod tag_filter_editor;
mod tag_filter_favourites;
mod thumbnail_ratings;
mod times_editor;
mod urls_editor;

mod export_files;
mod services_review;
mod tag_relationships;

mod services_editor;

mod parser_editors;
mod parser_test_data;
mod tag_display;

mod client_api_admin;

mod network_sessions;

mod downloader_interchange;
mod network_data;
mod tag_migration;

mod downloader_display;
mod favourite_search_editor;
mod regex_favourites;
mod tab_context;

mod tag_dialog_defaults;
mod tag_dialog_preferences;

mod write_autocomplete;

mod network_job_control;
