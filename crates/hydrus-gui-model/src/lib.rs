//! The desktop client's workings without its windows: what hydrus-gui's
//! windows show and do (menus, editors, sorting, selection, the layout of
//! what is drawn over thumbnails, the options), as plain Rust over the
//! store. It builds and tests without Slint, so its tests (and mutation
//! testing them) don't wait on the windows' generated code. hydrus-gui
//! re-exports each module under its own name.

pub mod archive_delete;
pub mod audio;
pub mod autocomplete;
pub mod collect;
pub mod domains;
pub mod duplicate_filter;
pub mod favourites;
pub mod info_lines;
pub mod local_import;
pub mod main_menu;
pub mod manage_tags;
pub mod media_actions;
pub mod options;
pub mod page_chooser;
pub mod predicate_editors;
pub mod ratings;
pub mod scanbar;
pub mod selection;
pub mod sort;
pub mod status;
pub mod thumbnail_icons;
pub mod thumbnail_ratings;
