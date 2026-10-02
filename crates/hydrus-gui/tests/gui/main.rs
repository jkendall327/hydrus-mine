//! hydrus-gui's integration tests, one module per part of the GUI, built
//! as one test binary (each was its own, every one hundreds of megabytes
//! and linked separately).

mod common;

mod animation;
mod archive_delete;
mod collect;
mod duplicate_filter;
mod favourites;
mod import_files;
mod info_lines;
mod local_import_dialog;
mod main_menu;
mod main_shortcuts;
mod manage_tags;
mod media_actions;
mod media_shortcuts;
mod media_sort;
mod menu_bar;
mod mpv;
mod options_dialog;
mod options_window;
mod page_scroll;
mod popups;
mod predicate_editors;
mod ratings;
mod recent_predicates;
mod scanbar;
mod search_domains;
mod search_lock;
mod search_page;
mod selection;
mod session;
mod slideshow;
mod status_bar;
mod still;
mod thumbnail_icons;
mod thumbnail_menu;
mod thumbnail_ratings;
mod thumbnail_selection;
mod unlock;
mod viewer_menu;
mod viewer_top_frame;
mod viewer_window;
mod volume;
mod zoom;
