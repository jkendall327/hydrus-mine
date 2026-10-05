//! hydrus-gui's integration tests, one module per part of the GUI, built
//! as one test binary (each was its own, every one hundreds of megabytes
//! and linked separately).

mod common;

mod about;
mod animation;
mod archive_delete;
mod auto_resolution_preview;
mod auto_resolution_review;
mod auto_resolution_rules;
mod clipboard_urls;
mod collect;
mod downloader_definitions;
mod downloader_lists;
mod duplicate_colours;
mod duplicate_filter;
mod duplicates_page;
mod edit_subscription;
mod embedded_metadata;
mod favourites;
mod file_log;
mod filename_rules;
mod filename_simple;
mod folder_manager_lifecycle;
mod folders;
mod force_filetype;
mod formula_editors;
mod import_files;
mod import_folder_log;
mod import_options;
mod import_options_panel;
mod importer_list_menu;
mod info_lines;
mod list_drag;
mod login_workflows;
mod main_shortcuts;
mod manage_notes;
mod manage_ratings;
mod manage_tags;
mod manage_times;
mod manage_urls;
mod media_actions;
mod media_shortcuts;
mod media_sort;
mod menu_bar;
mod merge_options;
mod mpv;
mod notebook_new_page;
mod notebook_refresh;
mod notebook_sessions;
mod notes_preferences;
mod options_window;
mod page_chooser_options;
mod page_navigation_options;
mod page_scroll;
mod popups;
mod predicate_editors;
mod predicate_history;
mod rating_sizes;
mod ratings;
mod recent_predicates;
mod scanbar;
mod search_domains;
mod search_lock;
mod search_log;
mod search_page;
mod session;
mod session_autosave;
mod session_startup;
mod sidecars;
mod simple_downloader;
mod slideshow;
mod status_bar;
mod still;
mod subscriptions;
mod subscriptions_dedupe;
mod subscriptions_duplicate;
mod subscriptions_separate;
mod thumbnail_icons;
mod thumbnail_menu;
mod thumbnail_navigation;
mod thumbnail_ratings;
mod thumbnail_selection;
mod unlock;
mod viewer_menu;
mod viewer_top_frame;
mod viewer_window;
mod volume;
mod watcher_checker;
mod zoom;

mod export_files;
mod services_review;
mod tag_relationships;

mod services_editor;

mod client_api_admin;
mod parser_editors;
mod tag_display;

mod network_sessions;

mod network_data;

mod downloader_interchange;
mod tag_migration;

mod regex_favourites;
mod tag_filter_favourites;

mod downloader_display;
mod favourite_search_editor;

mod sibling_connector;
mod tag_dialog_defaults;
mod tag_dialog_preferences;
mod unselected_tag_cap;

mod write_autocomplete;

mod network_job_control;

mod gallery_source;

mod namespace_sorts;
mod tag_list_display_types;

mod sidebar_context_cogs;
mod sort_cog;

mod command_palette;

mod read_autocomplete;

mod read_or;
mod system_or_activation;

mod manage_tag_counts;

mod frame_locations;
mod incremental_tagging;

mod notebook_tree;
mod tab_drag;
mod tab_presentation;

mod archive_repair;

mod file_history;

mod autocomplete_tabs;
