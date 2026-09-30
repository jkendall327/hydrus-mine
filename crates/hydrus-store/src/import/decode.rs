//! Decoding a reference install's serialised settings into [`ImportInput`].
//!
//! The SQL half of the importer copies rows; this half turns the reference's
//! serialised objects (service settings, Client API keys, client options)
//! into native settings, via `hydrus-legacy`'s typed decoders.

use std::collections::{BTreeSet, HashMap};

use serde_json::{Value as Json, json};

use hydrus_core::tag_filter::{FilterRule, TagFilter};
use hydrus_core::thumbnail::{ThumbnailScale, ThumbnailSettings};
use hydrus_core::{CanvasType, DuplicateType, ServiceKey, ServiceType};
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
    insert_setting(&mut input, &gallery)?;

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
        if let Some(&dpr) = options.integers.get("thumbnail_dpr_percent") {
            thumbnails.dpr_percent = positive(dpr, "thumbnail dpr percent")?;
        }
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
            let merge = duplicate_merge_settings(stored, &mut input.warnings)?;
            insert_setting(&mut input, &merge)?;
        }
        if let Some(&size) = options.integers.get("duplicate_filter_max_batch_size") {
            insert_setting(
                &mut input,
                &DuplicateFilterSettings {
                    max_batch_size: positive(size, "duplicate filter batch size")?,
                },
            )?;
        }
    }
    insert_setting(&mut input, &thumbnails)?;
    if let Some(manager) = db.tag_display_manager()? {
        insert_setting(&mut input, &autocomplete_settings(&manager))?;
    }
    network_input(db, &mut input)?;
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
    downloader_pages(db, &legacy_options, &mut input);
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
                    auto_resolution_rule(&r, &|key| scales.get(key).copied(), &mut input.warnings)
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
    Ok(input)
}

/// A stored duplicates auto-resolution rule in our model, or why it can't
/// be converted. `scales` gives each numerical rating service's scale (see
/// [`predicate_with_scales`]). Warnings about details that were dropped are
/// added to `warnings`.
pub fn auto_resolution_rule(
    r: &legacy::auto_resolution::AutoResolutionRule,
    scales: &dyn Fn(&ServiceKey) -> Option<StarScale>,
    warnings: &mut Vec<String>,
) -> std::result::Result<crate::duplicates::auto::Rule, String> {
    use crate::duplicates::auto::{OperationMode, Rule, RuleAction, RuleSearch};
    use crate::duplicates::{PairSearchKind, PixelDuplicates};

    let search = &r.search;
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
        search: RuleSearch {
            search_1: file_search(&search.search_1, scales)?,
            search_2: file_search(&search.search_2, scales)?,
            kind: match search.dupe_search_type {
                0 => PairSearchKind::OneFileMatchesOneSearch,
                1 => PairSearchKind::BothFilesMatchOneSearch,
                2 => PairSearchKind::BothFilesMatchDifferentSearches,
                other => return Err(format!("unknown pair search type {other}")),
            },
            pixel_duplicates: match search.pixel_dupes {
                0 => PixelDuplicates::Required,
                1 => PixelDuplicates::Allowed,
                2 => PixelDuplicates::Excluded,
                other => return Err(format!("unknown pixel duplicates preference {other}")),
            },
            max_hamming_distance: u32::try_from(search.max_hamming_distance)
                .map_err(|_| format!("a search distance of {}", search.max_hamming_distance))?,
        },
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
            .map(|o| merge_options(o, &format!("rule \"{}\"'s merge options", r.name), warnings))
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

/// Each tag service's autocomplete search rules. (The options that only
/// shape the GUI's autocomplete widget are not carried over.)
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
    warnings: &mut Vec<String>,
) -> Result<DuplicateMergeSettings> {
    let mut convert = |code: i64, label: &str| -> Result<MergeOptions> {
        match stored.get(&code) {
            Some(o) => merge_options(o, &format!("the {label} duplicate merge options"), warnings),
            None => Ok(MergeOptions::default()),
        }
    };
    Ok(DuplicateMergeSettings {
        better: convert(DuplicateType::Better.code().into(), "better")?,
        same_quality: convert(DuplicateType::SameQuality.code().into(), "same quality")?,
        alternate: convert(DuplicateType::Alternate.code().into(), "alternate")?,
    })
}

