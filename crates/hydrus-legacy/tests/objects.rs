//! Serialised objects and their typed decoders, checked against what the
//! reference makes of the same data (`basic.expected.json`).

mod common;

use std::collections::BTreeMap;

use common::{basic, collect, expected};
use hydrus_core::{ServiceType, Sha256};
use hydrus_legacy::objects::{
    ApiPermissions, ClientApiServiceConfig, FileSearchContext, LocationContext, MediaCollect,
    MediaSort, MediaSortType, RatingDisplay, RepositoryMetadata, ServiceConfig, StarAppearance,
    TagAutocompleteOptions, TagContext, TagDisplayManager, TagFilter, TagSort, YamlValue,
};
use hydrus_legacy::paths::{FileLayout, PrefixKind, prefix, prefix_directories};
use hydrus_legacy::pyjson::{PyJson, write_python_float, write_python_string};
use hydrus_legacy::serialisable::{Body, SerialisableObject, SerialisableType};
use serde_json::{Map, Value, json};
use sha2::Digest;

fn py(value: &PyJson) -> Value {
    match value {
        PyJson::Null => Value::Null,
        PyJson::Bool(b) => json!(b),
        PyJson::Int(i) => json!(i),
        PyJson::BigInt(s) => Value::Number(s.parse().unwrap()),
        PyJson::Float(f) => json!(f),
        PyJson::Str(s) => json!(s),
        PyJson::List(items) => Value::Array(items.iter().map(py).collect()),
        PyJson::Object(entries) => {
            Value::Object(entries.iter().map(|(k, v)| (k.clone(), py(v))).collect())
        }
    }
}

fn tuple(object: &SerialisableObject) -> Value {
    py(&object.to_tuple())
}

fn rules(filter: &TagFilter) -> Value {
    Value::Array(
        filter
            .rules
            .iter()
            .map(|(slice, rule)| json!([slice, rule.code()]))
            .collect(),
    )
}

/// Check rules and the reference's verdicts on sample tags.
fn check_tag_filter(filter: &TagFilter, expected: &Value, context: &str) {
    assert_eq!(rules(filter), expected["rules"], "{context} rules");
    assert_eq!(
        filter.allows_everything(),
        expected["allows_everything"].as_bool().unwrap(),
        "{context} allows everything"
    );
    let compiled = filter.compile();
    for (tag, verdicts) in expected["verdicts"].as_object().unwrap() {
        assert_eq!(
            json!([compiled.allows(tag, false), compiled.allows(tag, true)]),
            *verdicts,
            "{context}: {tag}"
        );
    }
}

fn colours_json(display: &RatingDisplay) -> Value {
    Value::Array(
        display
            .colours
            .iter()
            .map(|(state, c)| json!([state.code(), [c.border, c.fill]]))
            .collect(),
    )
}

fn display_settings(display: &RatingDisplay, out: &mut Map<String, Value>) {
    out.insert("colours".into(), colours_json(display));
    out.insert("show_in_thumbnail".into(), json!(display.show_in_thumbnail));
    out.insert(
        "show_in_thumbnail_even_when_null".into(),
        json!(display.show_in_thumbnail_even_when_null),
    );
}

fn appearance_settings(appearance: &StarAppearance, out: &mut Map<String, Value>) {
    out.insert(
        "shape".into(),
        json!(
            appearance
                .shape
                .map(hydrus_legacy::objects::StarShape::code)
        ),
    );
    out.insert("rating_svg".into(), json!(appearance.rating_svg));
}

