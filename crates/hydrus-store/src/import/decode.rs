//! Decoding a reference install's serialised settings into [`ImportInput`].
//!
//! The SQL half of the importer copies rows; this half turns the reference's
//! serialised objects (service settings, Client API keys, client options)
//! into native settings, via `hydrus-legacy`'s typed decoders.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Value as Json, json};

use hydrus_core::pages::{DownloaderKind, PageSort, PageSortBy};
use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::thumbnail::{ThumbnailScale, ThumbnailSettings};
use hydrus_core::{CanvasType, DuplicateType, ServiceKey, ServiceType, Sha256};
use hydrus_legacy::LegacyDb;
use hydrus_legacy::objects::predicates::{StarScale, predicate_with_scales};
use hydrus_legacy::objects::{self as legacy, ServiceConfig, TagRule};
use hydrus_legacy::readers::Service as LegacyService;

use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_core::url::UrlClasses;

use super::{ApiPermissionsRow, ImportInput, SubscriptionInput, settingless_kind};
use crate::autocomplete::{AutocompleteRules, AutocompleteSettings};
use crate::duplicates::{DuplicateFilterSettings, DuplicateMergeSettings, MergeOptions};
use crate::error::{Result, StoreError};
use crate::queues::{FileSeed, FileSeedMeta, GallerySeed, GallerySeedMeta, SeedStatus, SeedType};
use crate::services::{
    LikeRatingConfig, NumericalRatingConfig, PenBrush, RatingColours, RatingDisplay,
    RepositoryConfig, Rgb, ServerConfig, ServiceKind, StarAppearance, StarShape,
};
use crate::settings::{FavouriteTags, FileViewingStatistics, Setting};