/// One set of duplicate metadata merge options. `what` names them in
/// warnings.
pub(crate) fn merge_options(
    o: &legacy::DuplicateMergeOptions,
    what: &str,
    warnings: &mut Vec<String>,
) -> Result<MergeOptions> {
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
    if !notes.name_whitelist.is_empty()
        || notes.all_name_override.is_some()
        || !notes.names_to_name_overrides.is_empty()
    {
        warnings.push(format!(
            "{what}' note name filters and renames were dropped (the reference's editor doesn't show them)"
        ));
    }
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

/// The downloader pages open in the session the reference opens with
/// (`default_gui_session`, "last session" unless changed).
fn downloader_pages(
    db: &LegacyDb,
    legacy_options: &hydrus_legacy::objects::LegacyOptions,
    input: &mut ImportInput,
) {
    use hydrus_legacy::objects::gui_sessions::{PageContent, page_data};
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
                "Session \"{name}\" could not be read, so its downloader pages were not carried \
                 over (the original is kept): {e}"
            ));
            return;
        }
    };
    for hash in session.top.page_data_hashes() {
        let Some(stored) = pages.get(hash) else {
            input.warnings.push(format!(
                "A page of session \"{name}\" has lost its data, so it was not carried over"
            ));
            continue;
        };
        let page = match stored.parse().and_then(|object| page_data(&object)) {
            Ok(data) => data.page,
            Err(e) => {
                input.warnings.push(format!(
                    "A page of session \"{name}\" could not be read, so its work was not carried \
                     over (the original is kept): {e}"
                ));
                continue;
            }
        };
        let queues = match page.content {
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
            PageContent::Other => {
                use hydrus_legacy::objects::gui_sessions::page_type;
                let what = match page.page_type {
                    page_type::SIMPLE_DOWNLOADER => "a simple downloader page",
                    page_type::IMPORT_FROM_DISK => "an import from disk",
                    _ => continue,
                };
                input.warnings.push(format!(
                    "Page \"{}\" of session \"{name}\" is {what}, which hydrus-rs doesn't run yet, \
                     so its unfinished work was not carried over (the original is kept)",
                    page.name
                ));
                continue;
            }
        };
        if !queues.is_empty() {
            input.downloader_pages.push(super::DownloaderPageInput {
                name: page.name,
                queues,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use hydrus_legacy::serialisable::SerialisableObject;

    use super::*;

    /// Every rule the reference stores (its suggestions and rules using
    /// every comparator; `oracle/dump_auto_resolution.py`) converts.
    #[test]
    fn auto_resolution_rules_convert() {
        use crate::duplicates::auto::{Comparator, OperationMode, RuleAction};
        let fixture = hydrus_testkit::fixture_json("auto_resolution.json");
        for case in fixture["rules"].as_array().unwrap() {
            let stored = SerialisableObject::from_tuple_str(&case["stored"].to_string()).unwrap();
            let legacy =
                hydrus_legacy::objects::auto_resolution::AutoResolutionRule::from_object(&stored)
                    .unwrap();
            let mut warnings = Vec::new();
            let rule = auto_resolution_rule(&legacy, &|_| None, &mut warnings)
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
        let mut warnings = Vec::new();
        let converted = duplicate_merge_settings(&stored, &mut warnings).unwrap();
        let expected: DuplicateMergeSettings =
            serde_json::from_value(phase["settings"].clone()).unwrap();
        assert_eq!(converted, expected);
        assert!(warnings.is_empty());
    }
}