fn client_api_settings(config: &ClientApiServiceConfig, out: &mut Map<String, Value>) {
    out.insert("port".into(), json!(config.port));
    out.insert(
        "allow_non_local_connections".into(),
        json!(config.allow_non_local_connections),
    );
    out.insert("support_cors".into(), json!(config.support_cors));
    out.insert("log_requests".into(), json!(config.log_requests));
    out.insert("use_normie_eris".into(), json!(config.use_normie_eris));
    out.insert("use_https".into(), json!(config.use_https));
    out.insert(
        "external_scheme_override".into(),
        json!(config.external_scheme_override),
    );
    out.insert(
        "external_host_override".into(),
        json!(config.external_host_override),
    );
    out.insert(
        "external_port_override".into(),
        json!(config.external_port_override),
    );
    out.insert("bandwidth_tracker".into(), tuple(&config.bandwidth_tracker));
    out.insert("bandwidth_rules".into(), tuple(&config.bandwidth_rules));
}

fn metadata_json(metadata: &RepositoryMetadata) -> Value {
    let updates: Vec<Value> = metadata
        .updates
        .iter()
        .map(|u| {
            let hashes: Vec<String> = u.update_hashes.iter().map(Sha256::to_hex).collect();
            json!([u.index, hashes, u.begin, u.end])
        })
        .collect();
    json!([37, 1, [updates, metadata.next_update_due]])
}

/// The settings dictionary as the reference's `_GetSerialisableDictionary`
/// would write it after loading.
fn settings_json(config: &ServiceConfig) -> Value {
    let mut out = Map::new();
    let remote = |remote: &hydrus_legacy::objects::RemoteConfig, out: &mut Map<String, Value>| {
        let c = &remote.credentials;
        out.insert(
            "credentials".into(),
            json!([
                35,
                1,
                [c.host, c.port, c.access_key.as_ref().map(hex::encode)]
            ]),
        );
        out.insert(
            "no_requests_reason".into(),
            json!(remote.no_requests_reason),
        );
        out.insert("no_requests_until".into(), json!(remote.no_requests_until));
    };
    match config {
        ServiceConfig::Plain => {}
        ServiceConfig::LikeRating(c) => {
            display_settings(&c.display, &mut out);
            appearance_settings(&c.appearance, &mut out);
        }
        ServiceConfig::NumericalRating(c) => {
            display_settings(&c.display, &mut out);
            appearance_settings(&c.appearance, &mut out);
            out.insert("num_stars".into(), json!(c.num_stars));
            out.insert("allow_zero".into(), json!(c.allow_zero));
            out.insert("custom_pad".into(), json!(c.custom_pad));
            out.insert(
                "show_fraction_beside_stars".into(),
                json!(c.show_fraction_beside_stars as i64),
            );
        }
        ServiceConfig::IncDecRating(display) => display_settings(display, &mut out),
        ServiceConfig::ClientApi(c) => client_api_settings(c, &mut out),
        ServiceConfig::Repository(c) => {
            remote(&c.restricted.remote, &mut out);
            out.insert(
                "next_account_sync".into(),
                json!(c.restricted.next_account_sync),
            );
            out.insert(
                "network_sync_paused".into(),
                json!(c.restricted.network_sync_paused),
            );
            out.insert(
                "service_options".into(),
                tuple(&c.restricted.service_options),
            );
            out.insert("metadata".into(), metadata_json(&c.metadata));
            out.insert(
                "do_a_full_metadata_resync".into(),
                json!(c.do_a_full_metadata_resync),
            );
            out.insert(
                "update_downloading_paused".into(),
                json!(c.update_downloading_paused),
            );
            out.insert(
                "update_processing_paused".into(),
                json!(c.update_processing_paused),
            );
            let paused: Vec<Value> = c
                .update_processing_content_types_paused
                .iter()
                .map(|(ct, p)| json!([ct.code(), p]))
                .collect();
            out.insert(
                "update_processing_content_types_paused".into(),
                json!(paused),
            );
        }
        ServiceConfig::Restricted(c) => {
            remote(&c.remote, &mut out);
            out.insert("next_account_sync".into(), json!(c.next_account_sync));
            out.insert("network_sync_paused".into(), json!(c.network_sync_paused));
            out.insert("service_options".into(), tuple(&c.service_options));
        }
        ServiceConfig::Ipfs(c) => {
            remote(&c.remote, &mut out);
            out.insert("multihash_prefix".into(), json!(c.multihash_prefix));
        }
    }
    Value::Object(out)
}

