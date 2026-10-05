//! The desktop client's workings without its windows: what hydrus-gui's
//! windows show and do (menus, editors, sorting, selection, the layout of
//! what is drawn over thumbnails, the options), as plain Rust over the
//! store. It builds and tests without Slint, so its tests (and mutation
//! testing them) don't wait on the windows' generated code. hydrus-gui
//! re-exports each module under its own name.

pub mod about;
pub mod archive_delete;
pub mod audio;
pub mod auto_resolution_preview;
pub mod auto_resolution_review;
pub mod auto_resolution_rules;
pub mod autocomplete;
pub mod checker_options;
pub mod clipboard_urls;
pub mod collect;
pub mod datetime_editor;
pub mod delete_files;
pub mod domains;
pub mod downloader_definitions;
pub mod downloader_interchange;
pub mod duplicate_colours;
pub mod duplicate_filter;
pub mod duplicates_page;
pub mod edit_subscription;
pub mod embedded_metadata;
pub mod export_files;
pub mod external_calls;
pub mod external_command;
pub mod favourites;
pub mod file_log;
pub mod filename_rules;
pub mod filename_tagging;
pub mod filetype_tree;
pub mod folders;
pub mod force_filetype;
pub mod formula_editors;
pub mod import_options_editor;
pub mod import_options_overwrite;
pub mod import_options_panel;
pub mod importer_menu;
pub mod info_lines;
pub mod list_selection;
pub mod local_import;
pub mod main_menu;
pub mod manage_tags;
pub mod media_actions;
pub mod merge_options_editor;
pub mod merge_summary;
pub mod namespace_colours;
pub mod notes_editor;
pub mod options;
pub mod page_chooser;
pub mod page_tree;
pub mod png_export;
pub mod predicate_editors;
pub mod predicate_history;
pub mod rating_sizes;
pub mod ratings;
pub mod ratings_editor;
pub mod related_weights;
pub mod scanbar;
pub mod search_log;
pub mod selection;
pub mod services_editor;
pub mod services_review;
pub mod session_lifecycle;
pub mod session_saving;
pub mod sidecar_editors;
pub mod sidecars;
pub mod simple_downloader;
pub mod sort;
pub mod status;
pub mod string_editors;
pub mod subscription_exchange;
pub mod subscription_quality;
pub mod subscriptions_dedupe;
pub mod subscriptions_dialog;
pub mod subscriptions_list;
pub mod tab_context;
pub mod tab_drag;
pub mod tag_filter_editor;
pub mod tag_suggestions;
pub mod thumbnail_icons;
pub mod thumbnail_navigation;
pub mod thumbnail_ratings;
pub mod times_editor;
pub mod urls_editor;

pub mod tag_relationships;

pub mod parser_editors;
pub mod parser_test_data;
pub mod tag_display;

pub mod client_api_admin;

pub mod network_sessions;

pub mod network_data;
pub mod tag_migration;

pub mod downloader_display;
pub mod login_workflows;
pub mod regex_favourites;

pub mod write_autocomplete;

pub mod network_job_control;
pub mod write_tag_menu;

pub mod gallery_source;
pub mod viewer_closing;

pub mod namespace_sorts;
pub mod viewer_cursor;

pub mod sort_cog;

pub mod command_palette;

mod palette_calculator;

pub mod viewing_statistics;

pub mod search_or;

pub mod frame_locations;
pub mod incremental_tagging;
pub mod tag_banner;

pub mod rating_example;
pub mod tab_presentation;

pub mod archive_repair;

pub mod file_history;

pub mod file_history_worker;

pub mod viewer_drag;
pub mod window_rescue;

pub mod gui_format;
pub mod viewer_tag_wheel;
