//! hydrus-gui's integration tests, one module per part of the GUI, built
//! as one test binary (each was its own, every one hundreds of megabytes
//! and linked separately).

mod common;

mod animation;
mod archive_delete;
mod auto_resolution_rules;
mod collect;
mod downloader_lists;
mod duplicate_filter;
mod duplicates_page;
mod edit_subscription;
mod favourites;
mod file_log;
mod folders;
mod import_files;
mod import_options;
mod importer_list_menu;
mod info_lines;
mod main_shortcuts;
mod manage_tags;
mod media_actions;
mod media_shortcuts;
mod media_sort;
mod menu_bar;
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
mod slideshow;
mod status_bar;
mod still;
mod subscriptions;
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