/// Colours are a dict in the reference; compare them sorted by state.
fn normalise_settings(mut settings: Value) -> Value {
    if let Some(colours) = settings.get_mut("colours").and_then(Value::as_array_mut) {
        colours.sort_by_key(|c| c[0].as_i64());
    }
    settings
}

#[test]
fn services_match_the_reference() {
    let fixture = basic();
    let services = fixture.db.services().unwrap();
    let expected = fixture.expected["services"].as_array().unwrap();
    assert_eq!(services.len(), expected.len());
    for (service, expected) in services.iter().zip(expected) {
        assert_eq!(
            u64::from(service.id.get()),
            expected["service_id"].as_u64().unwrap()
        );
        assert_eq!(service.key.to_hex(), expected["key"].as_str().unwrap());
        assert_eq!(
            u64::from(service.service_type.code()),
            expected["type"].as_u64().unwrap()
        );
        assert_eq!(service.name, expected["name"].as_str().unwrap());
        assert_eq!(
            settings_json(&service.config),
            normalise_settings(expected["settings"].clone()),
            "{}",
            service.name
        );
    }
    // the fixture's custom services, by their fixed keys
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    for (name, key) in manifest["service_keys"].as_object().unwrap() {
        let key = hydrus_core::ServiceKey::from_hex(key.as_str().unwrap()).unwrap();
        let info = fixture.db.service_by_key(&key).unwrap();
        assert_eq!(info.name.replace(' ', "_"), *name);
    }
}

#[test]
fn missing_service_settings_take_reference_defaults() {
    let expected = expected();
    let empty = SerialisableObject::from_tuple_str("[21, 2, []]").unwrap();
    for (code, settings) in expected["default_service_settings"].as_object().unwrap() {
        let service_type = ServiceType::from_code(code.parse().unwrap()).unwrap();
        let config = ServiceConfig::decode(service_type, &empty).unwrap();
        assert_eq!(
            settings_json(&config),
            normalise_settings(settings.clone()),
            "{service_type}"
        );
    }
}

#[test]
fn client_api_permissions_match_the_reference() {
    let fixture = basic();
    let manager = fixture.db.client_api_manager().unwrap().unwrap();
    let expected = fixture.expected["client_api_permissions"]
        .as_array()
        .unwrap();
    assert_eq!(manager.permissions.len(), expected.len());
    for (permissions, expected) in manager.permissions.iter().zip(expected) {
        assert_eq!(permissions.name, expected["name"].as_str().unwrap());
        assert_eq!(
            hex::encode(&permissions.access_key),
            expected["access_key"].as_str().unwrap()
        );
        assert_eq!(
            permissions.permits_everything,
            expected["permits_everything"].as_bool().unwrap()
        );
        let basic: Vec<i64> = permissions
            .basic_permissions
            .iter()
            .map(|p| p.code())
            .collect();
        assert_eq!(json!(basic), expected["basic_permissions"]);
        for (code, has) in expected["has_permission"].as_object().unwrap() {
            let permission =
                hydrus_legacy::objects::ApiPermission::from_code(code.parse().unwrap()).unwrap();
            assert_eq!(
                permissions.has_permission(permission),
                has.as_bool().unwrap(),
                "{} {permission:?}",
                permissions.name
            );
        }
        check_tag_filter(
            &permissions.search_tag_filter,
            &expected["search_tag_filter"],
            &permissions.name,
        );
    }
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let keys: Vec<String> = manager
        .permissions
        .iter()
        .map(|p| hex::encode(&p.access_key))
        .collect();
    for key in manifest["access_keys"].as_object().unwrap().values() {
        assert!(keys.contains(&key.as_str().unwrap().to_owned()));
    }
}

fn tag_context_json(context: &TagContext) -> Value {
    json!({
        "service_key": context.service_key.to_hex(),
        "include_current_tags": context.include_current_tags,
        "include_pending_tags": context.include_pending_tags,
        "display_service_key": context.display_service_key.to_hex(),
    })
}

