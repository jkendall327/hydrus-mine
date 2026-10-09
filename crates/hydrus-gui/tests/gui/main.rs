//! hydrus-gui's integration tests, one module per part of the GUI, built
//! as one test binary (each was its own, every one hundreds of megabytes
//! and linked separately).

mod common;
mod headless_lifetime;

mod about;
mod active_predicates;
mod animation;
mod animation_start;
mod api_update_toasts;
mod archive_delete;
mod archive_delete_playback;
mod auto_resolution_comparator_editors;
mod auto_resolution_preview;
mod auto_resolution_review;
mod auto_resolution_rules;
mod autocomplete_decoration_flags;
mod client_exit;
mod clipboard_urls;
mod collect;
mod database_maintenance;
mod downloader_definitions;
mod downloader_definitions_list;
mod downloader_lists;
mod downloader_update_times;
mod duplicate_colours;
mod duplicate_filter;
mod duplicate_filter_canvas;
mod duplicates_count;
mod duplicates_page;
mod duplicates_search_settings;
mod edit_subscription;
mod embedded_metadata;
mod emoji_fonts;
mod existing_tags_filter;
mod favourites;
mod file_log;
mod file_view_removal;
mod filename_rules;
mod filename_simple;
mod filesize_predicate;
mod folder_manager_lifecycle;
mod folders;
mod force_filetype;
mod force_idle;
mod formula_editors;
mod hash_predicate;
mod how_boned_window;
mod import_files;
mod import_folder_log;
mod import_options;
mod import_options_panel;
mod importer_list_menu;
mod info_lines;
mod list_drag;
mod login_script_controls;
mod login_workflows;
mod main_identity;
mod main_shortcuts;
mod manage_notes;
mod manage_ratings;
mod manage_tags;
mod manage_tags_cog;
mod manage_tags_sort;
mod manage_tags_viewer;
mod manage_times;
mod manage_times_replay;
mod manage_urls;
mod media_actions;
mod media_shortcuts;
mod media_sort;
mod menu_bar;
mod menu_choice_wheel;
mod merge_options;
mod mpv;
mod namespace_colours;
mod network_jobs_rows;
mod network_pause_paged;
mod normal_time_maintenance;
mod notebook_new_page;
mod notebook_refresh;
mod notebook_sessions;
mod notes_preferences;
mod options_downloading_checkers;
mod options_mpv_devices;
mod options_search_notes;
mod options_window;
mod page_chooser_options;
mod page_close_replay;
mod page_navigation_options;
mod page_scroll;
mod pages_menu_new_pages;
mod pages_menu_refresh;
mod parser_content_kinds;
mod physical_delete_delay;
mod popup_freeze;
mod popup_job_actions;
mod popup_width;
mod popups;
mod popups_collapse;
mod predicate_editors;
mod predicate_history;
mod preview_top_right;
mod preview_viewing;
mod rating_sizes;
mod ratings;
mod recent_predicates;
mod scanbar;
mod search_domains;
mod search_lock;
mod search_log;
mod search_page;
mod selected_deletion_records;
mod selected_viewing_stats;
mod session;
mod session_autosave;
mod session_backup_menu;
mod session_startup;
mod set_password;
mod shell_geometry;
mod shell_help_menu;
mod shell_modal_popups;
mod shell_popup_download;
mod shell_popups;
mod shell_tabs;
mod shortcut_capture;
mod sidecars;
mod simple_downloader;
mod simple_formulae_list;
mod slideshow;
mod status_app_busy;
mod status_bar;
mod still;
mod subscriptions;
mod subscriptions_dedupe;
mod subscriptions_duplicate;
mod subscriptions_separate;
mod system_sleep;
mod system_tray;
mod tag_filter_sibling_testing;
mod tag_service_tabs;
mod thumbnail_appearance;
mod thumbnail_cache;
mod thumbnail_icons;
mod thumbnail_menu;
mod thumbnail_navigation;
mod thumbnail_preview_selection;
mod thumbnail_ratings;
mod thumbnail_selection;
mod undelete_question;
mod unlock;
mod viewer_background;
mod viewer_menu;
mod viewer_top_frame;
mod viewer_window;
mod volume;
mod watcher_checker;
mod window_titles;
mod write_domain;
mod zoom;