/// Decode everything the importer needs from the reference install `db`.
pub fn decode_input(db: &LegacyDb) -> Result<ImportInput> {
    let mut input = ImportInput::default();
    // numerical rating services' scales, for the ratings stored searches test
    let mut scales: HashMap<ServiceKey, StarScale> = HashMap::new();
    for service in db.services()? {
        if let ServiceConfig::NumericalRating(c) = &service.config {
            scales.insert(service.key.clone(), (u64::from(c.num_stars), c.allow_zero));
        }
        if settingless_kind(service.service_type).is_none() {
            input
                .service_kinds
                .insert(service.id, service_kind(&service)?);
        }
    }
    if let Some(manager) = db.client_api_manager()? {
        input.api_permissions = manager.permissions.iter().map(api_permissions).collect();
    }
    let options = db.client_options()?;
    let legacy_options = db.legacy_options()?;

    let mut gallery = hydrus_core::subscriptions::GalleryDefaults::default();
    if let Some(value) = legacy_options.get("gallery_file_limit") {
        // (the reference stores "no limit" as None)
        gallery.file_limit = value.as_i64().and_then(|n| u64::try_from(n).ok());
    }
    if let Some(options) = &options {
        gallery.gug = options.default_gug();
    }
    insert_setting(&mut input, &gallery)?;

    // (the reference keeps "no limit" as None; its defaults fill missing keys)
    let limit = |key| {
        legacy_options
            .get(key)
            .and_then(hydrus_legacy::objects::YamlValue::as_i64)
            .and_then(|n| u64::try_from(n).ok())
    };
    let trash = crate::trash::TrashSettings {
        max_age_hours: limit("trash_max_age"),
        max_size_mb: limit("trash_max_size"),
    };
    insert_setting(&mut input, &trash)?;

    let mut folders = crate::settings::FolderSettings::default();
    if let Some(value) = legacy_options
        .get("delete_to_recycle_bin")
        .and_then(hydrus_legacy::objects::YamlValue::as_bool)
    {
        folders.delete_to_recycle_bin = value;
    }
    if let Some(options) = &options {
        for (key, field) in [
            (
                "pause_import_folders_sync",
                &mut folders.pause_import_folders,
            ),
            (
                "pause_export_folders_sync",
                &mut folders.pause_export_folders,
            ),
            (
                "copy_import_files_to_temp_dir",
                &mut folders.copy_import_files_to_temp_dir,
            ),
        ] {
            if let Some(&value) = options.booleans.get(key) {
                *field = value;
            }
        }
    }
    insert_setting(&mut input, &folders)?;
    let mut pauses = crate::settings::Pauses::default();
    if let Some(options) = &options {
        for (key, field) in pauses.by_option_name() {
            if let Some(&value) = options.booleans.get(key) {
                *field = value;
            }
        }
    }
    insert_setting(&mut input, &pauses)?;
    let boot = options
        .as_ref()
        .and_then(|o| o.booleans.get("boot_with_network_traffic_paused"))
        .copied()
        .unwrap_or(false);
    insert_setting(&mut input, &crate::settings::NetworkBootPause(boot))?;
    if let Some(&advanced) = options
        .as_ref()
        .and_then(|o| o.booleans.get("advanced_mode"))
    {
        insert_setting(&mut input, &crate::settings::AdvancedMode(advanced))?;
    }
    let mut notebooks = crate::sessions::NotebookSettings::default();
    if let Some(options) = &options {
        if let Some(value) = options.integers.get("close_page_focus_goes") {
            notebooks.close_focus_left = *value == 0;
        }
        if let Some(&value) = options.booleans.get("rename_page_of_pages_on_send") {
            notebooks.rename_sent_notebooks = value;
        }
    }
    insert_setting(&mut input, &notebooks)?;
    let mut tag_editing = crate::tag_editing::TagEditingSettings::default();
    if let Some(options) = &options {
        for (key, field) in [
            (
                "use_listbook_for_tag_service_panels",
                &mut tag_editing.use_listbook,
            ),
            (
                "show_parent_decorators_on_storage_taglists",
                &mut tag_editing.tag_list_show_parents,
            ),
            (
                "expand_parents_on_storage_taglists",
                &mut tag_editing.tag_list_expand_parents,
            ),
            (
                "show_sibling_decorators_on_storage_taglists",
                &mut tag_editing.tag_list_show_siblings,
            ),
        ] {
            if let Some(value) = options.booleans.get(key) {
                *field = *value;
            }
        }
        if let Some(&value) = options
            .booleans
            .get("save_default_tag_service_tab_on_change")
        {
            tag_editing.remember_service = value;
        }
        if let Some(&value) = options.booleans.get("ac_select_first_with_count") {
            tag_editing.select_first_with_count = value;
        }
        if let Some(&value) = options
            .booleans
            .get("skip_yesno_on_write_autocomplete_multiline_paste")
        {
            tag_editing.skip_multiline_paste_confirmation = value;
        }
        if let Some(&value) = options
            .booleans
            .get("show_parent_decorators_on_storage_autocomplete_taglists")
        {
            tag_editing.autocomplete_show_parents = value;
        }
        if let Some(&value) = options
            .booleans
            .get("expand_parents_on_storage_autocomplete_taglists")
        {
            tag_editing.autocomplete_expand_parents = value;
        }
        if let Some(&value) = options
            .booleans
            .get("show_sibling_decorators_on_storage_autocomplete_taglists")
        {
            tag_editing.autocomplete_show_siblings = value;
        }
        if let Some(&value) = options.integers.get("ac_write_list_height_num_chars") {
            tag_editing.autocomplete_list_height = u32::try_from(value).unwrap_or(11).clamp(1, 128);
        }
        if let Some(key) = options.keys.get("default_tag_service_tab") {
            tag_editing.default_service = ServiceKey::new(key.clone());
        }
    }
    insert_setting(&mut input, &tag_editing)?;
    let mut autocomplete_tabs = crate::settings::TagAutocompleteTabs::default();
    if let Some(value) = options.as_ref().and_then(|o| {
        o.noneable_integers
            .get("num_to_show_in_ac_dropdown_children_tab")
    }) {
        autocomplete_tabs.children_limit = value.map(|n| usize::try_from(n).unwrap_or(1).max(1));
    }
    if let Some(options) = &options {
        autocomplete_tabs.most_used = options
            .suggested_tags_favourites
            .iter()
            .map(|(key, tags)| (key.to_hex(), tags.clone()))
            .collect();
    }
    insert_setting(&mut input, &autocomplete_tabs)?;
    let notebook_creation = crate::settings::NotebookCreationSettings {
        rename_new_notebooks: options.as_ref().is_some_and(|options| {
            options.booleans.get("rename_page_of_pages_on_pick_new") == Some(&true)
        }),
    };
    let insertion = options
        .as_ref()
        .and_then(|options| options.integers.get("default_new_page_goes"))
        .and_then(|&code| crate::settings::PageInsertion::from_code(code))
        .unwrap_or_default();
    insert_setting(&mut input, &insertion)?;
    insert_setting(&mut input, &notebook_creation)?;
    let mut page_chooser = crate::settings::PageChooserSettings::default();
    if let Some(options) = &options {
        for (key, target) in [
            (
                "show_all_my_files_on_page_chooser",
                &mut page_chooser.show_combined,
            ),
            (
                "show_all_my_files_on_page_chooser_at_top",
                &mut page_chooser.combined_at_top,
            ),
            (
                "show_local_files_on_page_chooser",
                &mut page_chooser.show_storage,
            ),
            (
                "show_local_files_on_page_chooser_at_top",
                &mut page_chooser.storage_at_top,
            ),
        ] {
            if let Some(&value) = options.booleans.get(key) {
                *target = value;
            }
        }
    }
    insert_setting(&mut input, &page_chooser)?;
    let mut navigation = crate::settings::PageNavigationSettings::default();
    if let Some(options) = &options {
        navigation.confirm_all_closes = options
            .booleans
            .get("confirm_all_page_closes")
            .copied()
            .unwrap_or(false);
        navigation.focus_search_on_change = options
            .booleans
            .get("set_search_focus_on_page_change")
            .copied()
            .unwrap_or(false);
        if let Some(&value) = options.integers.get("page_nav_history_max_entries") {
            navigation.history_entries = u16::try_from(value.clamp(1, 1000)).unwrap_or(100);
        }
    }
    insert_setting(&mut input, &navigation)?;
    let import_ui = crate::settings::ImportOptionsUiSettings {
        simple: options
            .as_ref()
            .and_then(|options| options.booleans.get("import_options_simple_mode"))
            .copied()
            .unwrap_or(true),
    };
    insert_setting(&mut input, &import_ui)?;
    let mut lifecycle = crate::settings::GuiSessionSettings::default();
    if let Some(value) = legacy_options.get("default_gui_session") {
        lifecycle.startup = value
            .as_str()
            .filter(|&name| name != "just a blank page")
            .map(str::to_owned);
    }
    if let Some(options) = &options {
        if let Some(&period) = options.integers.get("last_session_save_period_minutes") {
            lifecycle.autosave_minutes = u16::try_from(period.clamp(1, 1440)).unwrap_or(5);
        }
        if let Some(&only) = options.booleans.get("only_save_last_session_during_idle") {
            lifecycle.only_during_idle = only;
        }
        if let Some(&warn) = options.booleans.get("show_session_size_warnings") {
            lifecycle.warn_large_session = warn;
        }
    }
    insert_setting(&mut input, &lifecycle)?;
    let mut idle = crate::settings::GuiIdleSettings {
        user_seconds: limit("idle_period"),
        mouse_seconds: limit("idle_mouse_period"),
        ..crate::settings::GuiIdleSettings::default()
    };
    if let Some(enabled) = legacy_options
        .get("idle_normal")
        .and_then(hydrus_legacy::objects::YamlValue::as_bool)
    {
        idle.enabled = enabled;
    }
    if let Some(options) = &options {
        idle.api_seconds = options
            .noneable_integers
            .get("idle_mode_client_api_timeout")
            .copied()
            .flatten()
            .and_then(|seconds| u64::try_from(seconds).ok());
    }
    insert_setting(&mut input, &idle)?;
    let mut backups = crate::session_backups::SessionBackupSettings::default();
    if let Some(options) = &options
        && let Some(value) = options.integers.get("number_of_gui_session_backups")
    {
        backups.keep = usize::try_from(*value).unwrap_or(1).clamp(1, 32);
    }
    insert_setting(&mut input, &backups)?;
    let mut gui = crate::settings::GuiSettings::default();
    if let Some(options) = &options
        && let Some(value) = options.strings.get("app_display_name")
    {
        gui.application_display_name.clone_from(value);
    }
    if let Some(value) = legacy_options
        .get("confirm_client_exit")
        .and_then(hydrus_legacy::objects::YamlValue::as_bool)
    {
        gui.confirm_exit = value;
    }
    insert_setting(&mut input, &gui)?;
    let mut preferences = crate::settings::OptionsPreferences::default();
    if let Some(options) = &options {
        if let Some(&value) = options.booleans.get("remember_options_window_panel") {
            preferences.remember_panel = value;
        }
        if let Some(&value) = options.booleans.get("options_search_bar_top_of_window") {
            preferences.search_at_top = value;
        }
        if let Some(value) = options.strings.get("last_options_window_panel") {
            preferences.last_panel.clone_from(value);
        }
    }
    insert_setting(&mut input, &preferences)?;
    let mut delete_lock = crate::delete_lock::DeleteLock::default();
    if let Some(options) = &options {
        for (key, field) in delete_lock.by_option_name() {
            if let Some(&value) = options.booleans.get(key) {
                *field = value;
            }
        }
    }
    insert_setting(&mut input, &delete_lock)?;
    let mut maintenance = crate::file_maintenance::FileMaintenanceSettings::default();
    if let Some(options) = &options {
        let m = &mut maintenance;
        for (key, field) in [
            ("file_maintenance_during_idle", &mut m.during_idle),
            ("file_maintenance_during_active", &mut m.during_active),
        ] {
            if let Some(&value) = options.booleans.get(key) {
                *field = value;
            }
        }
        for (key, field) in [
            ("file_maintenance_idle_throttle_files", &mut m.idle_files),
            (
                "file_maintenance_idle_throttle_time_delta",
                &mut m.idle_seconds,
            ),
            (
                "file_maintenance_active_throttle_files",
                &mut m.active_files,
            ),
            (
                "file_maintenance_active_throttle_time_delta",
                &mut m.active_seconds,
            ),
        ] {
            if let Some(value) = options
                .integers
                .get(key)
                .and_then(|&v| u64::try_from(v).ok())
            {
                *field = value;
            }
        }
    }
    insert_setting(&mut input, &maintenance)?;
    let lock = hydrus_core::lock::LockPassword {
        sha256: legacy_options.password_hash().map(hex::encode),
    };
    insert_setting(&mut input, &lock)?;
    if let Some(options) = &options {
        insert_setting(&mut input, &tag_presentation(options))?;
    }
    insert_setting(
        &mut input,
        &namespace_colours(&legacy_options, options.as_ref()),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::media_viewer_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::info_line_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::thumbnail_rating_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(|o| o.recent_predicates(&|key| scales.get(key).copied()))
            .transpose()
            .map_err(|e| StoreError::Invalid(format!("recent predicates: {e}")))?
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::audio_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::slideshow_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::page_name_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::downloader_page_settings)
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(|o| crate::settings::SimpleDownloaderFormulae {
                formulae: o.simple_downloader_formulae.clone(),
                favourite: o
                    .strings
                    .get("favourite_simple_downloader_formula")
                    .cloned()
                    .unwrap_or_default(),
            })
            .unwrap_or_default(),
    )?;
    insert_setting(
        &mut input,
        &options
            .as_ref()
            .map(legacy::ClientOptions::window_settings)
            .unwrap_or_default(),
    )?;
    let mut handling = crate::settings::FileHandlingSettings::default();
    if let Some(options) = &options {
        let boolean = |key: &str| options.booleans.get(key).copied();
        handling.comic_book_detection =
            boolean("allow_comic_book_archive_detection").unwrap_or(handling.comic_book_detection);
        handling.do_not_chmod = boolean("do_not_do_chmod_mode").unwrap_or(handling.do_not_chmod);
        handling.prefix_hash_when_copying =
            boolean("prefix_hash_when_copying").unwrap_or(handling.prefix_hash_when_copying);
        if let Some(level) = options
            .integers
            .get("file_has_transparency_strictness")
            .and_then(|&v| u8::try_from(v).ok())
            .filter(|&v| v <= 2)
        {
            handling.transparency_strictness = level;
        }
    }
    insert_setting(&mut input, &handling)?;
    let mut pages = crate::settings::PageSettings::default();
    if let Some(uses_all_my_files) = options.as_ref().and_then(|o| {
        o.booleans
            .get("open_files_to_duplicate_filter_uses_all_my_files")
            .copied()
    }) {
        pages.duplicate_filter_uses_all_my_files = uses_all_my_files;
    }
    insert_setting(&mut input, &pages)?;
    let mut sorts = hydrus_core::pages::SortSettings::default();
    if let Some(options) = &options {
        if let Some(sort) = &options.default_sort {
            sorts.default_sort = page_sort(sort);
        }
        if let Some(sort) = &options.fallback_sort {
            sorts.fallback_sort = page_sort(sort);
        }
        sorts.namespace_sorts = options
            .default_namespace_sorts
            .iter()
            .map(page_sort)
            .collect();
        if let Some(collect) = &options.default_collect {
            sorts.default_collect = page_collect(collect);
        }
        if let Some(&save) = options.booleans.get("save_page_sort_on_change") {
            sorts.save_page_sort_on_change = save;
        }
    }
    insert_setting(&mut input, &sorts)?;
    let mut network = crate::network::NetworkSettings::default();
    if let Some(options) = &options {
        let n = &mut network;
        let int = |key: &str| options.integers.get(key).copied();
        let unsigned = |key: &str| int(key).and_then(|v| u64::try_from(v).ok());
        let small = |key: &str| int(key).and_then(|v| u32::try_from(v).ok());
        let count = |key: &str| int(key).and_then(|v| usize::try_from(v).ok());
        let boolean = |key: &str| options.booleans.get(key).copied();
        let string = |key: &str| options.noneable_strings.get(key).cloned();
        n.network_timeout = unsigned("network_timeout").unwrap_or(n.network_timeout);
        n.connection_error_wait_time =
            unsigned("connection_error_wait_time").unwrap_or(n.connection_error_wait_time);
        n.serverside_bandwidth_wait_time =
            unsigned("serverside_bandwidth_wait_time").unwrap_or(n.serverside_bandwidth_wait_time);
        n.max_connection_attempts =
            small("max_connection_attempts_allowed").unwrap_or(n.max_connection_attempts);
        n.max_get_attempts =
            small("max_request_attempts_allowed_get").unwrap_or(n.max_get_attempts);
        n.max_jobs = count("max_network_jobs").unwrap_or(n.max_jobs);
        n.max_jobs_per_domain =
            count("max_network_jobs_per_domain").unwrap_or(n.max_jobs_per_domain);
        n.verify_https = boolean("verify_regular_https").unwrap_or(n.verify_https);
        n.domain_error_number =
            count("domain_network_infrastructure_error_number").unwrap_or(n.domain_error_number);
        n.domain_error_window =
            int("domain_network_infrastructure_error_time_delta").unwrap_or(n.domain_error_window);
        if let Some(proxy) = string("http_proxy") {
            n.http_proxy = proxy;
        }
        if let Some(proxy) = string("https_proxy") {
            n.https_proxy = proxy;
        }
        if let Some(hosts) = string("no_proxy") {
            n.no_proxy = hosts;
        }
        n.downloader_network_error_delay =
            unsigned("downloader_network_error_delay").unwrap_or(n.downloader_network_error_delay);
        n.subscription_network_error_delay =
            int("subscription_network_error_delay").unwrap_or(n.subscription_network_error_delay);
        n.subscription_other_error_delay =
            int("subscription_other_error_delay").unwrap_or(n.subscription_other_error_delay);
        if let Some(threshold) = options
            .noneable_integers
            .get("subscription_file_error_cancel_threshold")
        {
            n.subscription_file_error_cancel_threshold =
                threshold.and_then(|value| u64::try_from(value).ok());
        }
        n.process_subs_in_random_order =
            boolean("process_subs_in_random_order").unwrap_or(n.process_subs_in_random_order);
        n.max_simultaneous_subscriptions =
            small("max_simultaneous_subscriptions").unwrap_or(n.max_simultaneous_subscriptions);
        n.gug_percent_twenty_is_space = boolean("replace_percent_twenty_with_space_in_gug_input")
            .unwrap_or(n.gug_percent_twenty_is_space);
        n.detect_sleep = boolean("do_sleep_check").unwrap_or(n.detect_sleep);
        n.wake_delay_period = unsigned("wake_delay_period").unwrap_or(n.wake_delay_period);
    }
    insert_setting(&mut input, &network)?;
    let clipboard = crate::settings::ClipboardUrls {
        watchers: options
            .as_ref()
            .is_some_and(|o| o.booleans.get("watch_clipboard_for_watcher_urls") == Some(&true)),
        other_recognised: options.as_ref().is_some_and(|o| {
            o.booleans.get("watch_clipboard_for_other_recognised_urls") == Some(&true)
        }),
    };
    insert_setting(&mut input, &clipboard)?;
    let mut export = crate::settings::ExportSettings::default();
    if let Some(options) = &options {
        if let Some(phrase) = options.strings.get("export_phrase") {
            export.phrase.clone_from(phrase);
        }
        if let Some(&n) = options.integers.get("export_filename_character_limit") {
            export.filename_character_limit = n;
        }
        if let Some(&n) = options.noneable_integers.get("export_path_character_limit") {
            export.path_character_limit = n;
        }
        if let Some(&n) = options
            .noneable_integers
            .get("export_dirname_character_limit")
        {
            export.dirname_character_limit = n;
        }
        if let Some(&b) = options
            .booleans
            .get("always_apply_ntfs_export_filename_rules")
        {
            export.always_apply_ntfs_rules = b;
        }
    }
    insert_setting(&mut input, &export)?;

    let mut thumbnails = ThumbnailSettings::default();
    if let Some((w, h)) = legacy_options.thumbnail_dimensions() {
        thumbnails.bounding_width = positive(w, "thumbnail width")?;
        thumbnails.bounding_height = positive(h, "thumbnail height")?;
    }
    if let Some(options) = &options {
        if let Some(&code) = options.integers.get("thumbnail_scale_type") {
            thumbnails.scale = match code {
                0 => ThumbnailScale::DownOnly,
                1 => ThumbnailScale::ToFit,
                2 => ThumbnailScale::ToFill,
                other => {
                    return Err(StoreError::Invalid(format!(
                        "unknown thumbnail scale type {other}"
                    )));
                }
            };
        }
        if let Some(percent) = options
            .integers
            .get("video_thumbnail_percentage_in")
            .and_then(|&p| u32::try_from(p).ok())
        {
            thumbnails.video_percentage_in = percent;
        }
        if let Some(&dpr) = options.integers.get("thumbnail_dpr_percent") {
            thumbnails.dpr_percent = positive(dpr, "thumbnail dpr percent")?;
        }
        let mut layout = crate::settings::ThumbnailLayout::default();
        let pixels = |key: &str| {
            options
                .integers
                .get(key)
                .and_then(|&n| u32::try_from(n).ok())
        };
        if let Some(border) = pixels("thumbnail_border") {
            layout.border = border;
        }
        if let Some(margin) = pixels("thumbnail_margin") {
            layout.margin = margin;
        }
        insert_setting(&mut input, &layout)?;
        let mut search_defaults = crate::settings::SearchDefaults::default();
        if let Some(key) = options.keys.get("default_tag_service_search_page") {
            search_defaults.tag_service = ServiceKey::new(key.clone());
        }
        if let Some(location) = &options.default_local_location_context {
            search_defaults.local_location = hydrus_core::search::context::LocationContext::new(
                location.current.iter().cloned(),
                location.deleted.iter().cloned(),
            );
        }
        insert_setting(&mut input, &search_defaults)?;
        let mut file_search = crate::settings::FileSearchSettings::default();
        file_search.search_immediately = options
            .booleans
            .get("default_search_synchronised")
            .copied()
            .unwrap_or(file_search.search_immediately);
        file_search.show_system_everything = options
            .booleans
            .get("show_system_everything")
            .copied()
            .unwrap_or(file_search.show_system_everything);
        file_search.float_autocomplete = options
            .booleans
            .get("autocomplete_float_main_gui")
            .copied()
            .unwrap_or(file_search.float_autocomplete);
        if let Some(rows) = options
            .integers
            .get("active_search_predicates_height_num_chars")
        {
            file_search.active_predicate_rows = (*rows).clamp(1, 128) as u32;
        }
        if let Some(rows) = options.integers.get("ac_read_list_height_num_chars") {
            file_search.autocomplete_rows = (*rows).clamp(1, 128) as u32;
        }
        if let Some(limit) = options.noneable_integers.get("forced_search_limit") {
            file_search.implicit_limit = limit.map(|value| value.clamp(1, 100_000_000) as u64);
        }
        file_search.refresh_limited_sort = options
            .booleans
            .get("refresh_search_page_on_system_limited_sort_changed")
            .copied()
            .unwrap_or(file_search.refresh_limited_sort);
        insert_setting(&mut input, &file_search)?;
        let mut viewer_canvas = crate::settings::ViewerCanvasSettings::default();
        for (key, field) in [
            (
                "media_viewer_recenter_media_on_window_resize",
                &mut viewer_canvas.recenter_on_resize,
            ),
            (
                "draw_transparency_checkerboard_media_canvas",
                &mut viewer_canvas.transparency_checkerboard,
            ),
            (
                "draw_transparency_checkerboard_as_greenscreen",
                &mut viewer_canvas.transparency_greenscreen,
            ),
        ] {
            if let Some(value) = options.booleans.get(key) {
                *field = *value;
            }
        }
        if let Some(value) = options.integers.get("animated_scanbar_height") {
            viewer_canvas.seek_height = (*value).clamp(1, 255) as u32;
        }
        if let Some(value) = options
            .noneable_integers
            .get("animated_scanbar_hide_height")
        {
            viewer_canvas.seek_hidden_height = value.map(|height| height.clamp(1, 255) as u32);
        }
        if let Some(value) = options.integers.get("animated_scanbar_nub_width") {
            viewer_canvas.seek_nub_width = (*value).clamp(1, 63) as u32;
        }
        insert_setting(&mut input, &viewer_canvas)?;
        let mut viewer_cursor = crate::settings::ViewerCursorSettings::default();
        if let Some(value) = options
            .noneable_integers
            .get("media_viewer_cursor_autohide_time_ms")
        {
            viewer_cursor.autohide_ms = value.map(|delay| delay.clamp(100, 100000) as u32);
        }
        insert_setting(&mut input, &viewer_cursor)?;
        let mut viewer_background = crate::settings::ViewerBackgroundSettings::default();
        for (key, target) in [
            (
                "draw_tags_hover_in_media_viewer_background",
                &mut viewer_background.tags,
            ),
            (
                "draw_top_hover_in_media_viewer_background",
                &mut viewer_background.information,
            ),
            (
                "draw_top_right_hover_in_media_viewer_background",
                &mut viewer_background.ratings,
            ),
            (
                "draw_notes_hover_in_media_viewer_background",
                &mut viewer_background.notes,
            ),
        ] {
            if let Some(&value) = options.booleans.get(key) {
                *target = value;
            }
        }
        insert_setting(&mut input, &viewer_background)?;
        let mut viewer_closing = crate::settings::ViewerClosingSettings::default();
        for (key, field) in [
            (
                "focus_media_tab_on_viewer_close_if_possible",
                &mut viewer_closing.reselect_page,
            ),
            (
                "focus_media_thumb_on_viewer_close",
                &mut viewer_closing.select_exit_media,
            ),
            (
                "activate_main_gui_on_focusing_viewer_close",
                &mut viewer_closing.activate_focusing,
            ),
            (
                "activate_main_gui_on_viewer_close",
                &mut viewer_closing.activate_always,
            ),
        ] {
            if let Some(value) = options.booleans.get(key) {
                *field = *value;
            }
        }
        insert_setting(&mut input, &viewer_closing)?;
        let mut viewer_focus = crate::settings::ViewerFocusSettings::default();
        for (key, field) in [
            (
                "animated_scanbar_pop_in_requires_focus",
                &mut viewer_focus.seek_requires_focus,
            ),
            (
                "hover_windows_need_window_focus_to_pop_in",
                &mut viewer_focus.hovers_require_focus,
            ),
        ] {
            if let Some(value) = options.booleans.get(key) {
                *field = *value;
            }
        }
        insert_setting(&mut input, &viewer_focus)?;
        let mut viewer_pointer = crate::settings::ViewerPointerSettings::default();
        for (key, field) in [
            (
                "disallow_media_drags_on_duration_media",
                &mut viewer_pointer.disallow_duration_drag,
            ),
            ("hide_canvas_drags", &mut viewer_pointer.hide_during_drag),
        ] {
            if let Some(value) = options.booleans.get(key) {
                *field = *value;
            }
        }
        insert_setting(&mut input, &viewer_pointer)?;
        let mut viewer_hovers = crate::settings::ViewerHoverSettings::default();
        for (key, field) in [
            (
                "disable_tags_hover_in_media_viewer",
                &mut viewer_hovers.tags,
            ),
            (
                "disable_top_right_hover_in_media_viewer",
                &mut viewer_hovers.ratings,
            ),
            (
                "disable_notes_hover_in_media_viewer",
                &mut viewer_hovers.notes,
            ),
        ] {
            if let Some(value) = options.booleans.get(key) {
                *field = !*value;
            }
        }
        if let Some(value) = options
            .booleans
            .get("draw_bottom_right_index_in_media_viewer_background")
        {
            viewer_hovers.index_background = *value;
        }
        insert_setting(&mut input, &viewer_hovers)?;
        let mut summaries = hydrus_core::tag_summary::TagSummaries::default();
        for (name, field) in [
            ("thumbnail_top", &mut summaries.thumbnail_top),
            (
                "thumbnail_bottom_right",
                &mut summaries.thumbnail_bottom_right,
            ),
            ("media_viewer_top", &mut summaries.media_viewer_top),
        ] {
            if let Some(generator) = options.tag_summary_generators.get(name) {
                generator.clone_into(field);
            }
        }
        insert_setting(&mut input, &summaries)?;
        if let Some(tags) = options.string_lists.get("favourite_tags") {
            insert_setting(&mut input, &FavouriteTags(tags.clone()))?;
        }
        let mut viewing = FileViewingStatistics::default();
        if let Some(&active) = options.booleans.get("file_viewing_statistics_active") {
            viewing.active = active;
        }
        if let Some(codes) = options
            .integer_lists
            .get("file_viewing_stats_interesting_canvas_types")
        {
            viewing.interesting_canvases = codes
                .iter()
                .map(|&code| {
                    u8::try_from(code)
                        .ok()
                        .and_then(CanvasType::from_code)
                        .ok_or_else(|| StoreError::Invalid(format!("unknown canvas type {code}")))
                })
                .collect::<Result<_>>()?;
        }
        insert_setting(&mut input, &viewing)?;
        let mut similar = crate::similar::SimilarFilesSettings::default();
        if let Some(&d) = options
            .integers
            .get("similar_files_duplicate_pairs_search_distance")
        {
            similar.search_distance = u32::try_from(d).unwrap_or(0);
        }
        if let Some(&b) = options
            .booleans
            .get("maintain_similar_files_duplicate_pairs_during_active")
        {
            similar.during_active = b;
        }
        if let Some(&b) = options
            .booleans
            .get("maintain_similar_files_duplicate_pairs_during_idle")
        {
            similar.during_idle = b;
        }
        insert_setting(&mut input, &similar)?;
        let mut auto = crate::duplicates::auto::AutoResolutionSettings::default();
        for (key, field) in [
            (
                "duplicates_auto_resolution_during_active",
                &mut auto.during_active,
            ),
            (
                "duplicates_auto_resolution_during_idle",
                &mut auto.during_idle,
            ),
        ] {
            if let Some(&b) = options.booleans.get(key) {
                *field = b;
            }
        }
        for (key, field) in [
            (
                "duplicates_auto_resolution_work_time_ms_active",
                &mut auto.work_time_ms_active,
            ),
            (
                "duplicates_auto_resolution_work_time_ms_idle",
                &mut auto.work_time_ms_idle,
            ),
            (
                "duplicates_auto_resolution_rest_percentage_active",
                &mut auto.rest_percentage_active,
            ),
            (
                "duplicates_auto_resolution_rest_percentage_idle",
                &mut auto.rest_percentage_idle,
            ),
        ] {
            if let Some(&n) = options.integers.get(key) {
                *field = u32::try_from(n).unwrap_or(0);
            }
        }
        insert_setting(&mut input, &auto)?;
        let mut checkers = hydrus_core::subscriptions::CheckerDefaults::default();
        if let Some(c) = &options.default_subscription_checker_options {
            checkers.subscriptions = c.clone();
        }
        if let Some(c) = &options.default_watcher_checker_options {
            checkers.watchers = c.clone();
        }
        insert_setting(&mut input, &checkers)?;
        if let Some(stored) = &options.duplicate_action_options {
            let merge = duplicate_merge_settings(stored)?;
            insert_setting(&mut input, &merge)?;
        }
        let mut filter = DuplicateFilterSettings::default();
        if let Some(&size) = options.integers.get("duplicate_filter_max_batch_size") {
            filter.max_batch_size = positive(size, "duplicate filter batch size")?;
        }
        if let Some(&size) = options
            .noneable_integers
            .get("duplicate_filter_auto_commit_batch_size")
        {
            filter.auto_commit_batch_size = size.map(|n| u32::try_from(n).unwrap_or(0));
        }
        if let Some(&advanced) = options.booleans.get("advanced_mode") {
            filter.merge_alternates = advanced;
        }
        for (name, score) in filter.scores.by_option_name() {
            if let Some(&n) = options.integers.get(name) {
                *score = i32::try_from(n).unwrap_or(0);
            }
        }
        insert_setting(&mut input, &filter)?;
    }
    insert_setting(&mut input, &thumbnails)?;
    if let Some(manager) = db.tag_display_manager()? {
        insert_setting(&mut input, &autocomplete_settings(&manager))?;
        insert_setting(&mut input, &tag_display_filters(&manager))?;
        let services = manager
            .autocomplete_options
            .iter()
            .map(|o| {
                (
                    o.service_key.to_hex(),
                    crate::tag_display_config::AutocompleteOptions {
                        write_tag_service: o.write_autocomplete_tag_domain.clone(),
                        override_location: o.override_write_autocomplete_location_context,
                        write_location: hydrus_core::search::context::LocationContext::new(
                            o.write_autocomplete_location_context
                                .current
                                .iter()
                                .cloned(),
                            o.write_autocomplete_location_context
                                .deleted
                                .iter()
                                .cloned(),
                        ),
                        fetch_automatically: o.fetch_results_automatically,
                        exact_match_threshold: o
                            .exact_match_character_threshold
                            .and_then(|n| u16::try_from(n).ok())
                            .filter(|n| (1..=256).contains(n)),
                    },
                )
            })
            .collect();
        insert_setting(
            &mut input,
            &crate::tag_display_config::AutocompleteWidgetSettings { services },
        )?;
    }
    network_input(db, &mut input)?;
    bandwidth_input(db, options.as_ref(), &mut input)?;
    match db.url_class_settings() {
        Ok(Some(mut url_classes)) => {
            url_classes.collapse_leading_slashes = options
                .as_ref()
                .and_then(|o| o.booleans.get("remove_leading_url_double_slashes").copied())
                .unwrap_or(false);
            insert_setting(&mut input, &url_classes)?;
        }
        Ok(None) => {}
        // e.g. URL classes stored at an old version: the verbatim copy is
        // kept, and URLs are treated as unclassified until support is added
        Err(e) => input
            .warnings
            .push(format!("URL classes were not converted: {e}")),
    }
    match db.import_options_manager() {
        Ok(Some(manager)) => insert_setting(&mut input, &manager)?,
        // (every v688 database has one; a new client's defaults are used)
        Ok(None) => {}
        Err(e) => input.warnings.push(format!(
            "Import options were not converted, so the defaults apply: {e}"
        )),
    }
    match db.downloaders() {
        Ok(Some(downloaders)) => {
            for u in &downloaders.unconverted {
                input.warnings.push(format!(
                    "The {} \"{}\" was not converted: {}",
                    u.kind, u.name, u.reason
                ));
            }
            insert_setting(&mut input, &downloaders)?;
        }
        Ok(None) => {}
        Err(e) => input.warnings.push(format!(
            "Downloaders (GUGs and parsers) were not converted: {e}"
        )),
    }
    match db.subscriptions() {
        Ok(subscriptions) => {
            for (name, decoded) in subscriptions {
                match decoded {
                    Ok(s) => {
                        let converted = subscription_input(&s, &mut input.warnings);
                        input.subscriptions.push(converted);
                    }
                    Err(e) => input.warnings.push(format!(
                        "Subscription \"{name}\" could not be read, so it was not converted (the original \
                         is kept): {e}"
                    )),
                }
            }
        }
        Err(e) => input
            .warnings
            .push(format!("Subscriptions were not converted: {e}")),
    }
    session(
        db,
        &legacy_options,
        &|key| scales.get(key).copied(),
        &mut input,
    );
    other_sessions(
        db,
        &legacy_options,
        &|key| scales.get(key).copied(),
        &mut input,
    );
    match db.favourite_search_manager() {
        Ok(Some(manager)) => {
            let mut converted = Vec::new();
            for f in manager.searches {
                match file_search(&f.file_search_context, &|key| scales.get(key).copied()) {
                    Ok(search) => converted.push(hydrus_core::pages::FavouriteSearch {
                        folder: f.folder,
                        name: f.name,
                        search,
                        synchronised: f.synchronised,
                        sort: f.media_sort.as_ref().map(page_sort),
                        collect: f.media_collect.as_ref().map(page_collect),
                    }),
                    Err(e) => input.warnings.push(format!(
                        "The favourite search \"{}\" searches for something hydrus-rs can't, \
                         so it was not converted: {e}",
                        f.name
                    )),
                }
            }
            insert_setting(&mut input, &crate::settings::FavouriteSearches(converted))?;
        }
        Ok(None) => {}
        Err(e) => input
            .warnings
            .push(format!("Favourite searches were not converted: {e}")),
    }
    match db.export_folders() {
        Ok(folders) => {
            let mut converted = Vec::new();
            for (name, decoded) in folders {
                let folder = decoded.map_err(|e| e.to_string()).and_then(|f| {
                    Ok(hydrus_parse::folders::ExportFolder {
                        search: file_search(&f.search, &|key| scales.get(key).copied())?,
                        name: f.name,
                        path: f.path,
                        export_type: if f.export_type == 1 {
                            hydrus_parse::folders::ExportType::Synchronise
                        } else {
                            hydrus_parse::folders::ExportType::Regular
                        },
                        delete_from_client_after_export: f.delete_from_client_after_export,
                        export_symlinks: f.export_symlinks,
                        routers: f.routers,
                        run_regularly: f.run_regularly,
                        period: f.period,
                        phrase: f.phrase,
                        last_checked: f.last_checked,
                        run_now: f.run_now,
                        last_error: f.last_error,
                        show_working_popup: f.show_working_popup,
                        overwrite_sidecars_on_next_run: f.overwrite_sidecars_on_next_run,
                        always_overwrite_sidecars: f.always_overwrite_sidecars,
                    })
                });
                match folder {
                    Ok(f) => converted.push(f),
                    Err(e) => input.warnings.push(format!(
                        "Export folder \"{name}\" was not converted (the original is kept): {e}"
                    )),
                }
            }
            insert_setting(&mut input, &crate::settings::ExportFolders(converted))?;
        }
        Err(e) => input
            .warnings
            .push(format!("Export folders were not converted: {e}")),
    }
    match db.import_folders() {
        Ok(folders) => {
            for (name, decoded) in folders {
                match decoded {
                    Ok(f) => {
                        let warnings = &mut input.warnings;
                        let file_seeds = f
                            .file_seeds
                            .iter()
                            .filter_map(|seed| file_seed(seed, None, warnings))
                            .collect();
                        input.import_folders.push(super::ImportFolderInput {
                            name: f.name,
                            settings: f.settings,
                            options: f.options,
                            paused: f.paused,
                            file_seeds,
                        });
                    }
                    Err(e) => input.warnings.push(format!(
                        "Import folder \"{name}\" could not be read, so it was not converted (the original \
                         is kept): {e}"
                    )),
                }
            }
        }
        Err(e) => input
            .warnings
            .push(format!("Import folders were not converted: {e}")),
    }
    match db.auto_resolution_rules() {
        Ok(rules) => {
            for (name, decoded) in rules {
                let converted = decoded.map_err(|e| e.to_string()).and_then(|r| {
                    auto_resolution_rule(&r, &|key| scales.get(key).copied())
                        .map(|rule| (r.id, rule))
                });
                match converted {
                    Ok(rule) => input.auto_resolution_rules.push(rule),
                    Err(e) => input.warnings.push(format!(
                        "Duplicates auto-resolution rule \"{name}\" was not converted (the original is kept): {e}"
                    )),
                }
            }
        }
        Err(e) => input.warnings.push(format!(
            "Duplicates auto-resolution rules were not converted: {e}"
        )),
    }
    match db.logins() {
        Ok(Some(logins)) => {
            let active: Vec<_> = logins.into_iter().filter(|l| l.active).collect();
            if !active.is_empty() {
                let list: Vec<String> = active
                    .iter()
                    .map(|l| format!("{} ({})", l.domain, l.script))
                    .collect();
                input.warnings.push(format!(
                    "hydrus logged in to these sites with login scripts, which hydrus-rs doesn't run: {}. Their requests carry only the cookies you have for them, so keep those fresh (Hydrus Companion can send them)",
                    list.join(", ")
                ));
                let domains =
                    crate::network::LoginDomains(active.into_iter().map(|l| l.domain).collect());
                insert_setting(&mut input, &domains)?;
            }
        }
        Ok(None) => {}
        Err(e) => input
            .warnings
            .push(format!("The login settings could not be read: {e}")),
    }
    let callers = external_program_users(&input);
    if !callers.is_empty() {
        input.warnings.push(format!(
            "Some import options run a program on each imported file, which hydrus-rs doesn't do yet, so they won't run: {} (the options are kept)",
            callers.join("; ")
        ));
    }
    Ok(input)
}