fn location_context_json(context: &LocationContext) -> Value {
    let mut current: Vec<String> = context
        .current
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    let mut deleted: Vec<String> = context
        .deleted
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    current.sort();
    deleted.sort();
    json!({ "current": current, "deleted": deleted })
}

fn media_sort_json(sort: &MediaSort) -> Value {
    let (metatype, data) = match &sort.sort_type {
        MediaSortType::System(code) => ("system", json!(code)),
        MediaSortType::Namespaces {
            namespaces,
            tag_display_type,
        } => ("namespaces", json!([namespaces, tag_display_type])),
        MediaSortType::Rating(key) => ("rating", json!(key.to_hex())),
    };
    json!({
        "metatype": metatype,
        "data": data,
        "order": sort.sort_order.code(),
        "tag_context": tag_context_json(&sort.tag_context),
    })
}

fn media_collect_json(collect: &MediaCollect) -> Value {
    let keys: Vec<String> = collect
        .rating_service_keys
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    json!({
        "namespaces": collect.namespaces,
        "rating_service_keys": keys,
        "collect_unmatched": collect.collect_unmatched,
        "tag_context": tag_context_json(&collect.tag_context),
    })
}

fn file_search_context_json(context: &FileSearchContext) -> Value {
    let predicates: Vec<Value> = context.predicates.iter().map(tuple).collect();
    json!({
        "location_context": location_context_json(&context.location_context),
        "tag_context": tag_context_json(&context.tag_context),
        "search_type": context.search_type as i64,
        "predicates": predicates,
        "search_complete": context.search_complete,
    })
}

fn autocomplete_json(options: &TagAutocompleteOptions) -> Value {
    json!({
        "service_key": options.service_key.to_hex(),
        "write_autocomplete_tag_domain": options.write_autocomplete_tag_domain.to_hex(),
        "override_write_autocomplete_location_context": options.override_write_autocomplete_location_context,
        "write_autocomplete_location_context": location_context_json(&options.write_autocomplete_location_context),
        "search_namespaces_into_full_tags": options.search_namespaces_into_full_tags,
        "unnamespaced_search_gives_any_namespace_wildcards": options.unnamespaced_search_gives_any_namespace_wildcards,
        "namespace_bare_fetch_all_allowed": options.namespace_bare_fetch_all_allowed,
        "namespace_fetch_all_allowed": options.namespace_fetch_all_allowed,
        "fetch_all_allowed": options.fetch_all_allowed,
        "fetch_results_automatically": options.fetch_results_automatically,
        "exact_match_character_threshold": options.exact_match_character_threshold,
    })
}

fn tag_display_manager_json(manager: &TagDisplayManager) -> Value {
    let filters: Vec<Value> = manager
        .tag_filters
        .iter()
        .map(|(display_type, per_service)| {
            let per_service: Vec<Value> = per_service
                .iter()
                .map(|(key, filter)| json!([key.to_hex(), rules(filter)]))
                .collect();
            json!([display_type, per_service])
        })
        .collect();
    let autocomplete: Vec<Value> = manager
        .autocomplete_options
        .iter()
        .map(autocomplete_json)
        .collect();
    json!({ "tag_filters": filters, "autocomplete_options": autocomplete })
}

#[test]
fn tag_display_and_favourite_searches_match_the_reference() {
    let fixture = basic();
    let manager = fixture.db.tag_display_manager().unwrap().unwrap();
    assert_eq!(
        tag_display_manager_json(&manager),
        fixture.expected["tag_display_manager"]
    );

    let favourites = fixture.db.favourite_search_manager().unwrap().unwrap();
    let actual: Vec<Value> = favourites
        .searches
        .iter()
        .map(|s| {
            json!({
                "folder": s.folder,
                "name": s.name,
                "file_search_context": file_search_context_json(&s.file_search_context),
                "synchronised": s.synchronised,
                "media_sort": s.media_sort.as_ref().map(media_sort_json),
                "media_collect": s.media_collect.as_ref().map(media_collect_json),
            })
        })
        .collect();
    assert_eq!(json!(actual), fixture.expected["favourite_searches"]);
}

