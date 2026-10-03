//! The desktop client's workings without its windows: what hydrus-gui's
//! windows show and do (menus, editors, sorting, selection, the layout of
//! what is drawn over thumbnails, the options), as plain Rust over the
//! store. It builds and tests without Slint, so its tests (and mutation
//! testing them) don't wait on the windows' generated code. hydrus-gui
//! re-exports each module under its own name.

pub mod archive_delete;
pub mod audio;
pub mod auto_resolution_review;
pub mod auto_resolution_rules;
pub mod autocomplete;
pub mod checker_options;
pub mod collect;
pub mod domains;
pub mod duplicate_filter;
pub mod duplicates_page;
pub mod edit_subscription;
pub mod favourites;
pub mod file_log;
pub mod filename_tagging;
pub mod filetype_tree;
pub mod folders;
pub mod import_options_editor;
pub mod importer_menu;
pub mod info_lines;
pub mod list_selection;
pub mod local_import;
pub mod main_menu;
pub mod manage_tags;
pub mod media_actions;
pub mod merge_summary;
pub mod notes_editor;
pub mod options;
pub mod page_chooser;
pub mod predicate_editors;
pub mod ratings;
pub mod scanbar;
pub mod search_log;
pub mod selection;
pub mod session_saving;
pub mod sort;
pub mod status;
pub mod subscriptions_dedupe;
pub mod subscriptions_dialog;
pub mod subscriptions_list;
pub mod thumbnail_icons;
pub mod thumbnail_ratings;