/// Where import options are set to run a program on each imported file
/// (`DoExternalProgramCalls`).
pub(super) fn external_program_users(input: &ImportInput) -> Vec<String> {
    use hydrus_core::import_options::{ImportOptionsManager, ImportOptionsSlice};
    let calls = |s: &ImportOptionsSlice| {
        s.external_programs
            .as_ref()
            .is_some_and(|e| e.stored.is_some())
    };
    let mut out = Vec::new();
    if let Some(Ok(manager)) = input
        .settings
        .get(ImportOptionsManager::KEY)
        .map(|v| serde_json::from_value::<ImportOptionsManager>(v.clone()))
    {
        for (caller, slice) in &manager.caller_defaults {
            if calls(slice) {
                out.push(format!("the default import options ({caller:?})"));
            }
        }
        if manager.url_class_defaults.iter().any(|(_, s)| calls(s)) {
            out.push("a URL class's default import options".into());
        }
        for (name, slice) in &manager.favourites {
            if calls(slice) {
                out.push(format!("the favourite import options \"{name}\""));
            }
        }
    }
    for sub in &input.subscriptions {
        if calls(&sub.settings.import_options) {
            out.push(format!("subscription \"{}\"", sub.name));
        }
    }
    for folder in &input.import_folders {
        if calls(&folder.options) {
            out.push(format!("import folder \"{}\"", folder.name));
        }
    }
    for page in &input.downloader_pages {
        if page.queues.iter().any(|q| calls(&q.options)) {
            out.push(format!("downloader page \"{}\"", page.name));
        }
    }
    out
}