fn tag_context_api(context: &TagContext) -> Value {
    tag_context_json(context)
}

/// `MediaSort.ToDictForAPI`.
fn media_sort_api(sort: &MediaSort) -> Value {
    let mut out = json!({
        "sort_metatype": match sort.sort_type {
            MediaSortType::System(_) => "system",
            MediaSortType::Namespaces { .. } => "namespaces",
            MediaSortType::Rating(_) => "rating",
        },
        "sort_order": sort.sort_order.code(),
        "tag_context": tag_context_api(&sort.tag_context),
    });
    match &sort.sort_type {
        MediaSortType::System(code) => out["sort_type"] = json!(code),
        MediaSortType::Namespaces {
            namespaces,
            tag_display_type,
        } => {
            out["namespaces"] = json!(namespaces);
            out["tag_display_type"] = json!(tag_display_type);
        }
        MediaSortType::Rating(key) => out["service_key"] = json!(key.to_hex()),
    }
    out
}

/// `TagSort.ToDictForAPI`.
fn tag_sort_api(sort: &TagSort) -> Value {
    json!({
        "sort_type": sort.sort_type,
        "sort_order": sort.sort_order.code(),
        "use_siblings": sort.use_siblings,
        "group_by": sort.group_by,
    })
}

#[test]
fn client_options_match_what_the_api_reports() {
    let fixture = basic();
    let options = fixture.db.client_options().unwrap().unwrap();
    let api = &fixture.expected["client_options_api"];

    assert_eq!(json!(options.booleans), api["booleans"]);
    assert_eq!(json!(options.strings), api["strings"]);
    assert_eq!(json!(options.noneable_strings), api["noneable_strings"]);
    assert_eq!(json!(options.integers), api["integers"]);
    assert_eq!(json!(options.noneable_integers), api["noneable_integers"]);
    let keys: BTreeMap<&String, String> = options
        .keys
        .iter()
        .map(|(k, v)| (k, hex::encode(v)))
        .collect();
    assert_eq!(json!(keys), api["keys"]);
    let colours: BTreeMap<&String, BTreeMap<String, _>> = options
        .colours
        .iter()
        .map(|(set, colours)| {
            (
                set,
                colours
                    .iter()
                    .map(|(c, rgb)| (c.to_string(), rgb))
                    .collect(),
            )
        })
        .collect();
    assert_eq!(json!(colours), api["colors"]);
    assert_eq!(json!(options.media_zooms), api["media_zooms"]);
    assert_eq!(
        json!(options.slideshow_durations),
        api["slideshow_durations"]
    );
    let namespace_sorts: Vec<Value> = options
        .default_namespace_sorts
        .iter()
        .map(media_sort_api)
        .collect();
    assert_eq!(json!(namespace_sorts), api["default_namespace_sorts"]);
    assert_eq!(
        media_sort_api(options.default_sort.as_ref().unwrap()),
        api["default_sort"]
    );
    assert_eq!(
        media_sort_api(options.fallback_sort.as_ref().unwrap()),
        api["fallback_sort"]
    );
    for (field, presentation) in [
        ("default_tag_sort", 0),
        ("default_tag_sort_search_page", 0),
        ("default_tag_sort_search_page_manage_tags", 1),
        ("default_tag_sort_media_viewer", 2),
        ("default_tag_sort_media_vewier_manage_tags", 3),
    ] {
        assert_eq!(
            tag_sort_api(&options.default_tag_sorts[&presentation]),
            api[field],
            "{field}"
        );
    }
    let favourites: BTreeMap<String, &Vec<String>> = options
        .suggested_tags_favourites
        .iter()
        .map(|(k, v)| (k.to_hex(), v))
        .collect();
    assert_eq!(json!(favourites), api["suggested_tags_favourites"]);
    let location = options.default_local_location_context.as_ref().unwrap();
    let current: Vec<String> = location
        .current
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    let deleted: Vec<String> = location
        .deleted
        .iter()
        .map(hydrus_core::ServiceKey::to_hex)
        .collect();
    assert_eq!(
        json!({ "current_service_keys": current, "deleted_service_keys": deleted }),
        api["default_local_location_context"]
    );

    let extra = &fixture.expected["client_options_extra"];
    assert_eq!(
        options.floats.len(),
        extra["floats"].as_object().unwrap().len()
    );
    for (name, value) in &options.floats {
        assert!(
            (extra["floats"][name].as_f64().unwrap() - value).abs() < 1e-12,
            "{name}"
        );
    }
    assert_eq!(json!(options.string_lists), extra["string_list"]);
    assert_eq!(json!(options.integer_lists), extra["integer_list"]);
    assert_eq!(
        options.key_lists.len(),
        extra["key_list"].as_object().unwrap().len()
    );
    assert_eq!(
        json!(options.string_lists["favourite_tags"]),
        extra["favourite_tags"]
    );
    assert_eq!(
        media_collect_json(options.default_collect.as_ref().unwrap()),
        extra["default_collect"]
    );
    let filters = extra["favourite_tag_filters"].as_object().unwrap();
    assert_eq!(options.favourite_tag_filters.len(), filters.len());
    for (name, filter) in &options.favourite_tag_filters {
        check_tag_filter(filter, &filters[name], name);
    }
    // everything the reference stores is still available undecoded
    let Body::Dictionary(pairs) = &options.dictionary.body else {
        panic!("options dictionary is not structured");
    };
    let names: Vec<&str> = pairs.iter().filter_map(|(k, _)| k.as_str()).collect();
    assert_eq!(json!(names), extra["top_level_keys"]);
}

