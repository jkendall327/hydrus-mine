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
mod duplicate_filter;
mod duplicates_page;
mod edit_subscription;
mod embedded_metadata;
mod favourites;
mod file_log;
mod folders;
mod force_filetype;
mod formula_editors;
mod import_files;
mod import_options;
mod importer_list_menu;
mod info_lines;
mod list_drag;
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
mod options_window;
mod page_scroll;
mod popups;
mod predicate_editors;
mod ratings;
mod recent_predicates;
mod scanbar;
mod search_domains;
mod search_lock;
mod search_log;
mod search_page;
mod session;
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
