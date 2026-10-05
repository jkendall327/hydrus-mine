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
mod delete_files;
mod downloader_definitions;
mod duplicate_colours;
mod duplicates_page;
mod edit_subscription;
mod embedded_metadata;
mod existing_tags_filter;
mod file_log;
mod filename_rules;
mod filename_simple;
mod filename_tagging;
mod filetype_tree;
mod folders;
mod force_filetype;
mod formula_editors;
mod import_options_editor;
mod import_options_overwrite;
mod import_options_panel;
mod importer_menu;
mod local_import_dialog;
mod login_workflows;
mod main_menu;
mod manage_notes;
mod merge_options_editor;
mod merge_summaries;
mod namespace_colours;
mod notes_preferences;
mod options_dialog;
mod page_chooser_options;
mod page_navigation_options;
mod predicate_history;
mod rating_sizes;
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
mod subscription_exchange;
mod subscription_import_options;
mod subscriptions_buttons;
mod subscriptions_dedupe;
mod subscriptions_list;
mod tag_filter_editor;
mod tag_filter_favourites;
mod thumbnail_navigation;
mod thumbnail_ratings;
mod times_editor;
mod urls_editor;
mod viewtime_milliseconds;

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

mod sibling_colours;
mod sibling_connector;
mod tag_dialog_defaults;
mod tag_dialog_preferences;
mod tag_suggestions;
mod unselected_tag_cap;

mod write_autocomplete;

mod network_job_control;

mod gallery_source;
mod subscription_quality;
mod viewer_closing;

mod namespace_sorts;
mod viewer_cursor;

mod tag_list_display_types;

mod sort_cog;

mod command_palette;

mod viewing_statistics;

mod search_or;
mod system_or_activation;

mod manage_tag_counts;

mod frame_locations;
mod incremental_tagging;
mod tag_banner;

mod page_tree;
mod tab_drag;
mod tab_presentation;

mod archive_repair;

mod file_history;
mod related_weights;

mod autocomplete_tabs;
mod external_calls;

mod viewer_drag;
mod window_rescue;

mod gui_format;
mod viewer_tag_wheel;

mod idle_timeout_options;
mod legacy_seed_caches;

mod import_work_slots;
mod viewing_maintenance;