fn yaml_json(value: &YamlValue) -> Value {
    match value {
        YamlValue::None => Value::Null,
        YamlValue::Bool(b) => json!(b),
        YamlValue::Int(i) => json!(i),
        YamlValue::Float(f) => json!(f),
        YamlValue::Str(s) => json!(s),
        YamlValue::Bytes(b) => json!(hex::encode(b)),
        YamlValue::List(items) | YamlValue::Tuple(items) => {
            Value::Array(items.iter().map(yaml_json).collect())
        }
        // json.dumps turns a None key into "null"
        YamlValue::Map(entries) => Value::Object(
            entries
                .iter()
                .map(|(k, v)| {
                    let key = match k {
                        YamlValue::None => "null".to_owned(),
                        YamlValue::Str(s) => s.clone(),
                        other => panic!("unexpected key {other:?}"),
                    };
                    (key, yaml_json(v))
                })
                .collect(),
        ),
    }
}

#[test]
fn legacy_options_match_what_the_api_reports() {
    let fixture = basic();
    let options = fixture.db.legacy_options().unwrap();
    let actual: Map<String, Value> = options
        .api_entries()
        .map(|(k, v)| (k.to_owned(), yaml_json(v)))
        .collect();
    assert_eq!(
        Value::Object(actual),
        fixture.expected["legacy_options_api"]
    );
    assert_eq!(options.thumbnail_dimensions(), Some((150, 125)));
    assert_eq!(options.password_hash(), None);
    assert!(
        options
            .namespace_colours()
            .contains(&(None, [114, 160, 193]))
    );
}