mod export_files;
mod services_review;
mod tag_relationships;
mod tag_relationships_replay;

mod services_editor;

mod client_api_admin;
mod client_api_permissions;
mod parser_editors;
mod tag_display;
mod tag_display_replay;
mod tags_sync_menu;

mod network_sessions;

mod network_data;

mod domain_metadata;
mod downloader_interchange;
mod subscriptions_missing_logs;
mod suggested_tag_panels;
mod tag_filter_tooltips;
mod tag_migration;

mod manage_notes_replay;
mod regex_favourites;
mod regex_options_editor;
mod sidecar_node_window;
mod string_converter_window;
mod string_match_window;
mod string_processor_window;
mod tab_context_menu;
mod tag_filter_favourites;

mod downloader_display;
mod favourite_search_editor;

mod sibling_colours;
mod sibling_connector;
mod tag_dialog_defaults;
mod tag_dialog_preferences;
mod tag_namespace_order;
mod unselected_tag_cap;

mod write_autocomplete;
mod write_autocomplete_leaves;

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
mod file_maintenance_current;

mod autocomplete_tabs;
mod gui_colours;
mod gui_format;
mod preview_default_zoom;
mod related_tags_panel;
mod related_weight_table;
mod relationship_application;
mod relationship_exchange;

mod external_call_tests;
mod external_calls;
mod external_calls_editor;
mod external_calls_recorded;
mod open_externally;
mod open_externally_dispatch;
mod viewer_drag;
mod window_rescue;

mod viewer_tag_wheel;

mod idle_timeout_options;

mod import_work_slots;
mod viewing_maintenance;

mod local_transfer;

mod sidebar_layout;

mod hidden_page_preview;
mod or_connector;

mod image_cache;
mod image_colour;

mod debug_delayed_pages;
mod debug_delayed_popup;
mod debug_fetch;
mod debug_long_popup;
mod debug_menu_actions;
mod debug_session_reload;
mod duplicates_progress;
mod quick_export_directory;

mod archive_delete_policies;

mod ffmpeg_timeout;
mod radio_return;

mod database_backup_menu;
mod database_file_maintenance_new;
mod database_locations_menu;
mod database_menu_jobs;
mod database_orphan_files;
mod database_toggles;
mod dateparser_preview;
mod duplicate_hover_pin;
mod duplicates_lane_page;
mod file_log_actions;
mod files_io_export_files;
mod files_io_file_menu;
mod files_io_folder_editors;
mod files_io_import_review;
mod formula_exchange;
mod import_additional_tags;
mod import_locations;
mod import_options_fields;
mod media_file_actions;
mod media_ratings_counter;
mod media_selection_menu;
mod media_support;
mod media_viewer_navigation;
mod network_bandwidth_actions;
mod network_pause_menu;
mod options_gui_duplicates;
mod options_gui_frames;
mod options_gui_kept;
mod options_gui_misc;
mod options_gui_pages;
mod options_gui_ratings;
mod options_gui_ratings_examples;
mod options_gui_shortcuts;
mod options_gui_suggestions;
mod options_gui_support;
mod options_gui_tags;
mod options_media_hovers;
mod options_media_mpv;
mod options_media_playback;
mod options_media_slideshow;
mod options_media_support;
mod options_media_thumbnails;
mod options_media_zoom;
mod options_namespace_edit;
mod options_shortcut_sets;
mod options_system_consumers;
mod preview_audio;
mod search_pages_favourites;
mod search_pages_menu;
mod subscriptions_overwrite_checker;
mod tag_filter_removal;
mod tag_filter_replay;
mod video_buffer;
mod viewer_audio;
mod viewer_prefetch;
