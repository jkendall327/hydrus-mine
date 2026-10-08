//! hydrus-gui-model's integration tests, one module per part of the GUI's
//! workings, against the reference's recordings where it has one. They
//! build without the windows, so they (and mutation testing them) run in
//! seconds; the windows' own tests are hydrus-gui's.

mod about;
mod active_predicates;
mod animation_start;
mod api_update_toasts;
mod auto_resolution_exchange;
mod auto_resolution_review;
mod auto_resolution_rules;
mod checker_options;
mod clipboard_urls;
mod content_undo;
mod database_backup;
mod database_locations;
mod database_maintenance;
mod datetime_editor;
mod debug_actions;
mod delete_files;
mod downloader_definitions;
mod downloader_update_times;
mod duplicate_colours;
mod duplicates_filtering;
mod duplicates_page;
mod edit_subscription;
mod embedded_metadata;
mod existing_tags_filter;
mod file_log;
mod file_relationships;
mod file_view_removal;
mod filename_rules;
mod filename_simple;
mod filename_tagging;
mod filesize_predicate;
mod filetype_tree;
mod folder_runs;
mod folders;
mod force_filetype;
mod force_idle;
mod formula_editors;
mod hash_predicate;
mod how_boned;
mod image_cache;
mod import_options_editor;
mod import_options_overwrite;
mod import_options_panel;
mod importer_menu;
mod local_import_dialog;
mod login_script_controls;
mod login_workflows;
mod main_identity;
mod main_menu;
mod manage_notes;
mod merge_options_editor;
mod merge_summaries;
mod namespace_colours;
mod normal_time_maintenance;
mod notes_preferences;
mod options_dialog;
mod orphan_files;
mod page_chooser_options;
mod page_navigation_options;
mod physical_delete_delay;
mod predicate_history;
mod rating_sizes;
mod ratings_editor;
mod recent_predicates;
mod search_log;
mod selected_deletion_records;
mod selection;
mod session_lifecycle;
mod session_saving;
mod session_weight;
mod set_password;
mod shortcut_capture;
mod shortcut_content;
mod shortcut_sets;
mod shutdown_work;
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
mod thumbnail_appearance;
mod thumbnail_cache;
mod thumbnail_maintenance;
mod thumbnail_navigation;
mod thumbnail_preview_selection;
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
mod tag_application_queues;
mod tag_display;
mod tag_relationships_default;

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
mod tag_sync_review;
mod unselected_tag_cap;
mod vacuum_review;

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
mod manage_tags_sort;

mod frame_locations;
mod incremental_tagging;
mod tag_banner;

mod page_tree;
mod tab_drag;
mod tab_presentation;

mod archive_repair;

mod file_history;
mod file_maintenance_current;
mod file_maintenance_new;
mod related_weights;

mod autocomplete_tabs;
mod external_calls;
mod open_externally;

mod viewer_drag;
mod window_rescue;

mod gui_colours;
mod gui_format;
mod popup_width;
mod preview_default_zoom;
mod viewer_shortcut_menu;
mod viewer_tag_wheel;

mod idle_timeout_options;
mod legacy_seed_caches;

mod import_work_slots;
mod viewing_maintenance;

mod local_transfer;

mod page_layout;

mod or_connector;

mod image_colour;

mod duplicates_progress;
mod quick_export_directory;

mod archive_delete_policies;

mod ffmpeg_timeout;
mod radio_return;
mod tag_namespace_order;

mod media_view_options;
mod menu_choice_wheel;
mod popup_freeze;

mod viewer_prefetch;