#[test]
fn every_stored_object_round_trips_byte_for_byte() {
    let fixture = basic();
    let db = &fixture.db;
    let mut stored: Vec<(String, u16, Option<String>, u32, String)> = Vec::new();
    for object in db.json_dumps().unwrap() {
        stored.push((
            "json_dumps".into(),
            object.kind.code(),
            None,
            object.version,
            object.dump,
        ));
    }
    for object in collect(db.json_dumps_named()) {
        stored.push((
            "json_dumps_named".into(),
            object.kind.code(),
            Some(object.name),
            object.version,
            object.dump,
        ));
    }
    for object in collect(db.json_dumps_hashed()) {
        stored.push((
            "json_dumps_hashed".into(),
            object.kind.code(),
            None,
            object.version,
            object.dump,
        ));
    }

    let expected = fixture.expected["stored_objects"].as_array().unwrap();
    assert_eq!(stored.len(), expected.len());
    let mut structured_containers = 0;
    for ((table, kind, name, version, dump), expected) in stored.iter().zip(expected) {
        assert_eq!(table, expected["table"].as_str().unwrap());
        assert_eq!(u64::from(*kind), expected["type"].as_u64().unwrap());
        assert_eq!(name.as_deref(), expected["name"].as_str());
        assert_eq!(u64::from(*version), expected["version"].as_u64().unwrap());
        assert_eq!(
            hex::encode(sha2::Sha256::digest(dump.as_bytes())),
            expected["dump_sha256"].as_str().unwrap()
        );
        let object =
            SerialisableObject::from_stored(SerialisableType(*kind), name.clone(), *version, dump)
                .unwrap_or_else(|e| panic!("{table} {kind} {name:?}: {e}"));
        assert_eq!(&object.info_string(), dump, "{table} {kind} {name:?}");
        object.visit(&mut |o| {
            if matches!(
                o.body,
                Body::Dictionary(_) | Body::List(_) | Body::BytesDictionary(_)
            ) {
                structured_containers += 1;
            }
            let is_container = [
                SerialisableType::DICTIONARY,
                SerialisableType::LIST,
                SerialisableType::BYTES_DICT,
            ]
            .contains(&o.kind);
            assert!(
                !is_container || !matches!(o.body, Body::Other(_)),
                "a {} was not decoded structurally",
                o.kind
            );
        });
        // the options' info is itself a whole dictionary tuple
        if *kind == SerialisableType::CLIENT_OPTIONS.code() {
            let inner = SerialisableObject::from_tuple(&PyJson::parse(dump).unwrap()).unwrap();
            assert_eq!(&inner.to_tuple_string(), dump);
            inner.visit(&mut |_| structured_containers += 1);
        }
    }
    assert!(structured_containers > 50, "{structured_containers}");

    for service in db.services().unwrap() {
        let stored: String = service.dictionary.to_tuple_string();
        let again = SerialisableObject::from_tuple_str(&stored).unwrap();
        assert_eq!(again, service.dictionary);
    }
}

#[test]
fn serialisable_type_table_matches_the_reference() {
    let expected = expected();
    let types = expected["serialisable_types"].as_array().unwrap();
    assert_eq!(types.len(), SerialisableType::all().count());
    for entry in types {
        let kind = SerialisableType(u16::try_from(entry[0].as_u64().unwrap()).unwrap());
        assert_eq!(kind.name(), entry[1].as_str(), "{kind:?}");
        assert_eq!(
            kind.current_version().map(u64::from),
            entry[2].as_u64(),
            "{kind:?}"
        );
    }
}

fn vectors(name: &str) -> Vec<(PyJson, PyJson)> {
    let expected = expected();
    expected["upgrade_vectors"][name]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            (
                PyJson::parse(&v["old"].to_string()).unwrap(),
                PyJson::parse(&v["current"].to_string()).unwrap(),
            )
        })
        .collect()
}