/// A duplicates page's filtering.
fn duplicates_page(
    d: &legacy::gui_sessions::LegacyDuplicatesPage,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> std::result::Result<hydrus_core::pages::DuplicatesPage, String> {
    use hydrus_core::duplicates::PairOrder;
    Ok(hydrus_core::pages::DuplicatesPage {
        search: duplicates_search(&d.search, scales)?,
        synchronised: d.synchronised,
        order: PairOrder::from_code(d.sort_type).unwrap_or(PairOrder::MaxFilesize),
        ascending: d.sort_ascending,
        group_mode: d.group_mode,
    })
}

/// A duplicates page that an import kept as stored, from before duplicates
/// pages were read (its page data, as `PageContent::Other` keeps it): its
/// filtering, if it can be read. (Rating predicates in its search are read
/// without the services' star counts.)
pub fn stored_duplicates_page(stored: &Json) -> Option<hydrus_core::pages::DuplicatesPage> {
    use hydrus_legacy::objects::gui_sessions::{PageContent, page_data};
    use hydrus_legacy::serialisable::{SerialisableObject, SerialisableType};
    let object = SerialisableObject::from_stored(
        SerialisableType::GUI_SESSION_PAGE_DATA,
        None,
        1,
        &stored.to_string(),
    )
    .ok()?;
    match page_data(&object).ok()?.page.content {
        PageContent::Duplicates(d) => duplicates_page(&d, &|_| None).ok(),
        _ => None,
    }
}

/// A potential-duplicates search (a rule's, a duplicates page's).
fn duplicates_search(
    search: &legacy::auto_resolution::PotentialsSearch,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> std::result::Result<hydrus_core::duplicates::DuplicatesSearch, String> {
    use hydrus_core::duplicates::{DuplicatesSearch, PairSearchKind, PixelDuplicates};
    Ok(DuplicatesSearch {
        search_1: file_search(&search.search_1, scales)?,
        search_2: file_search(&search.search_2, scales)?,
        kind: PairSearchKind::from_code(search.dupe_search_type)
            .ok_or_else(|| format!("unknown pair search type {}", search.dupe_search_type))?,
        pixel_duplicates: PixelDuplicates::from_code(search.pixel_dupes)
            .ok_or_else(|| format!("unknown pixel duplicates preference {}", search.pixel_dupes))?,
        max_hamming_distance: u32::try_from(search.max_hamming_distance)
            .map_err(|_| format!("a search distance of {}", search.max_hamming_distance))?,
    })
}

/// A stored duplicates auto-resolution rule in our model, or why it can't
/// be converted. `scales` gives each numerical rating service's scale (see
/// [`predicate_with_scales`]).
pub fn auto_resolution_rule(
    r: &legacy::auto_resolution::AutoResolutionRule,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> std::result::Result<crate::duplicates::auto::Rule, String> {
    use crate::duplicates::auto::{OperationMode, Rule, RuleAction};

    Ok(Rule {
        name: r.name.clone(),
        paused: r.paused,
        mode: match r.operation_mode {
            1 => OperationMode::SemiAutomatic,
            2 => OperationMode::FullyAutomatic,
            other => return Err(format!("unknown operation mode {other}")),
        },
        max_pending_pairs: r
            .max_pending_pairs
            .map(|n| u32::try_from(n.max(0)).unwrap_or(u32::MAX)),
        search: duplicates_search(&r.search, scales)?,
        comparators: r
            .comparators
            .iter()
            .map(|c| comparator(c, scales))
            .collect::<std::result::Result<_, _>>()?,
        action: u8::try_from(r.action)
            .ok()
            .and_then(DuplicateType::from_code)
            .and_then(RuleAction::from_duplicate_type)
            .ok_or_else(|| format!("unknown action {}", r.action))?,
        delete_a: r.delete_a,
        delete_b: r.delete_b,
        custom_merge: r
            .custom_merge_options
            .as_ref()
            .map(merge_options)
            .transpose()
            .map_err(|e| e.to_string())?,
    })
}

fn file_search(
    f: &legacy::FileSearchContext,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> std::result::Result<hydrus_core::search::context::FileSearchContext, String> {
    use hydrus_core::search::context::{FileSearchContext, LocationContext, TagContext};
    Ok(FileSearchContext {
        location: LocationContext::new(
            f.location_context.current.iter().cloned(),
            f.location_context.deleted.iter().cloned(),
        ),
        tags: TagContext {
            service: f.tag_context.service_key.clone(),
            include_current: f.tag_context.include_current_tags,
            include_pending: f.tag_context.include_pending_tags,
            display_service: f.tag_context.display_service_key.clone(),
        },
        predicates: predicates(&f.predicates, scales)?,
    })
}

fn predicates(
    stored: &[hydrus_legacy::serialisable::SerialisableObject],
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> std::result::Result<Vec<hydrus_core::search::predicate::Predicate>, String> {
    stored
        .iter()
        .map(|p| predicate_with_scales(p, scales).map_err(|e| e.to_string()))
        .collect()
}

fn comparator(
    c: &legacy::auto_resolution::Comparator,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
) -> std::result::Result<crate::duplicates::auto::Comparator, String> {
    use crate::duplicates::auto::{Comparator, LookingAt, OneFileTest, PairTest};
    use hydrus_core::search::comparable::Comparable;
    use legacy::auto_resolution::Comparator as L;

    let looking_at = |code: i64| match code {
        0 => Ok(LookingAt::A),
        1 => Ok(LookingAt::B),
        2 => Ok(LookingAt::Either),
        other => Err(format!("unknown file to look at {other}")),
    };
    Ok(match c {
        L::OneFileMetadata {
            looking_at: l,
            search,
        } => Comparator::OneFileMetadata {
            looking_at: looking_at(*l)?,
            predicates: predicates(&search.predicates, scales)?,
        },
        L::OneFileHardcoded {
            looking_at: l,
            test,
        } => Comparator::OneFileHardcoded {
            looking_at: looking_at(*l)?,
            test: match test {
                0 => OneFileTest::JpegIsProgressive,
                1 => OneFileTest::JpegIsNotProgressive,
                other => return Err(format!("unknown one-file test {other}")),
            },
        },
        L::RelativeFileInfo {
            property,
            test,
            multiplier,
            delta,
        } => Comparator::RelativeFileInfo {
            property: Comparable::from_predicate_type(*property)
                .ok_or_else(|| format!("cannot compare predicate type {property} between files"))?,
            test: *test,
            multiplier: *multiplier,
            delta: *delta,
        },
        L::RelativeHardcoded(code) => Comparator::Pair(
            PairTest::from_code(*code).ok_or_else(|| format!("unknown two-file test {code}"))?,
        ),
        L::VisualDuplicates(confidence) => Comparator::VisualDuplicates {
            confidence: u8::try_from(*confidence)
                .map_err(|_| format!("a visual duplicates confidence of {confidence}"))?,
        },
        L::Or(members) => Comparator::Or(
            members
                .iter()
                .map(|c| comparator(c, scales))
                .collect::<std::result::Result<_, _>>()?,
        ),
        L::And(members) => Comparator::And(
            members
                .iter()
                .map(|c| comparator(c, scales))
                .collect::<std::result::Result<_, _>>()?,
        ),
    })
}

/// A subscription's settings and queries (their histories are copied during
/// the import).
fn subscription_input(
    s: &legacy::subscriptions::LegacySubscription,
    warnings: &mut Vec<String>,
) -> SubscriptionInput {
    for what in &s.unconverted {
        warnings.push(format!(
            "Subscription \"{}\" was last saved by an old hydrus, and {what} could not be converted, so \
             the defaults stand in",
            s.name
        ));
    }
    let limit = |limit: Option<i64>| limit.map(|n| u64::try_from(n).unwrap_or(0));
    let settings = SubscriptionSettings {
        gug_key: s.gug_key.clone(),
        gug_name: s.gug_name.clone(),
        checker: s.checker.clone(),
        initial_file_limit: limit(s.initial_file_limit),
        periodic_file_limit: limit(s.periodic_file_limit),
        this_is_a_random_sample: s.this_is_a_random_sample,
        paused: s.paused,
        import_options: s.import_options.clone(),
        no_work_until: s.no_work_until,
        no_work_until_reason: s.no_work_until_reason.clone(),
        show_a_popup_while_working: s.show_a_popup_while_working,
        publish_files_to_popup_button: s.publish_files_to_popup_button,
        publish_files_to_page: s.publish_files_to_page,
        publish_label_override: s.publish_label_override.clone(),
        merge_query_publish_events: s.merge_query_publish_events,
    };
    let queries = s
        .queries
        .iter()
        .map(|q| {
            let state = QueryState {
                query_text: q.query_text.clone(),
                display_name: q.display_name.clone(),
                check_now: q.check_now,
                last_check_time: q.last_check_time,
                next_check_time: q.next_check_time,
                paused: q.paused,
                // (CHECKER_STATUS_DEAD; subscriptions never 404)
                dead: q.checker_status != 0,
                file_seed_compaction_number: q.file_seed_compaction_number,
                gallery_seed_compaction_number: q.gallery_seed_compaction_number,
                tag_import_options: q.tag_import_options.clone(),
            };
            (q.log_name.clone(), state)
        })
        .collect();
    SubscriptionInput {
        name: s.name.clone(),
        settings,
        queries,
    }
}

fn service_tags(tags: &[(String, Vec<String>)]) -> Vec<(String, BTreeSet<String>)> {
    tags.iter()
        .map(|(key, tags)| (key.clone(), tags.iter().cloned().collect()))
        .collect()
}

/// A query's saved file seed; `None` (with a warning) if it cannot be kept.
/// A URL saved before its comparison form was stored is normalised with
/// the URL classes, as the reference does when it loads it.
pub(super) fn file_seed(
    f: &legacy::subscriptions::LegacyFileSeed,
    url_classes: Option<&UrlClasses>,
    warnings: &mut Vec<String>,
) -> Option<FileSeed> {
    let seed_type = match f.seed_type {
        0 => SeedType::Path,
        1 => SeedType::Url,
        other => {
            warnings.push(format!(
                "A file import of unknown type {other} ({}) was dropped",
                f.data
            ));
            return None;
        }
    };
    let data_for_comparison = f.data_for_comparison.clone().unwrap_or_else(|| {
        url_classes
            .and_then(|r| r.normalise(&f.data, false).ok())
            .unwrap_or_else(|| f.data.clone())
    });
    Some(FileSeed {
        id: 0,
        queue_id: 0,
        seed_type,
        data: f.data.clone(),
        data_for_comparison,
        created: f.created,
        modified: f.modified,
        source_time: f.source_time,
        status: seed_status(f.status, &f.data, warnings),
        note: f.note.clone(),
        referral_url: f.referral_url.clone(),
        meta: FileSeedMeta {
            request_headers: f.request_headers.clone(),
            external_filterable_tags: f.external_filterable_tags.iter().cloned().collect(),
            external_additional_tags: service_tags(&f.external_additional_tags),
            primary_urls: f.primary_urls.iter().cloned().collect(),
            source_urls: f.source_urls.iter().cloned().collect(),
            tags: f.tags.iter().cloned().collect(),
            notes: f.notes.clone(),
            hashes: f.hashes.clone(),
            // (the reference keeps this only while the seed works)
            cloudflare_last_modified: None,
        },
    })
}

pub(super) fn gallery_seed(
    g: &legacy::subscriptions::LegacyGallerySeed,
    warnings: &mut Vec<String>,
) -> GallerySeed {
    GallerySeed {
        id: 0,
        queue_id: 0,
        url: g.url.clone(),
        can_generate_more_pages: g.can_generate_more_pages,
        created: g.created,
        modified: g.modified,
        status: seed_status(g.status, &g.url, warnings),
        note: g.note.clone(),
        referral_url: g.referral_url.clone(),
        meta: GallerySeedMeta {
            request_headers: g.request_headers.clone(),
            external_filterable_tags: g.external_filterable_tags.iter().cloned().collect(),
            external_additional_tags: service_tags(&g.external_additional_tags),
            run_token: String::new(),
            force_next_page_url_generation: false,
        },
    }
}

fn seed_status(code: i64, what: &str, warnings: &mut Vec<String>) -> SeedStatus {
    SeedStatus::from_code(code).unwrap_or_else(|| {
        warnings.push(format!(
            "An import of {what} had unknown status {code}; it will be tried again"
        ));
        SeedStatus::Unknown
    })
}

/// Each tag service's autocomplete search rules. Widget behavior is imported
/// separately so it does not gate API searches.
fn autocomplete_settings(manager: &legacy::TagDisplayManager) -> AutocompleteSettings {
    let services = manager
        .autocomplete_options
        .iter()
        .map(|o| {
            (
                o.service_key.to_hex(),
                AutocompleteRules {
                    search_namespaces_into_full_tags: o.search_namespaces_into_full_tags,
                    unnamespaced_search_gives_any_namespace_wildcards: o
                        .unnamespaced_search_gives_any_namespace_wildcards,
                    namespace_bare_fetch_all_allowed: o.namespace_bare_fetch_all_allowed,
                    namespace_fetch_all_allowed: o.namespace_fetch_all_allowed,
                    fetch_all_allowed: o.fetch_all_allowed,
                },
            )
        })
        .collect();
    AutocompleteSettings { services }
}

fn insert_setting<S: Setting>(input: &mut ImportInput, value: &S) -> Result<()> {
    input
        .settings
        .insert(S::KEY.to_owned(), serde_json::to_value(value)?);
    Ok(())
}

fn positive(value: i64, what: &str) -> Result<u32> {
    u32::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| StoreError::Invalid(format!("{what} {value} is out of range")))
}

fn service_kind(service: &LegacyService) -> Result<ServiceKind> {
    // The verbatim settings of services we don't run yet, so nothing is lost.
    let verbatim = || RepositoryConfig {
        legacy: Json::String(service.dictionary.to_tuple_string()),
    };
    Ok(match (&service.config, service.service_type) {
        (ServiceConfig::LikeRating(c), _) => ServiceKind::RatingLike(LikeRatingConfig {
            display: rating_display(&c.display),
            appearance: appearance(&c.appearance)?,
        }),
        (ServiceConfig::NumericalRating(c), _) => {
            ServiceKind::RatingNumerical(NumericalRatingConfig {
                display: rating_display(&c.display),
                appearance: appearance(&c.appearance)?,
                num_stars: c.num_stars,
                allow_zero: c.allow_zero,
                custom_pad: i32::try_from(c.custom_pad).map_err(|_| {
                    StoreError::Invalid(format!("custom pad {} is out of range", c.custom_pad))
                })?,
                show_fraction_beside_stars: c.show_fraction_beside_stars as u8,
            })
        }
        (ServiceConfig::IncDecRating(display), _) => {
            ServiceKind::RatingIncDec(rating_display(display))
        }
        (ServiceConfig::ClientApi(c), _) => ServiceKind::ClientApi(ServerConfig {
            port: c.port,
            allow_non_local_connections: c.allow_non_local_connections,
            support_cors: c.support_cors,
            log_requests: c.log_requests,
            use_normie_eris: c.use_normie_eris,
            use_https: c.use_https,
            external_scheme_override: c.external_scheme_override.clone(),
            external_host_override: c.external_host_override.clone(),
            external_port_override: c.external_port_override.and_then(|p| u16::try_from(p).ok()),
        }),
        (_, ServiceType::TagRepository) => ServiceKind::TagRepository(verbatim()),
        (_, ServiceType::FileRepository) => ServiceKind::FileRepository(verbatim()),
        (_, ServiceType::Ipfs) => ServiceKind::Ipfs(verbatim()),
        (_, service_type) => ServiceKind::Unsupported {
            service_type,
            config: verbatim().legacy,
        },
    })
}

fn rating_display(display: &legacy::RatingDisplay) -> RatingDisplay {
    use legacy::RatingState;
    let defaults = RatingColours::default();
    let colour = |state, default: PenBrush| {
        display
            .colours
            .get(&state)
            .map_or(default, |c: &legacy::RatingColours| PenBrush {
                pen: Rgb(c.border),
                brush: Rgb(c.fill),
            })
    };
    RatingDisplay {
        colours: RatingColours {
            like: colour(RatingState::Like, defaults.like),
            dislike: colour(RatingState::Dislike, defaults.dislike),
            null: colour(RatingState::Null, defaults.null),
            mixed: colour(RatingState::Mixed, defaults.mixed),
        },
        show_in_thumbnail: display.show_in_thumbnail,
        show_in_thumbnail_even_when_null: display.show_in_thumbnail_even_when_null,
    }
}

fn appearance(appearance: &legacy::StarAppearance) -> Result<StarAppearance> {
    // the reference draws the shape if there is one, else the SVG
    if let Some(shape) = appearance.effective_shape() {
        let code = u8::try_from(shape.code()).map_err(|_| {
            StoreError::Invalid(format!("star shape {} is out of range", shape.code()))
        })?;
        return Ok(StarAppearance::Shape(StarShape(code)));
    }
    Ok(appearance
        .rating_svg
        .clone()
        .map_or_else(StarAppearance::default, StarAppearance::Svg))
}

/// Custom headers and unexpired cookies.
/// The bandwidth rules (with the options that pace gallery pages) and each
/// network context's usage so far, so today's limits carry on.
fn bandwidth_input(
    db: &LegacyDb,
    options: Option<&legacy::ClientOptions>,
    input: &mut ImportInput,
) -> Result<()> {
    use crate::bandwidth::BandwidthSettings;
    use hydrus_core::bandwidth::{BandwidthType, Rule, Rules, Tracker};
    use hydrus_core::network::NetworkContext;

    let context = |c: &legacy::bandwidth::LegacyNetworkContext| NetworkContext {
        kind: c.kind,
        data: c.data.clone().unwrap_or_default(),
    };
    let mut settings = BandwidthSettings::default();
    match db.bandwidth_manager() {
        Ok(Some(manager)) => {
            let mut rules = Vec::new();
            for (c, legacy_rules) in &manager.rules {
                let mut converted = Rules::default();
                for &(kind, span, max) in legacy_rules {
                    let Some(kind) = BandwidthType::from_code(kind) else {
                        input.warnings.push(format!(
                            "A bandwidth rule of unknown type {kind} on {} was dropped",
                            context(c).to_human_string()
                        ));
                        continue;
                    };
                    converted.add(Rule::new(
                        kind,
                        span.and_then(|s| u64::try_from(s).ok()),
                        u64::try_from(max).unwrap_or(0),
                    ));
                }
                rules.push((context(c), converted));
            }
            settings.rules = rules;
        }
        Ok(None) => {}
        Err(e) => input.warnings.push(format!(
            "The bandwidth rules could not be read, so the defaults apply: {e}"
        )),
    }
    if let Some(options) = options {
        for (key, field) in [
            (
                "gallery_page_wait_period_pages",
                &mut settings.gallery_page_wait_pages,
            ),
            (
                "gallery_page_wait_period_subscriptions",
                &mut settings.gallery_page_wait_subscriptions,
            ),
            ("watcher_page_wait_period", &mut settings.watcher_page_wait),
        ] {
            if let Some(&n) = options.integers.get(key) {
                *field = n;
            }
        }
        if let Some(&b) = options
            .booleans
            .get("override_bandwidth_on_file_urls_from_post_urls")
        {
            settings.override_on_file_urls_from_posts = b;
        }
    }
    insert_setting(input, &settings)?;

    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    match db.bandwidth_trackers() {
        Ok(trackers) => {
            for (name, decoded) in trackers {
                match decoded {
                    Ok(t) => {
                        let c = context(&t.context);
                        // (downloader and watcher pages' usage isn't kept)
                        if c.is_ephemeral() {
                            continue;
                        }
                        let counters = t.counters.map(|v| {
                            v.into_iter()
                                .map(|(k, n)| (k, u64::try_from(n).unwrap_or(0)))
                                .collect()
                        });
                        input
                            .bandwidth_usage
                            .push((c, Tracker::from_counters(counters, now)));
                    }
                    Err(e) => input.warnings.push(format!(
                        "Bandwidth usage {name:?} could not be read, so it starts afresh: {e}"
                    )),
                }
            }
        }
        Err(e) => input.warnings.push(format!(
            "Bandwidth usage could not be read, so it starts afresh: {e}"
        )),
    }
    Ok(())
}

fn network_input(db: &LegacyDb, input: &mut ImportInput) -> Result<()> {
    use crate::network::{Approval, Cookie, CustomHeader, NetworkContext};

    let context = |kind: i64, data: &Option<String>| NetworkContext {
        kind,
        data: data.clone().unwrap_or_default(),
    };
    match db.custom_headers() {
        Ok(Some(headers)) => {
            input.custom_headers = Some(
                headers
                    .iter()
                    .map(|h| {
                        let approval = Approval::from_code(h.approval).ok_or_else(|| {
                            StoreError::Invalid(format!("unknown header approval {}", h.approval))
                        })?;
                        Ok((
                            context(h.context_type, &h.context_data),
                            CustomHeader {
                                name: h.name.clone(),
                                value: h.value.clone(),
                                approval,
                                reason: h.reason.clone(),
                            },
                        ))
                    })
                    .collect::<Result<_>>()?,
            );
        }
        Ok(None) => {}
        Err(e) => input
            .warnings
            .push(format!("custom HTTP headers were not converted: {e}")),
    }
    // the reference drops expired cookies when it loads a session
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    match db.network_sessions() {
        Ok(sessions) => {
            for session in sessions {
                let key = context(session.context_type, &session.context_data);
                for c in session.cookies {
                    if c.expires.is_some_and(|e| e <= now) {
                        continue;
                    }
                    input.cookies.push((
                        key.clone(),
                        Cookie {
                            name: c.name,
                            value: c.value,
                            domain: c.domain,
                            path: c.path,
                            expires: c.expires,
                            secure: c.secure,
                            rest: c.rest,
                        },
                    ));
                }
            }
        }
        Err(e) => input.warnings.push(format!(
            "cookies were not converted (send them again from your browser): {e}"
        )),
    }
    Ok(())
}

/// The tag lists' colours: the old options' namespace colours (the
/// defaults if it has none), and the namespace OR predicates take theirs
/// from.
fn namespace_colours(
    legacy_options: &hydrus_legacy::objects::LegacyOptions,
    options: Option<&legacy::ClientOptions>,
) -> hydrus_core::tag_presentation::NamespaceColours {
    let mut out = hydrus_core::tag_presentation::NamespaceColours::default();
    let colours = legacy_options.namespace_colours();
    if !colours.is_empty() {
        out.colours = colours
            .into_iter()
            .map(|(namespace, rgb)| (namespace.map(str::to_owned), rgb))
            .collect();
    }
    if let Some(Some(namespace)) = options.and_then(|o| {
        o.noneable_strings
            .get("or_connector_custom_namespace_colour")
    }) {
        out.or_connector = Some(namespace.clone());
    }
    out
}

/// How tags are shown: `RenderTag`'s options, the namespace order and the
/// search page's and media viewer's tag sorts.
fn tag_presentation(
    options: &legacy::ClientOptions,
) -> hydrus_core::tag_presentation::TagPresentation {
    use hydrus_core::tag_presentation::TagPresentation;
    use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
    let mut out = TagPresentation::default();
    for (key, field) in [
        ("show_namespaces", &mut out.show_namespaces),
        ("show_number_namespaces", &mut out.show_number_namespaces),
        (
            "show_subtag_number_namespaces",
            &mut out.show_subtag_number_namespaces,
        ),
        (
            "replace_tag_underscores_with_spaces",
            &mut out.replace_underscores,
        ),
        ("replace_tag_emojis_with_boxes", &mut out.replace_emojis),
    ] {
        if let Some(&value) = options.booleans.get(key) {
            *field = value;
        }
    }
    if let Some(connector) = options.strings.get("namespace_connector") {
        out.namespace_connector.clone_from(connector);
    }
    if let Some(limit) = options
        .noneable_integers
        .get("number_of_unselected_medias_to_present_tags_for")
    {
        out.unselected_tag_limit = limit.map(|limit| limit.clamp(0, 10_000_000) as u32);
    }
    if let Some(connector) = options.strings.get("sibling_connector") {
        out.sibling_connector.clone_from(connector);
    }
    if let Some(namespaces) = options.string_lists.get("user_namespace_group_by_sort") {
        out.user_namespaces.clone_from(namespaces);
    }
    let sort = |legacy: &legacy::TagSort| -> Option<TagSort> {
        Some(TagSort {
            sort_type: match legacy.sort_type {
                0 => TagSortType::Tag,
                1 => TagSortType::Subtag,
                2 => TagSortType::Count,
                _ => return None,
            },
            ascending: legacy.sort_order == legacy::SortOrder::Ascending,
            group_by: match legacy.group_by {
                0 => TagGroupBy::Nothing,
                1 => TagGroupBy::NamespaceAz,
                2 => TagGroupBy::NamespaceUser,
                _ => return None,
            },
        })
    };
    // (`CC.TAG_PRESENTATION_SEARCH_PAGE` and `_MEDIA_VIEWER`)
    for (code, field) in [
        (0, &mut out.search_page_sort),
        (2, &mut out.media_viewer_sort),
    ] {
        if let Some(converted) = options.default_tag_sorts.get(&code).and_then(sort) {
            *field = converted;
        }
    }
    out
}

/// The tag display manager's filters for the single media and selection
/// list views (the others it may hold aren't used by the reference).
fn tag_display_filters(
    manager: &hydrus_legacy::objects::TagDisplayManager,
) -> crate::tag_display::TagDisplayFilters {
    use crate::tag_display::{TagDisplayFilters, TagView};
    let mut out = TagDisplayFilters::default();
    for (code, per_service) in &manager.tag_filters {
        let Some(view) = TagView::from_code(*code) else {
            continue;
        };
        for (key, filter) in per_service {
            let filter = tag_filter(filter);
            if !filter.allows_everything() {
                out.for_view_mut(view).insert(key.to_hex(), filter);
            }
        }
    }
    out
}

fn tag_filter(legacy: &legacy::TagFilter) -> TagFilter {
    let mut filter = TagFilter::new();
    for (slice, rule) in legacy.effective_rules() {
        let rule = match rule {
            TagRule::Allow => FilterRule::Whitelist,
            TagRule::Block => FilterRule::Blacklist,
        };
        filter.set_rule(slice, rule);
    }
    filter
}

/// The duplicate filter's metadata merge options. Relationships without
/// stored options merge nothing, as in the reference.
fn duplicate_merge_settings(
    stored: &std::collections::BTreeMap<i64, legacy::DuplicateMergeOptions>,
) -> Result<DuplicateMergeSettings> {
    let convert = |code: i64| -> Result<MergeOptions> {
        match stored.get(&code) {
            Some(o) => merge_options(o),
            None => Ok(MergeOptions::default()),
        }
    };
    Ok(DuplicateMergeSettings {
        better: convert(DuplicateType::Better.code().into())?,
        same_quality: convert(DuplicateType::SameQuality.code().into())?,
        alternate: convert(DuplicateType::Alternate.code().into())?,
    })
}

/// One set of duplicate metadata merge options.
pub(crate) fn merge_options(o: &legacy::DuplicateMergeOptions) -> Result<MergeOptions> {
    use crate::duplicates::merge::{ArchiveSync, MergeAction, RatingMerge, SyncAction, TagMerge};
    use hydrus_core::notes::{NoteConflict, NoteMerge};
    use legacy::MergeAction as Legacy;

    let action = |a: Legacy| match a {
        Legacy::Copy => Some(MergeAction::Copy),
        Legacy::Move => Some(MergeAction::Move),
        Legacy::TwoWay => Some(MergeAction::TwoWay),
        Legacy::None => None,
    };
    // the reference only copies URLs and modified dates; "move" does nothing
    let sync = |a: Legacy| match a {
        Legacy::Copy => Some(SyncAction::Copy),
        Legacy::TwoWay => Some(SyncAction::TwoWay),
        Legacy::Move | Legacy::None => None,
    };
    let notes = &o.note_import;
    let note_names = crate::duplicates::merge::NoteNames {
        whitelist: notes.name_whitelist.clone(),
        all_override: notes.all_name_override.clone(),
        overrides: notes.names_to_name_overrides.clone(),
    };
    let note_merge = if notes.get_notes && o.sync_notes != Legacy::None {
        let conflict = NoteConflict::from_code(notes.conflict_resolution).ok_or_else(|| {
            StoreError::Invalid(format!(
                "unknown note conflict resolution {}",
                notes.conflict_resolution
            ))
        })?;
        Some(NoteMerge {
            extend_existing: notes.extend_existing_note_if_possible,
            conflict,
        })
    } else {
        None
    };
    Ok(MergeOptions {
        tags: o
            .tag_services
            .iter()
            .filter_map(|(service, a, filter)| {
                action(*a).map(|action| TagMerge {
                    service: service.clone(),
                    action,
                    filter: tag_filter(filter),
                })
            })
            .collect(),
        ratings: o
            .rating_services
            .iter()
            .filter_map(|(service, a)| {
                action(*a).map(|action| RatingMerge {
                    service: service.clone(),
                    action,
                })
            })
            .collect(),
        notes: action(o.sync_notes),
        note_merge,
        note_names,
        archive: match o.sync_archive {
            legacy::ArchiveSync::None => ArchiveSync::Never,
            legacy::ArchiveSync::IfOneDoBoth => ArchiveSync::IfEither,
            legacy::ArchiveSync::DoBothRegardless => ArchiveSync::Always,
        },
        urls: sync(o.sync_urls),
        file_modified: sync(o.sync_file_modified_date),
    })
}

fn api_permissions(p: &legacy::ApiPermissions) -> ApiPermissionsRow {
    let codes: BTreeSet<i64> = p.basic_permissions.iter().map(|p| p.code()).collect();
    let filter = tag_filter(&p.search_tag_filter);
    ApiPermissionsRow {
        access_key: p.access_key.clone(),
        name: p.name.clone(),
        permits_everything: p.permits_everything,
        permissions: json!(codes),
        search_tag_filter: (filter != TagFilter::default())
            .then(|| serde_json::to_value(&filter).expect("a tag filter serialises")),
    }
}

/// The session the reference opens with (`default_gui_session`, "last
/// session" unless changed), as the tree of pages it shows. Downloader
/// pages' queues are carried over as queues (`input.downloader_pages`).
fn session(
    db: &LegacyDb,
    legacy_options: &hydrus_legacy::objects::LegacyOptions,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
    input: &mut ImportInput,
) {
    let name = legacy_options
        .get("default_gui_session")
        .and_then(hydrus_legacy::objects::YamlValue::as_str)
        .unwrap_or("last session");
    if name == "just a blank page" {
        return;
    }
    let (session, pages) = match db.gui_session(name) {
        Ok(Some(found)) => found,
        Ok(None) => return,
        Err(e) => {
            input.warnings.push(format!(
                "Session \"{name}\" could not be read, so its pages were not carried over (the \
                 original is kept): {e}"
            ));
            return;
        }
    };
    let hydrus_legacy::objects::gui_sessions::SessionNode::Notebook { pages: top, .. } =
        &session.top
    else {
        return;
    };
    let mut context = SessionContext {
        name,
        pages: &pages,
        scales,
        input,
        downloaders: true,
    };
    let pages = top.iter().filter_map(|node| context.page(node)).collect();
    input.session = Some(super::SessionInput {
        name: name.to_owned(),
        pages,
    });
}

/// The other saved sessions, to load later. As the owner chose, their
/// downloader pages are kept but make no queues: the reference only runs a
/// session's downloaders while it is open, and ours would run at once.
fn other_sessions(
    db: &LegacyDb,
    legacy_options: &hydrus_legacy::objects::LegacyOptions,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
    input: &mut ImportInput,
) {
    let opened = legacy_options
        .get("default_gui_session")
        .and_then(hydrus_legacy::objects::YamlValue::as_str)
        .unwrap_or("last session")
        .to_owned();
    let names = match db.gui_session_names() {
        Ok(names) => names,
        Err(e) => {
            input.warnings.push(format!(
                "The saved sessions could not be listed, so only the one hydrus opens with was carried over: {e}"
            ));
            return;
        }
    };
    for name in names.into_iter().filter(|n| *n != opened) {
        let (session, pages) = match db.gui_session(&name) {
            Ok(Some(found)) => found,
            Ok(None) => continue,
            Err(e) => {
                input.warnings.push(format!(
                    "Session \"{name}\" could not be read, so it was not carried over (the \
                     original is kept): {e}"
                ));
                continue;
            }
        };
        let hydrus_legacy::objects::gui_sessions::SessionNode::Notebook { pages: top, .. } =
            &session.top
        else {
            continue;
        };
        let mut context = SessionContext {
            name: &name,
            pages: &pages,
            scales,
            input,
            downloaders: false,
        };
        let pages = top.iter().filter_map(|node| context.page(node)).collect();
        // (our own "last session" is the one we open with)
        let kept_name = if name == crate::sessions::LAST_SESSION {
            format!("{name} (from hydrus)")
        } else {
            name.clone()
        };
        input.other_sessions.push(super::SessionInput {
            name: kept_name,
            pages,
        });
    }
}

struct SessionContext<'a> {
    name: &'a str,
    pages: &'a HashMap<Vec<u8>, hydrus_legacy::readers::StoredHashedObject>,
    scales: &'a dyn Fn(&ServiceKey) -> Option<StarScale>,
    input: &'a mut ImportInput,
    /// Whether downloader pages bring their queues (the session opened
    /// with) or are only kept (the others).
    downloaders: bool,
}

impl SessionContext<'_> {
    /// A page of the session, or a notebook with its pages; `None` (with a
    /// warning) for a page that can't be read.
    fn page(
        &mut self,
        node: &hydrus_legacy::objects::gui_sessions::SessionNode,
    ) -> Option<super::PageInput> {
        use hydrus_legacy::objects::gui_sessions::{PageContent, SessionNode, page_data};
        let name = self.name;
        let page_data_hash = match node {
            SessionNode::Notebook { name, pages } => {
                return Some(super::PageInput {
                    name: name.clone(),
                    content: super::PageInputContent::Pages(
                        pages.iter().filter_map(|node| self.page(node)).collect(),
                    ),
                    hashes: Vec::new(),
                });
            }
            SessionNode::Page { page_data_hash, .. } => page_data_hash,
        };
        let Some(stored) = self.pages.get(page_data_hash) else {
            self.input.warnings.push(format!(
                "A page of session \"{name}\" has lost its data, so it was not carried over"
            ));
            return None;
        };
        let data = match stored.parse().and_then(|object| page_data(&object)) {
            Ok(data) => data,
            Err(e) => {
                self.input.warnings.push(format!(
                    "A page of session \"{name}\" could not be read, so it was not carried over \
                     (the original is kept): {e}"
                ));
                return None;
            }
        };
        let page = data.page;
        let sort = page.sort.as_ref().map(page_sort);
        // (one collecting nothing is kept for whether unmatched files
        // would collect, which the page's collect control shows)
        let collect = page.collect.as_ref().map(page_collect);
        let hashes = data
            .hashes
            .iter()
            .filter_map(|h| Sha256::from_slice(h).ok())
            .collect();
        let kept = |sort| super::PageInputContent::Other {
            page_type: page.page_type,
            stored: serde_json::from_str(&stored.dump).ok(),
            sort,
        };
        let (state, highlighted) = match &page.content {
            PageContent::Gallery(m) => {
                let (state, highlighted) = gallery_page_state(m);
                (Some(state), highlighted)
            }
            PageContent::Watchers(m) => {
                let (state, highlighted) = watcher_page_state(m);
                (Some(state), highlighted)
            }
            _ => (None, None),
        };
        let queues = match page.content {
            PageContent::Query(q) => {
                let content = match file_search(&q.search, self.scales) {
                    Ok(search) => super::PageInputContent::Search {
                        search,
                        synchronised: q.synchronised,
                        sort,
                        lock: q.hash_locked.then_some(hydrus_core::pages::HashLock {
                            syncs_new: q.lock_syncs.syncs_new,
                            syncs_removes: q.lock_syncs.syncs_removes,
                        }),
                        collect,
                    },
                    Err(e) => {
                        self.input.warnings.push(format!(
                            "Page \"{}\" of session \"{name}\" searches for something hydrus-rs \
                             can't, so it is kept but not opened: {e}",
                            page.name
                        ));
                        kept(sort)
                    }
                };
                return Some(super::PageInput {
                    name: page.name,
                    content,
                    hashes,
                });
            }
            PageContent::Urls(u) => vec![super::PageQueueInput {
                options: u.import_options,
                files_paused: u.paused,
                gallery_paused: u.paused,
                created: None,
                state: super::PageQueueState::Urls,
                file_seeds: u.file_seeds,
                gallery_seeds: u.gallery_seeds,
            }],
            PageContent::Gallery(m) => m
                .gallery_imports
                .into_iter()
                .map(|g| super::PageQueueInput {
                    options: g.import_options,
                    files_paused: g.files_paused,
                    gallery_paused: g.gallery_paused,
                    created: Some(g.created),
                    state: super::PageQueueState::Gallery(hydrus_core::gallery::GallerySearch {
                        query: g.query,
                        source_name: g.source_name,
                        file_limit: g.file_limit.and_then(|n| u64::try_from(n).ok()),
                        num_new_urls_found: u64::try_from(g.num_new_urls_found).unwrap_or(0),
                        num_urls_found: u64::try_from(g.num_urls_found).unwrap_or(0),
                    }),
                    file_seeds: g.file_seeds,
                    gallery_seeds: g.gallery_seeds,
                })
                .collect(),
            PageContent::Watchers(m) => m
                .watchers
                .into_iter()
                // a watcher not yet given a thread has nothing to do
                .filter(|w| !w.url.is_empty())
                .map(|w| {
                    use hydrus_core::watchers::{CheckerStatus, WatcherState};
                    let state = WatcherState {
                        url: w.url,
                        subject: w.subject,
                        checker: w.checker,
                        last_check_time: w.last_check_time,
                        next_check_time: 0,
                        check_now: false,
                        checking_paused: w.checking_paused,
                        status: match w.checking_status {
                            1 => CheckerStatus::Dead,
                            2 => CheckerStatus::NotFound,
                            _ => CheckerStatus::Ok,
                        },
                        no_work_until: w.no_work_until,
                        no_work_until_reason: w.no_work_until_reason,
                        created: w.created,
                        external_filterable_tags: w.external_filterable_tags.into_iter().collect(),
                        external_additional_tags: w
                            .external_additional_tags
                            .into_iter()
                            .map(|(key, tags)| (key, tags.into_iter().collect()))
                            .collect(),
                    };
                    super::PageQueueInput {
                        options: w.import_options,
                        files_paused: w.files_paused,
                        gallery_paused: false,
                        created: Some(w.created),
                        state: super::PageQueueState::Watcher(state),
                        file_seeds: w.file_seeds,
                        gallery_seeds: w.gallery_seeds,
                    }
                })
                .collect(),
            PageContent::LocalImport(h) => {
                vec![super::PageQueueInput {
                    options: h.import_options,
                    files_paused: h.paused,
                    gallery_paused: false,
                    created: None,
                    state: super::PageQueueState::LocalImport(crate::queues::LocalImport {
                        delete_after_success: h.delete_after_success,
                        routers: h.metadata_routers,
                    }),
                    file_seeds: h.file_seeds,
                    gallery_seeds: Vec::new(),
                }]
            }
            PageContent::SimpleDownloader(d) => vec![super::PageQueueInput {
                options: d.import_options,
                files_paused: d.files_paused,
                gallery_paused: d.gallery_paused,
                created: None,
                state: super::PageQueueState::SimpleDownloader(crate::queues::SimpleDownloader {
                    formula_name: d.formula_name,
                    pending: d
                        .pending
                        .into_iter()
                        .map(|(url, formula)| crate::queues::SimpleJob { url, formula })
                        .collect(),
                }),
                file_seeds: d.file_seeds,
                gallery_seeds: d.gallery_seeds,
            }],
            PageContent::Duplicates(d) => {
                let content = match duplicates_page(&d, self.scales) {
                    Ok(duplicates) => super::PageInputContent::Duplicates { duplicates, sort },
                    Err(e) => {
                        self.input.warnings.push(format!(
                            "Duplicates page \"{}\" of session \"{name}\" searches for something \
                             hydrus-rs can't, so it is kept but not opened: {e}",
                            page.name
                        ));
                        kept(sort)
                    }
                };
                return Some(super::PageInput {
                    name: page.name,
                    content,
                    hashes,
                });
            }
            PageContent::Other => {
                return Some(super::PageInput {
                    name: page.name,
                    content: kept(sort),
                    hashes,
                });
            }
        };
        if !self.downloaders {
            return Some(super::PageInput {
                name: page.name,
                content: kept(sort),
                hashes,
            });
        }
        let kind = match page.page_type {
            hydrus_legacy::objects::gui_sessions::page_type::URLS => DownloaderKind::Urls,
            hydrus_legacy::objects::gui_sessions::page_type::GALLERY => DownloaderKind::Gallery,
            hydrus_legacy::objects::gui_sessions::page_type::IMPORT_FROM_DISK => {
                DownloaderKind::Local
            }
            hydrus_legacy::objects::gui_sessions::page_type::SIMPLE_DOWNLOADER => {
                DownloaderKind::Simple
            }
            _ => DownloaderKind::Watchers,
        };
        self.input
            .downloader_pages
            .push(super::DownloaderPageInput {
                name: page.name.clone(),
                queues,
                state,
                highlighted,
            });
        Some(super::PageInput {
            name: page.name,
            content: super::PageInputContent::Downloader {
                kind,
                index: self.input.downloader_pages.len() - 1,
                sort,
            },
            hashes,
        })
    }
}

/// A gallery page's own state (`MultipleGalleryImport`), and the search it
/// shows, by its place among the page's searches.
pub(crate) fn gallery_page_state(
    m: &hydrus_legacy::objects::gui_sessions::LegacyMultipleGalleryImport,
) -> (hydrus_core::pages::DownloaderPageState, Option<usize>) {
    let highlighted = m
        .highlighted
        .as_ref()
        .and_then(|key| m.gallery_imports.iter().position(|g| &g.key == key));
    let state = hydrus_core::pages::DownloaderPageState {
        highlighted: None,
        options: m.import_options.clone(),
        gallery: Some(hydrus_core::pages::GalleryPageState {
            gug_key: m.gug_key.clone(),
            gug_name: m.gug_name.clone(),
            file_limit: m.file_limit.and_then(|n| u64::try_from(n).ok()),
            start_files_paused: m.start_file_queues_paused,
            start_gallery_paused: m.start_gallery_queues_paused,
            no_new_dupes: m.do_not_allow_new_dupes,
            merge_pends: m.merge_simultaneous_pends_to_one_importer,
        }),
        checker: None,
    };
    (state, highlighted)
}

/// A watcher page's own state (`MultipleWatcherImport`), and the watcher it
/// shows, by its place among the page's watchers that come across (those
/// with a thread).
pub(crate) fn watcher_page_state(
    m: &hydrus_legacy::objects::gui_sessions::LegacyMultipleWatcherImport,
) -> (hydrus_core::pages::DownloaderPageState, Option<usize>) {
    let highlighted = m
        .highlighted
        .as_ref()
        .filter(|url| !url.is_empty())
        .and_then(|url| {
            m.watchers
                .iter()
                .filter(|w| !w.url.is_empty())
                .position(|w| &w.url == url)
        });
    let state = hydrus_core::pages::DownloaderPageState {
        highlighted: None,
        options: m.import_options.clone(),
        gallery: None,
        checker: Some(m.checker.clone()),
    };
    (state, highlighted)
}

#[cfg(test)]
pub(super) fn file_search_for_tests(
    f: &legacy::FileSearchContext,
) -> hydrus_core::search::context::FileSearchContext {
    file_search(f, &|_| None).unwrap()
}

/// A legacy page collect in our page model.
fn page_collect(collect: &legacy::MediaCollect) -> hydrus_core::pages::PageCollect {
    hydrus_core::pages::PageCollect {
        namespaces: collect.namespaces.clone(),
        ratings: collect.rating_service_keys.clone(),
        collect_unmatched: collect.collect_unmatched,
        tag_context: hydrus_core::search::context::TagContext {
            service: collect.tag_context.service_key.clone(),
            include_current: collect.tag_context.include_current_tags,
            include_pending: collect.tag_context.include_pending_tags,
            display_service: collect.tag_context.display_service_key.clone(),
        },
    }
}

/// A legacy page sort in our page model.
fn page_sort(sort: &legacy::MediaSort) -> PageSort {
    use legacy::MediaSortType;
    PageSort {
        by: match &sort.sort_type {
            MediaSortType::System(code) => PageSortBy::System(*code),
            MediaSortType::Namespaces {
                namespaces,
                tag_display_type,
            } => PageSortBy::Namespaces {
                namespaces: namespaces.clone(),
                tag_display_type: *tag_display_type,
            },
            MediaSortType::Rating(key) => PageSortBy::Rating(key.clone()),
        },
        ascending: sort.sort_order == legacy::SortOrder::Ascending,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use hydrus_legacy::serialisable::SerialisableObject;

    use super::*;

    /// The lock password comes across as hydrus stored it (the sha256 of
    /// "hunter2", in the old options' YAML).
    #[test]
    fn the_lock_password_converts() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = |source: &std::path::Path| {
            let input = decode_input(&LegacyDb::open(source).unwrap()).unwrap();
            serde_json::from_value::<hydrus_core::lock::LockPassword>(
                input.settings["lock_password"].clone(),
            )
            .unwrap()
        };
        assert!(!decoded(source.path()).is_set());
        let conn = rusqlite::Connection::open(source.path().join("client.db")).unwrap();
        let yaml: String = conn
            .query_row("SELECT options FROM options", [], |r| r.get(0))
            .unwrap();
        let yaml = yaml.replace(
            "password: null\n",
            "password: !!binary |\n  9S+9MrKzuG/4jvbEkGKChfSCrxXdyylUH5S89Saj9sc=\n",
        );
        conn.execute("UPDATE options SET options = ?1", [&yaml])
            .unwrap();
        drop(conn);
        let lock = decoded(source.path());
        assert_eq!(
            lock.sha256.as_deref(),
            Some("f52fbd32b2b3b86ff88ef6c490628285f482af15ddcb29541f94bcf526a3f6c7")
        );
        assert!(lock.accepts("hunter2") && !lock.accepts("hunter"));
    }

    /// The fixture's client options (`ClientOptions`), with text in their
    /// stored form replaced.
    fn edit_client_options(source: &std::path::Path, edits: &[(&str, &str)]) {
        let conn = rusqlite::Connection::open(source.join("client.db")).unwrap();
        let dump: Vec<u8> = conn
            .query_row(
                "SELECT dump FROM json_dumps WHERE dump_type = 22",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let mut dump = String::from_utf8(dump).unwrap();
        for (from, to) in edits {
            assert!(dump.contains(from), "{from}");
            dump = dump.replace(from, to);
        }
        conn.execute(
            "UPDATE json_dumps SET dump = ?1 WHERE dump_type = 22",
            [dump.into_bytes()],
        )
        .unwrap();
    }

    #[test]
    fn subscription_file_failure_threshold_imports_none_and_number() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<crate::network::NetworkSettings>(
                input.settings["network"].clone(),
            )
            .unwrap()
            .subscription_file_error_cancel_threshold
        };
        assert_eq!(decoded(), Some(5));
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "subscription_file_error_cancel_threshold"], [0, 5]]"#,
                r#"[[0, "subscription_file_error_cancel_threshold"], [0, null]]"#,
            )],
        );
        assert_eq!(decoded(), None);
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "subscription_file_error_cancel_threshold"], [0, null]]"#,
                r#"[[0, "subscription_file_error_cancel_threshold"], [0, 19]]"#,
            )],
        );
        assert_eq!(decoded(), Some(19));
    }

    #[test]
    fn network_boot_preference_converts_without_changing_live_pause() {
        use crate::settings::{NetworkBootPause, Pauses};
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        assert_eq!(
            decoded().settings["boot_with_network_traffic_paused"],
            false
        );
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "boot_with_network_traffic_paused"], [0, false]]"#,
                r#"[[0, "boot_with_network_traffic_paused"], [0, true]]"#,
            )],
        );
        let input = decoded();
        let boot: NetworkBootPause =
            serde_json::from_value(input.settings["boot_with_network_traffic_paused"].clone())
                .unwrap();
        assert!(boot.0);
        let pauses: Pauses = serde_json::from_value(input.settings["pauses"].clone()).unwrap();
        let before: Pauses = serde_json::from_value(
            decode_input(&LegacyDb::open(hydrus_testkit::legacy_fixture("basic").path()).unwrap())
                .unwrap()
                .settings["pauses"]
                .clone(),
        )
        .unwrap();
        assert_eq!(pauses, before, "import does not apply a boot action");
    }

    #[test]
    fn tag_dialog_defaults_import_independently_of_autocomplete_defaults() {
        use crate::tag_editing::TagEditingSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<TagEditingSettings>(input.settings["tag_editing"].clone())
                .unwrap()
        };
        let before = decoded();
        assert!(!before.use_listbook);
        assert!(before.tag_list_show_parents);
        assert!(before.tag_list_expand_parents);
        assert!(before.tag_list_show_siblings);
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "use_listbook_for_tag_service_panels"], [0, false]]"#,
                    r#"[[0, "use_listbook_for_tag_service_panels"], [0, true]]"#,
                ),
                (
                    r#"[[0, "show_parent_decorators_on_storage_taglists"], [0, true]]"#,
                    r#"[[0, "show_parent_decorators_on_storage_taglists"], [0, false]]"#,
                ),
                (
                    r#"[[0, "expand_parents_on_storage_taglists"], [0, true]]"#,
                    r#"[[0, "expand_parents_on_storage_taglists"], [0, false]]"#,
                ),
                (
                    r#"[[0, "show_sibling_decorators_on_storage_taglists"], [0, true]]"#,
                    r#"[[0, "show_sibling_decorators_on_storage_taglists"], [0, false]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            TagEditingSettings {
                use_listbook: true,
                tag_list_show_parents: false,
                tag_list_expand_parents: false,
                tag_list_show_siblings: false,
                ..before
            }
        );
    }

    #[test]
    fn clipboard_monitor_switches_convert_independently() {
        use crate::settings::ClipboardUrls;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ClipboardUrls>(input.settings["clipboard_urls"].clone())
                .unwrap()
        };
        assert_eq!(decoded(), ClipboardUrls::default());
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "watch_clipboard_for_watcher_urls"], [0, false]]"#,
                r#"[[0, "watch_clipboard_for_watcher_urls"], [0, true]]"#,
            )],
        );
        assert_eq!(
            decoded(),
            ClipboardUrls {
                watchers: true,
                other_recognised: false
            }
        );
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "watch_clipboard_for_other_recognised_urls"], [0, false]]"#,
                r#"[[0, "watch_clipboard_for_other_recognised_urls"], [0, true]]"#,
            )],
        );
        assert_eq!(
            decoded(),
            ClipboardUrls {
                watchers: true,
                other_recognised: true
            }
        );
    }

    /// The thumbnail grid's border and margin come across.
    #[test]
    fn the_thumbnail_border_and_margin_convert() {
        use crate::settings::ThumbnailLayout;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = |source: &std::path::Path| {
            let input = decode_input(&LegacyDb::open(source).unwrap()).unwrap();
            serde_json::from_value::<ThumbnailLayout>(input.settings["thumbnail_layout"].clone())
                .unwrap()
        };
        // the fixture's are hydrus's defaults
        assert_eq!(
            decoded(source.path()),
            ThumbnailLayout {
                border: 1,
                margin: 2
            }
        );
        // as are its ratings over thumbnails
        let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        assert_eq!(
            serde_json::from_value::<hydrus_core::thumbnail::ThumbnailRatingSettings>(
                input.settings["thumbnail_ratings"].clone()
            )
            .unwrap(),
            hydrus_core::thumbnail::ThumbnailRatingSettings::default()
        );
        // and its recent predicates (none)
        assert_eq!(
            serde_json::from_value::<hydrus_core::search::recent::RecentPredicates>(
                input.settings["recent_predicates"].clone()
            )
            .unwrap(),
            hydrus_core::search::recent::RecentPredicates::default()
        );
        // and the user's
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "thumbnail_border"], [0, 1]]"#,
                    r#"[[0, "thumbnail_border"], [0, 0]]"#,
                ),
                (
                    r#"[[0, "thumbnail_margin"], [0, 2]]"#,
                    r#"[[0, "thumbnail_margin"], [0, 7]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(source.path()),
            ThumbnailLayout {
                border: 0,
                margin: 7
            }
        );
    }

    /// The tag summaries drawn over thumbnails come across.
    #[test]
    fn the_tag_summaries_convert() {
        use hydrus_core::tag_summary::TagSummaries;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = |source: &std::path::Path| {
            let input = decode_input(&LegacyDb::open(source).unwrap()).unwrap();
            serde_json::from_value::<TagSummaries>(input.settings["tag_summaries"].clone()).unwrap()
        };
        // the fixture's are hydrus's defaults (their examples in another
        // order)
        let sorted = |mut s: TagSummaries| {
            for g in [
                &mut s.thumbnail_top,
                &mut s.thumbnail_bottom_right,
                &mut s.media_viewer_top,
            ] {
                g.example_tags.sort();
            }
            s
        };
        assert_eq!(
            sorted(decoded(source.path())),
            sorted(TagSummaries::default())
        );
        // and the user's: the top one hidden, the bottom right one's
        // separator changed
        edit_client_options(
            source.path(),
            &[
                (
                    r#"["series:series", "title:title", "creator:creator"], true]]]], [[0, "thumbnail_bottom_right"]"#,
                    r#"["series:series", "title:title", "creator:creator"], false]]]], [[0, "thumbnail_bottom_right"]"#,
                ),
                (
                    r#"[["volume", "v", "-"], ["chapter", "c", "-"], ["page", "p", "-"]], "-", ["chapter:10""#,
                    r#"[["volume", "v", "-"], ["chapter", "c", "-"], ["page", "p", "-"]], " ", ["chapter:10""#,
                ),
            ],
        );
        let after = decoded(source.path());
        assert!(!after.thumbnail_top.show);
        assert_eq!(after.thumbnail_bottom_right.separator, " ");
        assert!(after.media_viewer_top.show);
    }

    /// New search pages' tag service, and the default local file domain,
    /// come across.
    #[test]
    fn the_search_defaults_convert() {
        use crate::settings::SearchDefaults;
        use hydrus_core::search::context::LocationContext;
        use hydrus_core::service::builtin_keys;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = |source: &std::path::Path| {
            let input = decode_input(&LegacyDb::open(source).unwrap()).unwrap();
            serde_json::from_value::<SearchDefaults>(input.settings["search_defaults"].clone())
                .unwrap()
        };
        // the fixture's are hydrus's defaults
        assert_eq!(decoded(source.path()), SearchDefaults::default());
        // and the user's: "my tags", and "my files" with the trash
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "default_tag_service_search_page"], [0, "616c6c206b6e6f776e2074616773"]]"#,
                    r#"[[0, "default_tag_service_search_page"], [0, "6c6f63616c2074616773"]]"#,
                ),
                (
                    r#"[[0, "default_local_location_context"], [2, [103, 1, [["6c6f63616c2066696c6573"], []]]]]"#,
                    r#"[[0, "default_local_location_context"], [2, [103, 1, [["6c6f63616c2066696c6573", "7472617368"], []]]]]"#,
                ),
            ],
        );
        let key = |k: &[u8]| ServiceKey::new(k.to_vec());
        assert_eq!(
            decoded(source.path()),
            SearchDefaults {
                tag_service: key(builtin_keys::MY_TAGS),
                local_location: LocationContext::new(
                    [key(builtin_keys::MY_FILES), key(builtin_keys::TRASH)],
                    []
                ),
            }
        );
    }

    #[test]
    fn file_search_defaults_convert_user_values() {
        use crate::settings::FileSearchSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<FileSearchSettings>(input.settings["file_search"].clone())
                .unwrap()
        };
        assert_eq!(decoded(), FileSearchSettings::default());
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "default_search_synchronised"], [0, true]]"#,
                    r#"[[0, "default_search_synchronised"], [0, false]]"#,
                ),
                (
                    r#"[[0, "show_system_everything"], [0, true]]"#,
                    r#"[[0, "show_system_everything"], [0, false]]"#,
                ),
                (
                    r#"[[0, "autocomplete_float_main_gui"], [0, true]]"#,
                    r#"[[0, "autocomplete_float_main_gui"], [0, false]]"#,
                ),
                (
                    r#"[[0, "active_search_predicates_height_num_chars"], [0, 6]]"#,
                    r#"[[0, "active_search_predicates_height_num_chars"], [0, 9]]"#,
                ),
                (
                    r#"[[0, "ac_read_list_height_num_chars"], [0, 22]]"#,
                    r#"[[0, "ac_read_list_height_num_chars"], [0, 24]]"#,
                ),
                (
                    r#"[[0, "forced_search_limit"], [0, null]]"#,
                    r#"[[0, "forced_search_limit"], [0, 3]]"#,
                ),
                (
                    r#"[[0, "refresh_search_page_on_system_limited_sort_changed"], [0, true]]"#,
                    r#"[[0, "refresh_search_page_on_system_limited_sort_changed"], [0, false]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            FileSearchSettings {
                search_immediately: false,
                show_system_everything: false,
                float_autocomplete: false,
                active_predicate_rows: 9,
                autocomplete_rows: 24,
                implicit_limit: Some(3),
                refresh_limited_sort: false,
            }
        );
    }

    #[test]
    fn viewer_canvas_options_convert_user_values() {
        use crate::settings::ViewerCanvasSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerCanvasSettings>(input.settings["viewer_canvas"].clone())
                .unwrap()
        };
        assert_eq!(decoded(), ViewerCanvasSettings::default());
        assert_eq!(
            serde_json::from_value::<ViewerCanvasSettings>(serde_json::json!({})).unwrap(),
            ViewerCanvasSettings::default()
        );
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "media_viewer_recenter_media_on_window_resize"], [0, true]]"#,
                    r#"[[0, "media_viewer_recenter_media_on_window_resize"], [0, false]]"#,
                ),
                (
                    r#"[[0, "draw_transparency_checkerboard_media_canvas"], [0, false]]"#,
                    r#"[[0, "draw_transparency_checkerboard_media_canvas"], [0, true]]"#,
                ),
                (
                    r#"[[0, "draw_transparency_checkerboard_as_greenscreen"], [0, false]]"#,
                    r#"[[0, "draw_transparency_checkerboard_as_greenscreen"], [0, true]]"#,
                ),
                (
                    r#"[[0, "animated_scanbar_height"], [0, 20]]"#,
                    r#"[[0, "animated_scanbar_height"], [0, 37]]"#,
                ),
                (
                    r#"[[0, "animated_scanbar_hide_height"], [0, 5]]"#,
                    r#"[[0, "animated_scanbar_hide_height"], [0, null]]"#,
                ),
                (
                    r#"[[0, "animated_scanbar_nub_width"], [0, 10]]"#,
                    r#"[[0, "animated_scanbar_nub_width"], [0, 19]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            ViewerCanvasSettings {
                recenter_on_resize: false,
                transparency_checkerboard: true,
                transparency_greenscreen: true,
                seek_height: 37,
                seek_hidden_height: None,
                seek_nub_width: 19
            }
        );
    }

    #[test]
    fn cursor_autohide_import_preserves_timeout_and_do_not_hide() {
        use crate::settings::ViewerCursorSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerCursorSettings>(input.settings["viewer_cursor"].clone())
                .unwrap()
        };
        assert_eq!(decoded(), ViewerCursorSettings::default());
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "media_viewer_cursor_autohide_time_ms"], [0, 700]]"#,
                r#"[[0, "media_viewer_cursor_autohide_time_ms"], [0, 1250]]"#,
            )],
        );
        assert_eq!(decoded().autohide_ms, Some(1250));
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "media_viewer_cursor_autohide_time_ms"], [0, 1250]]"#,
                r#"[[0, "media_viewer_cursor_autohide_time_ms"], [0, null]]"#,
            )],
        );
        assert_eq!(decoded().autohide_ms, None);
    }

    #[test]
    fn passive_background_options_import_each_draw_key() {
        use crate::settings::ViewerBackgroundSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerBackgroundSettings>(
                input.settings["viewer_background"].clone(),
            )
            .unwrap()
        };
        assert_eq!(decoded(), ViewerBackgroundSettings::default());
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "draw_tags_hover_in_media_viewer_background"], [0, true]]"#,
                    r#"[[0, "draw_tags_hover_in_media_viewer_background"], [0, false]]"#,
                ),
                (
                    r#"[[0, "draw_top_hover_in_media_viewer_background"], [0, true]]"#,
                    r#"[[0, "draw_top_hover_in_media_viewer_background"], [0, false]]"#,
                ),
                (
                    r#"[[0, "draw_top_right_hover_in_media_viewer_background"], [0, true]]"#,
                    r#"[[0, "draw_top_right_hover_in_media_viewer_background"], [0, false]]"#,
                ),
                (
                    r#"[[0, "draw_notes_hover_in_media_viewer_background"], [0, true]]"#,
                    r#"[[0, "draw_notes_hover_in_media_viewer_background"], [0, false]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            ViewerBackgroundSettings {
                tags: false,
                information: false,
                ratings: false,
                notes: false
            }
        );
    }

    #[test]
    fn viewer_closing_options_import_each_independent_preference() {
        use crate::settings::ViewerClosingSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerClosingSettings>(
                input.settings["viewer_closing"].clone(),
            )
            .unwrap()
        };
        assert_eq!(decoded(), ViewerClosingSettings::default());
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "focus_media_tab_on_viewer_close_if_possible"], [0, false]]"#,
                    r#"[[0, "focus_media_tab_on_viewer_close_if_possible"], [0, true]]"#,
                ),
                (
                    r#"[[0, "focus_media_thumb_on_viewer_close"], [0, true]]"#,
                    r#"[[0, "focus_media_thumb_on_viewer_close"], [0, false]]"#,
                ),
                (
                    r#"[[0, "activate_main_gui_on_focusing_viewer_close"], [0, false]]"#,
                    r#"[[0, "activate_main_gui_on_focusing_viewer_close"], [0, true]]"#,
                ),
                (
                    r#"[[0, "activate_main_gui_on_viewer_close"], [0, false]]"#,
                    r#"[[0, "activate_main_gui_on_viewer_close"], [0, true]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            ViewerClosingSettings {
                reselect_page: true,
                select_exit_media: false,
                activate_focusing: true,
                activate_always: true
            }
        );
    }

    #[test]
    fn viewer_focus_options_import_independent_mouseover_gates() {
        use crate::settings::ViewerFocusSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerFocusSettings>(input.settings["viewer_focus"].clone())
                .unwrap()
        };
        assert_eq!(decoded(), ViewerFocusSettings::default());
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "animated_scanbar_pop_in_requires_focus"], [0, true]]"#,
                    r#"[[0, "animated_scanbar_pop_in_requires_focus"], [0, false]]"#,
                ),
                (
                    r#"[[0, "hover_windows_need_window_focus_to_pop_in"], [0, true]]"#,
                    r#"[[0, "hover_windows_need_window_focus_to_pop_in"], [0, false]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            ViewerFocusSettings {
                seek_requires_focus: false,
                hovers_require_focus: false
            }
        );
    }

    #[test]
    fn viewer_pointer_options_import_both_drag_preferences() {
        use crate::settings::ViewerPointerSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerPointerSettings>(
                input.settings["viewer_pointer"].clone(),
            )
            .unwrap()
        };
        assert_eq!(
            decoded(),
            ViewerPointerSettings {
                disallow_duration_drag: false,
                hide_during_drag: true
            }
        );
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "disallow_media_drags_on_duration_media"], [0, false]]"#,
                    r#"[[0, "disallow_media_drags_on_duration_media"], [0, true]]"#,
                ),
                (
                    r#"[[0, "hide_canvas_drags"], [0, true]]"#,
                    r#"[[0, "hide_canvas_drags"], [0, false]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            ViewerPointerSettings {
                disallow_duration_drag: true,
                hide_during_drag: false
            }
        );
        let fresh: ViewerPointerSettings = serde_json::from_str("{}").unwrap();
        assert!(!fresh.disallow_duration_drag);
        assert_eq!(fresh.hide_during_drag, !cfg!(target_os = "macos"));
    }

    #[test]
    fn viewer_hover_options_migrate_disable_keys_as_enabled_controls() {
        use crate::settings::ViewerHoverSettings;
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = || {
            let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
            serde_json::from_value::<ViewerHoverSettings>(input.settings["viewer_hovers"].clone())
                .unwrap()
        };
        assert_eq!(decoded(), ViewerHoverSettings::default());
        edit_client_options(
            source.path(),
            &[
                (
                    r#"[[0, "disable_tags_hover_in_media_viewer"], [0, false]]"#,
                    r#"[[0, "disable_tags_hover_in_media_viewer"], [0, true]]"#,
                ),
                (
                    r#"[[0, "disable_top_right_hover_in_media_viewer"], [0, false]]"#,
                    r#"[[0, "disable_top_right_hover_in_media_viewer"], [0, true]]"#,
                ),
                (
                    r#"[[0, "disable_notes_hover_in_media_viewer"], [0, false]]"#,
                    r#"[[0, "disable_notes_hover_in_media_viewer"], [0, true]]"#,
                ),
                (
                    r#"[[0, "draw_bottom_right_index_in_media_viewer_background"], [0, true]]"#,
                    r#"[[0, "draw_bottom_right_index_in_media_viewer_background"], [0, false]]"#,
                ),
            ],
        );
        assert_eq!(
            decoded(),
            ViewerHoverSettings {
                tags: false,
                ratings: false,
                notes: false,
                index_background: false
            }
        );
    }

    /// Whether a sort chosen on a page becomes the default comes across.
    #[test]
    fn saving_the_page_sort_on_change_converts() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let decoded = |source: &std::path::Path| {
            let input = decode_input(&LegacyDb::open(source).unwrap()).unwrap();
            serde_json::from_value::<hydrus_core::pages::SortSettings>(
                input.settings["sorts"].clone(),
            )
            .unwrap()
            .save_page_sort_on_change
        };
        assert!(!decoded(source.path()));
        edit_client_options(
            source.path(),
            &[(
                r#"[[0, "save_page_sort_on_change"], [0, false]]"#,
                r#"[[0, "save_page_sort_on_change"], [0, true]]"#,
            )],
        );
        assert!(decoded(source.path()));
    }

    /// The tag lists' colours come across: hydrus's defaults, the user's,
    /// and the namespace OR predicates take theirs from.
    #[test]
    fn namespace_colours_convert() {
        use hydrus_core::tag_presentation::NamespaceColours;
        use hydrus_legacy::objects::LegacyOptions;
        let source = hydrus_testkit::legacy_fixture("basic");
        let db = LegacyDb::open(source.path()).unwrap();
        let mut options = db.client_options().unwrap().unwrap();
        // the fixture's are hydrus's defaults
        let sorted = |c: NamespaceColours| {
            let mut colours = c.colours;
            colours.sort();
            colours
        };
        assert_eq!(
            sorted(namespace_colours(
                &db.legacy_options().unwrap(),
                Some(&options)
            )),
            sorted(NamespaceColours::default())
        );
        // the user's, and the OR connector's
        let custom = LegacyOptions::parse(Some(
            "namespace_colours:\n  null: !!python/tuple\n  - 1\n  - 2\n  - 3\n  ? ''\n  : !!python/tuple\n  - 4\n  - 5\n  - 6\n  character: !!python/tuple\n  - 7\n  - 8\n  - 9\n",
        ))
        .unwrap();
        options.noneable_strings.insert(
            "or_connector_custom_namespace_colour".into(),
            Some("character".into()),
        );
        let converted = namespace_colours(&custom, Some(&options));
        assert_eq!(
            sorted(converted.clone()),
            vec![
                (None, [1, 2, 3]),
                (Some(String::new()), [4, 5, 6]),
                (Some("character".into()), [7, 8, 9]),
            ]
        );
        assert_eq!(converted.or_connector.as_deref(), Some("character"));
    }

    /// The user's tag presentation options come across, with the search
    /// page's and media viewer's tag sorts.
    #[test]
    fn tag_presentation_converts() {
        use hydrus_core::tag_sort::{TagGroupBy, TagSort, TagSortType};
        let source = hydrus_testkit::legacy_fixture("basic");
        let db = LegacyDb::open(source.path()).unwrap();
        let mut options = db.client_options().unwrap().unwrap();
        assert_eq!(
            tag_presentation(&options),
            hydrus_core::tag_presentation::TagPresentation::default()
        );
        options.booleans.insert("show_namespaces".into(), false);
        options
            .booleans
            .insert("replace_tag_underscores_with_spaces".into(), true);
        options
            .strings
            .insert("namespace_connector".into(), " - ".into());
        options
            .strings
            .insert("sibling_connector".into(), " ⇢ ".into());
        options.string_lists.insert(
            "user_namespace_group_by_sort".into(),
            vec!["series".into(), ":".into(), String::new()],
        );
        options.default_tag_sorts.insert(
            0,
            legacy::TagSort {
                sort_type: 2,
                sort_order: legacy::SortOrder::Descending,
                use_siblings: true,
                group_by: 1,
            },
        );
        let converted = tag_presentation(&options);
        assert!(!converted.show_namespaces && converted.replace_underscores);
        assert_eq!(converted.namespace_connector, " - ");
        assert_eq!(converted.sibling_connector, " ⇢ ");
        assert_eq!(converted.user_namespaces, ["series", ":", ""]);
        assert_eq!(
            converted.search_page_sort,
            TagSort {
                sort_type: TagSortType::Count,
                ascending: false,
                group_by: TagGroupBy::NamespaceAz,
            }
        );
        assert_eq!(converted.media_viewer_sort, TagSort::DEFAULT);
    }

    #[test]
    fn unselected_tag_limit_import_preserves_zero_none_and_presentation() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let db = LegacyDb::open(source.path()).unwrap();
        let mut options = db.client_options().unwrap().unwrap();
        let before = tag_presentation(&options);
        assert_eq!(before.unselected_tag_limit, Some(4096));
        for limit in [None, Some(0), Some(1), Some(10_000_000)] {
            options.noneable_integers.insert(
                "number_of_unselected_medias_to_present_tags_for".into(),
                limit,
            );
            assert_eq!(
                tag_presentation(&options),
                hydrus_core::tag_presentation::TagPresentation {
                    unselected_tag_limit: limit.map(|limit| limit as u32),
                    ..before.clone()
                }
            );
        }
    }

    /// The single media and selection list filters come across by service;
    /// filters that hide nothing, and other views, are left out.
    #[test]
    fn tag_display_filters_convert() {
        use crate::tag_display::TagView;
        let key = |k: &[u8]| ServiceKey::new(k.to_vec());
        let hide = |slice: &str| legacy::TagFilter {
            rules: vec![(slice.into(), TagRule::Block)],
        };
        let manager = legacy::TagDisplayManager {
            tag_filters: vec![
                (
                    2,
                    vec![
                        (key(b"local tags"), hide("meta:")),
                        (key(b"downloader tags"), legacy::TagFilter { rules: vec![] }),
                    ],
                ),
                (3, vec![(key(b"all known tags"), hide("blue eyes"))]),
                (4, vec![(key(b"local tags"), hide("page:"))]),
            ],
            autocomplete_options: Vec::new(),
        };
        let filters = tag_display_filters(&manager);
        let rules = |view, k: &[u8]| -> Option<Vec<(String, FilterRule)>> {
            filters
                .for_view(view)
                .get(&key(k).to_hex())
                .map(|f| f.rules().map(|(s, r)| (s.to_owned(), r)).collect())
        };
        assert_eq!(
            rules(TagView::SingleMedia, b"local tags"),
            Some(vec![("meta:".to_owned(), FilterRule::Blacklist)])
        );
        assert_eq!(rules(TagView::SingleMedia, b"downloader tags"), None);
        assert_eq!(
            rules(TagView::SelectionList, b"all known tags"),
            Some(vec![("blue eyes".to_owned(), FilterRule::Blacklist)])
        );
        assert_eq!(filters.single_media.len() + filters.selection_list.len(), 2);
    }

    /// Every rule the reference stores (its suggestions, rules using every
    /// comparator, and the owner's own; `oracle/dump_auto_resolution.py`)
    /// converts.
    #[test]
    fn auto_resolution_rules_convert() {
        use crate::duplicates::auto::{Comparator, OperationMode, RuleAction};
        let fixture = hydrus_testkit::fixture_json("auto_resolution.json");
        for case in fixture["rules"].as_array().unwrap() {
            let stored = SerialisableObject::from_tuple_str(&case["stored"].to_string()).unwrap();
            let legacy =
                hydrus_legacy::objects::auto_resolution::AutoResolutionRule::from_object(&stored)
                    .unwrap();
            let rule = auto_resolution_rule(&legacy, &|_| None)
                .unwrap_or_else(|e| panic!("{}: {e}", legacy.name));
            let expected = &case["expected"];
            assert_eq!(rule.name, expected["name"].as_str().unwrap());
            assert_eq!(
                rule.mode,
                if expected["operation_mode"] == 2 {
                    OperationMode::FullyAutomatic
                } else {
                    OperationMode::SemiAutomatic
                }
            );
            assert_eq!(
                i64::from(rule.action.duplicate_type().code()),
                expected["action"].as_i64().unwrap()
            );
            assert_eq!(
                rule.comparators.len(),
                expected["comparators"].as_array().unwrap().len()
            );
            // a stored rule round-trips through its JSON
            let json = serde_json::to_string(&rule).unwrap();
            assert_eq!(
                serde_json::from_str::<crate::duplicates::auto::Rule>(&json).unwrap(),
                rule
            );
            if rule.name == "everything" {
                assert_eq!(rule.action, RuleAction::SameQuality);
                assert!(rule.custom_merge.is_some());
                assert!(
                    rule.comparators
                        .iter()
                        .any(|c| matches!(c, Comparator::Or(_)))
                );
            }
        }
    }

    #[test]
    fn downloader_pages_own_state_converts() {
        use hydrus_legacy::objects::gui_sessions::{
            multiple_gallery_import, multiple_watcher_import,
        };
        use hydrus_legacy::serialisable::SerialisableObject;

        let recorded = hydrus_testkit::fixture_json("gui_sessions.json");
        let object =
            |v: &serde_json::Value| SerialisableObject::from_tuple_str(&v.to_string()).unwrap();
        let mut highlighted_any = false;
        for case in recorded["multiple_gallery_imports"].as_array().unwrap() {
            let facts = &case["facts"];
            let m = multiple_gallery_import(&object(&case["stored"])).unwrap();
            let (state, highlighted) = gallery_page_state(&m);
            let gallery = state.gallery.as_ref().unwrap();
            assert_eq!(gallery.gug_key, facts["gug_key"]);
            assert_eq!(gallery.gug_name, facts["gug_name"]);
            assert_eq!(gallery.file_limit, facts["file_limit"].as_u64());
            assert_eq!(
                gallery.start_files_paused,
                facts["start_file_queues_paused"]
            );
            assert_eq!(
                gallery.start_gallery_paused,
                facts["start_gallery_queues_paused"]
            );
            assert_eq!(gallery.no_new_dupes, facts["do_not_allow_new_dupes"]);
            assert_eq!(
                gallery.merge_pends,
                facts["merge_simultaneous_pends_to_one_importer"]
            );
            assert_eq!(state.options, m.import_options);
            assert!(state.checker.is_none());
            // (the highlighted search, by its place among the page's)
            let expected = facts["gallery_imports"]
                .as_array()
                .unwrap()
                .iter()
                .position(|g| g["key"] == facts["highlighted"]);
            assert_eq!(highlighted, expected, "{facts}");
            highlighted_any |= highlighted.is_some();
        }
        for case in recorded["multiple_watcher_imports"].as_array().unwrap() {
            let facts = &case["facts"];
            let m = multiple_watcher_import(&object(&case["stored"])).unwrap();
            let (state, highlighted) = watcher_page_state(&m);
            assert_eq!(state.checker.as_ref(), Some(&m.checker));
            assert!(state.gallery.is_none());
            // (among the watchers that come across: those with a thread)
            let expected = facts["watchers"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|w| w["url"] != "")
                .position(|w| !facts["highlighted"].is_null() && w["url"] == facts["highlighted"]);
            assert_eq!(highlighted, expected, "{facts}");
            highlighted_any |= highlighted.is_some();
        }
        assert!(highlighted_any);
    }

    #[test]
    fn a_clients_simple_downloader_formulae_come_across() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        let decoded: crate::settings::SimpleDownloaderFormulae =
            serde_json::from_value(input.settings["simple_downloader_formulae"].clone()).unwrap();
        // (a new client's: the reference's two defaults, in its stored order)
        let mut decoded = decoded;
        decoded.formulae.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            decoded,
            crate::settings::SimpleDownloaderFormulae::default()
        );
    }

    #[test]
    fn a_new_clients_merge_options_are_our_defaults() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        let decoded: DuplicateMergeSettings =
            serde_json::from_value(input.settings["duplicate_merge"].clone()).unwrap();
        assert_eq!(decoded, DuplicateMergeSettings::default());
    }

    #[test]
    fn a_new_clients_import_options_and_downloaders_convert() {
        use hydrus_core::import_options::ImportOptionsManager;

        let source = hydrus_testkit::legacy_fixture("basic");
        let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        let decoded: ImportOptionsManager =
            serde_json::from_value(input.settings["import_options"].clone()).unwrap();
        assert_eq!(decoded, ImportOptionsManager::default());
        let downloaders: hydrus_parse::Downloaders =
            serde_json::from_value(input.settings["downloaders"].clone()).unwrap();
        assert!(downloaders.unconverted.is_empty());
    }

    #[test]
    fn a_new_clients_checker_timings_are_our_defaults() {
        let source = hydrus_testkit::legacy_fixture("basic");
        let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        let decoded: hydrus_core::subscriptions::CheckerDefaults =
            serde_json::from_value(input.settings["checker_defaults"].clone()).unwrap();
        assert_eq!(
            decoded,
            hydrus_core::subscriptions::CheckerDefaults::default()
        );
        let gallery: hydrus_core::subscriptions::GalleryDefaults =
            serde_json::from_value(input.settings["gallery_defaults"].clone()).unwrap();
        assert_eq!(gallery.file_limit, Some(2000));
    }

    #[test]
    fn a_new_clients_headers_are_our_defaults() {
        use crate::network::{NetworkContext, default_headers};

        let source = hydrus_testkit::legacy_fixture("basic");
        let input = decode_input(&LegacyDb::open(source.path()).unwrap()).unwrap();
        let expected: Vec<_> = default_headers()
            .into_iter()
            .map(|h| (NetworkContext::global(), h))
            .collect();
        assert_eq!(input.custom_headers, Some(expected));
        assert!(input.warnings.is_empty(), "{:?}", input.warnings);
    }

    /// Custom options as the reference serialised them
    /// (`oracle/dump_duplicate_merges.py`), against what the oracle's
    /// merges were checked with.
    #[test]
    fn custom_merge_options_convert() {
        let recorded = hydrus_testkit::fixture_json("duplicate_merges.json");
        let phase = recorded["phases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "custom")
            .unwrap();
        let stored: BTreeMap<i64, legacy::DuplicateMergeOptions> = phase["legacy_options"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(kind, tuple)| {
                let object = SerialisableObject::from_tuple_str(&tuple.to_string()).unwrap();
                (
                    kind.parse().unwrap(),
                    legacy::DuplicateMergeOptions::from_object(&object).unwrap(),
                )
            })
            .collect();
        let converted = duplicate_merge_settings(&stored).unwrap();
        let expected: DuplicateMergeSettings =
            serde_json::from_value(phase["settings"].clone()).unwrap();
        assert_eq!(converted, expected);
    }
}