#[test]
fn upgrades_match_the_reference() {
    for name in ["dictionary", "list"] {
        for (old, current) in vectors(name) {
            let upgraded = SerialisableObject::from_tuple(&old)
                .unwrap()
                .upgraded()
                .unwrap();
            assert_eq!(upgraded.to_tuple(), current, "{name}");
        }
    }
    for (old, current) in vectors("tag_context") {
        assert_eq!(
            TagContext::from_tuple(&old).unwrap(),
            TagContext::from_tuple(&current).unwrap()
        );
    }
    for (old, current) in vectors("media_sort") {
        assert_eq!(
            MediaSort::from_tuple(&old).unwrap(),
            MediaSort::from_tuple(&current).unwrap()
        );
    }
    for (old, current) in vectors("media_collect") {
        assert_eq!(
            MediaCollect::from_tuple(&old).unwrap(),
            MediaCollect::from_tuple(&current).unwrap()
        );
    }
    for (old, current) in vectors("api_permissions") {
        assert_eq!(
            ApiPermissions::from_tuple(&old).unwrap(),
            ApiPermissions::from_tuple(&current).unwrap()
        );
    }
    let object = |value: &PyJson| SerialisableObject::from_tuple(value).unwrap();
    for (old, current) in vectors("tag_autocomplete_options") {
        assert_eq!(
            TagAutocompleteOptions::from_object(&object(&old)).unwrap(),
            TagAutocompleteOptions::from_object(&object(&current)).unwrap()
        );
    }
    for (old, current) in vectors("tag_display_manager") {
        assert_eq!(
            TagDisplayManager::from_object(&object(&old)).unwrap(),
            TagDisplayManager::from_object(&object(&current)).unwrap()
        );
    }
    for (old, current) in vectors("file_search_context") {
        assert_eq!(
            FileSearchContext::from_object(&object(&old)).unwrap(),
            FileSearchContext::from_object(&object(&current)).unwrap()
        );
    }
}

#[test]
fn json_formatting_matches_python() {
    let expected = expected();
    for sample in expected["python_json"]["floats"].as_array().unwrap() {
        let bits: [u8; 8] = hex::decode(sample[0].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let value = f64::from_le_bytes(bits);
        let mut out = String::new();
        write_python_float(value, &mut out);
        assert_eq!(out, sample[1].as_str().unwrap(), "{value:e}");
        assert_eq!(
            PyJson::parse(&out).unwrap().as_f64().unwrap().to_bits(),
            value.to_bits()
        );
    }
    for sample in expected["python_json"]["strings"].as_array().unwrap() {
        let mut out = String::new();
        write_python_string(sample[0].as_str().unwrap(), &mut out);
        assert_eq!(out, sample[1].as_str().unwrap());
    }
}

#[test]
fn file_paths_match_the_reference() {
    let fixture = basic();
    let config = fixture.db.file_storage().unwrap();
    assert_eq!(config.granularity, 2);
    let layout = FileLayout::new(&config, fixture.db.db_dir()).unwrap();
    let files = fixture.expected["file_paths"].as_array().unwrap();
    assert!(!files.is_empty());
    for file in files {
        let hash: Sha256 = file["hash"].as_str().unwrap().parse().unwrap();
        let mime =
            hydrus_core::Mime::from_code(u8::try_from(file["mime"].as_u64().unwrap()).unwrap())
                .unwrap();
        let path = layout.file_path(&hash, mime).unwrap();
        assert_eq!(
            path,
            fixture.db.db_dir().join(file["file"].as_str().unwrap())
        );
        assert!(path.is_file(), "{}", path.display());
        let thumbnail = layout.thumbnail_path(&hash).unwrap();
        assert_eq!(
            thumbnail,
            fixture
                .db
                .db_dir()
                .join(file["thumbnail"].as_str().unwrap())
        );
        assert_eq!(
            thumbnail.is_file(),
            file["thumbnail_exists"].as_bool().unwrap()
        );
    }
    assert_eq!(layout.directories().count(), 512);
    for example in fixture.expected["path_examples"].as_array().unwrap() {
        let hash: Sha256 = example["hash"].as_str().unwrap().parse().unwrap();
        let kind = match example["kind"].as_str().unwrap() {
            "f" => PrefixKind::File,
            _ => PrefixKind::Thumbnail,
        };
        let granularity = usize::try_from(example["granularity"].as_u64().unwrap()).unwrap();
        let p = prefix(&hash, kind, granularity);
        assert_eq!(p, example["prefix"].as_str().unwrap());
        assert_eq!(
            prefix_directories(&p).join("/"),
            example["relative_dir"].as_str().unwrap()
        );
    }
}
