//! Complete reference subscription containers, including both URL histories.
//! Cached header data is retained alongside native settings and query state.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, import_options, transport};
use hydrus_core::import_options::{ImportOptionsSlice, TagImportOptions};
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_legacy::{
    objects::subscriptions as legacy,
    serialisable::{Body, Meta, SerialisableObject},
};
pub use legacy::QueryLog;
use serde_json::{Value, json};

/// Decode one complete reference history container.
pub fn decode_log(value: &Value) -> Result<QueryLog> {
    let value = crate::subscription_seed_cache::log(value)?;
    legacy::query_log(&object(&value)?).map_err(|e| Error::Unsupported(e.to_string()))
}

/// A selected subscription and all its query histories, frozen for exchange.
#[derive(Debug, Clone, PartialEq)]
pub struct Subscription {
    pub name: String,
    pub settings: SubscriptionSettings,
    pub queries: Vec<Query>,
    /// Unreferenced history is retained until the list owner reinitialises it.
    pub orphaned_logs: Vec<legacy::QueryLog>,
}
/// A query's native state and complete reference history and cached metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub state: QueryState,
    /// `None` records the reference's recoverable missing-log condition.
    pub log: Option<legacy::QueryLog>,
    pub log_name: String,
    /// Cached velocity, status and example seeds which are not native settings.
    pub reference_header: Option<Value>,
}

/// The reference invalidates velocity when assigning a new history identity.
pub fn rename_history(query: &mut Query, name: String) {
    query.log_name = name;
    if let Some(log) = &mut query.log {
        log.name.clone_from(&query.log_name);
    }
    if let Some(header) = &mut query.reference_header {
        header[2][0] = json!(query.log_name);
        header[2][8] = json!(1);
        let velocity = if header[1] == json!(1) { 11 } else { 13 };
        header[2][velocity] = json!([0, 1]);
        header[2][velocity + 1] = json!("unknown");
    }
}
/// The velocity words a header shows until the reference reads its history
/// again (`SetCheckerOptions`' `pretty_velocity_override`).
pub const RECALCULATE_WORDS: &str = "will recalculate when next fully loaded";

/// What editing a subscription's checker options does to a query's cached
/// header (`Subscription.SetCheckerOptions`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckerEdit {
    /// Its history was not loaded: marked unsynced, to be recalculated by the
    /// reference's own sync when it next loads the history.
    Unsynced,
    /// Its history was loaded (an imported one, or one read for an export or
    /// a reset): recalculated at once (`SyncToQueryLogContainer`) with the
    /// velocity the new checker options find in it.
    Synced {
        /// `GetRawCurrentVelocity`: files found, over how many seconds.
        velocity: (i64, i64),
        /// `GetPrettyCurrentVelocity` without its prefix.
        words: String,
    },
}

/// Carry out a checker edit on a query's cached header, making the header if
/// it has none yet. A synced header also takes the example gallery seed of
/// its history; the reference picks one at random when none is unfinished,
/// native takes the first of the last ten.
pub fn apply_checker_edit(query: &mut Query, edit: &CheckerEdit, now: i64) -> Result<()> {
    if query.reference_header.is_none() || matches!(edit, CheckerEdit::Synced { .. }) {
        update_file_status(query, now)?;
    }
    if query.reference_header.is_none() {
        query.reference_header = Some(query_header_tuple(query)?);
    }
    let velocity_index = match query.reference_header.as_ref().map(|h| &h[1]) {
        Some(version) if *version == json!(1) => 11,
        _ => 13,
    };
    let example_gallery = query.log.as_ref().and_then(|log| {
        log.gallery_seeds
            .iter()
            .find(|s| s.status == 0)
            .or_else(|| {
                log.gallery_seeds
                    .get(log.gallery_seeds.len().saturating_sub(10))
            })
            .map(gallery)
    });
    let Some(header) = &mut query.reference_header else {
        return Ok(());
    };
    match edit {
        CheckerEdit::Unsynced => {
            header[2][8] = json!(1);
            header[2][velocity_index] = json!([0, 1]);
            header[2][velocity_index + 1] = json!(RECALCULATE_WORDS);
        }
        CheckerEdit::Synced { velocity, words } => {
            header[2][8] = json!(0);
            header[2][velocity_index] = json!([velocity.0, velocity.1]);
            header[2][velocity_index + 1] = json!(words);
            header[2][velocity_index + 3] = example_gallery.unwrap_or(Value::Null);
        }
    }
    Ok(())
}
/// Refresh the reference file-count and example-seed fields after a log edit.
/// Reset/retry keeps gallery examples and velocity unchanged (`UpdateFileStatus`).
pub fn update_file_status(query: &mut Query, now: i64) -> Result<()> {
    let Some(log) = &query.log else {
        return Ok(());
    };
    let mut counts: Vec<(i64, usize)> = Vec::new();
    for seed in &log.file_seeds {
        if let Some((_, count)) = counts.iter_mut().find(|(status, _)| *status == seed.status) {
            *count += 1;
        } else {
            counts.push((seed.status, 1));
        }
    }
    let example = log
        .file_seeds
        .iter()
        .rev()
        .take(30)
        .find(|s| matches!(s.status, 1 | 2 | 9))
        .or_else(|| log.file_seeds.iter().find(|s| s.status == 0))
        .or_else(|| log.file_seeds.get(log.file_seeds.len().saturating_sub(10)));
    let example = example
        .filter(|s| s.seed_type == 1)
        .map_or(Value::Null, file);
    let mut header = query_header_tuple(query)?;
    if query.reference_header.is_none() {
        header[2][8] = json!(1);
    }
    header[2][9] = json!([
        89,
        1,
        [
            now,
            counts,
            log.file_seeds.iter().map(|s| s.created).max().unwrap_or(0)
        ]
    ]);
    header[2][15] = example;
    query.reference_header = Some(header);
    Ok(())
}

fn tag_tuple(tags: &TagImportOptions) -> Result<Value> {
    let options = import_options::tuple(&ImportOptionsSlice {
        tags: Some(tags.clone()),
        ..ImportOptionsSlice::default()
    })?;
    Ok(options[2][2][0][1][1].clone())
}
fn normalise_headers(value: &Value) -> Result<Value> {
    let mut value = value.clone();
    if let Some(headers) = value
        .get_mut(2)
        .and_then(|v| v.get_mut(0))
        .and_then(|v| v.get_mut(3))
        .and_then(|v| v.get_mut(1))
        .and_then(Value::as_array_mut)
    {
        for header in headers {
            // Python's legacy subscription converter retains type6 inside v3 headers.
            if header[1] == json!(3) && header[2][12][0] == json!(6) {
                let converted = hydrus_legacy::objects::legacy_import_options::tag_import_options(
                    &object(&header[2][12])?,
                )
                .map_err(|e| Error::Unsupported(e.to_string()))?;
                header[2][12] = tag_tuple(&converted.tags)?;
            }
        }
    }
    Ok(value)
}

pub(crate) fn object(value: &Value) -> Result<SerialisableObject> {
    let object = SerialisableObject::from_tuple_str(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    object
        .check_not_future()
        .map_err(|e| Error::Unsupported(e.to_string()))?;
    Ok(object)
}
fn values(value: &Value) -> Result<Vec<Value>> {
    let object = object(value)?
        .upgraded()
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let Body::List(items) = object.body else {
        return Err(Error::Invalid("Expected a serialisable list.".into()));
    };
    items
        .into_iter()
        .map(|item| {
            let Meta::Object(item) = item else {
                return Err(Error::Invalid(
                    "Subscription package contains a non-object value.".into(),
                ));
            };
            serde_json::from_str(&item.to_tuple().to_python_string())
                .map_err(|e| Error::Invalid(e.to_string()))
        })
        .collect()
}
fn validate_headers(headers: &[Value]) -> Result<()> {
    for header in headers {
        let header_object = object(header)?;
        let fields = header[2]
            .as_array()
            .ok_or_else(|| Error::Invalid("Malformed query header.".into()))?;
        let expected = if header_object.version == 1 { 15 } else { 17 };
        if fields.len() != expected {
            return Err(Error::Invalid("Malformed query header cache.".into()));
        }
        let status = object(&fields[9])?;
        if status.kind.code() != 89 || status.version != 1 {
            return Err(Error::Unsupported(
                "Unsupported file seed cache status.".into(),
            ));
        }
        let examples = if header_object.version == 1 { 13 } else { 15 };
        if !fields[examples].is_null() {
            legacy::file_seed(&object(&fields[examples])?)
                .map_err(|e| Error::Unsupported(e.to_string()))?;
        }
        if !fields[examples + 1].is_null() {
            legacy::gallery_seed(&object(&fields[examples + 1])?)
                .map_err(|e| Error::Unsupported(e.to_string()))?;
        }
    }
    Ok(())
}
pub(crate) fn decode_container(value: &Value, now: i64) -> Result<Subscription> {
    let container = object(value)?;
    if container.kind.code() == 3 {
        return crate::subscription_legacy::convert(value, now);
    }
    if container.kind.code() != 90 || container.version != 1 {
        return Err(Error::Unsupported(format!(
            "expected subscription container type 90 version 1, got type {} version {}",
            container.kind.code(),
            container.version
        )));
    }
    let normalised = normalise_headers(value)?;
    let value = &normalised;
    let info = value[2]
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| Error::Invalid("Malformed subscription container.".into()))?;
    let subscription =
        legacy::subscription(&object(&info[0])?).map_err(|e| Error::Unsupported(e.to_string()))?;
    hex::decode(&subscription.gug_key).map_err(|e| Error::Invalid(e.to_string()))?;
    if !subscription.unconverted.is_empty() {
        return Err(Error::Unsupported(subscription.unconverted.join("; ")));
    }
    let logs = values(&info[1])?
        .iter()
        .map(decode_log)
        .collect::<Result<Vec<_>>>()?;
    let headers = info[0][3][1]
        .as_array()
        .ok_or_else(|| Error::Invalid("Malformed query headers.".into()))?;
    validate_headers(headers)?;
    let orphaned_logs = logs
        .iter()
        .filter(|log| !subscription.queries.iter().any(|q| q.log_name == log.name))
        .cloned()
        .collect();
    let queries = subscription
        .queries
        .iter()
        .zip(headers)
        .map(|(q, header)| Query {
            state: QueryState {
                query_text: q.query_text.clone(),
                display_name: q.display_name.clone(),
                check_now: q.check_now,
                last_check_time: q.last_check_time,
                next_check_time: q.next_check_time,
                paused: q.paused,
                dead: q.checker_status == 1,
                file_seed_compaction_number: q.file_seed_compaction_number,
                gallery_seed_compaction_number: q.gallery_seed_compaction_number,
                tag_import_options: q.tag_import_options.clone(),
            },
            log: logs
                .iter()
                .rev()
                .find(|log| log.name == q.log_name)
                .cloned(),
            log_name: q.log_name.clone(),
            reference_header: Some(header.clone()),
        })
        .collect();
    let limit = |n: Option<i64>| n.map(|n| u64::try_from(n).unwrap_or(0));
    Ok(Subscription {
        name: subscription.name,
        settings: SubscriptionSettings {
            gug_key: subscription.gug_key,
            gug_name: subscription.gug_name,
            checker: subscription.checker,
            initial_file_limit: limit(subscription.initial_file_limit),
            periodic_file_limit: limit(subscription.periodic_file_limit),
            this_is_a_random_sample: subscription.this_is_a_random_sample,
            paused: subscription.paused,
            import_options: subscription.import_options,
            no_work_until: subscription.no_work_until,
            no_work_until_reason: subscription.no_work_until_reason,
            show_a_popup_while_working: subscription.show_a_popup_while_working,
            publish_files_to_popup_button: subscription.publish_files_to_popup_button,
            publish_files_to_page: subscription.publish_files_to_page,
            publish_label_override: subscription.publish_label_override,
            merge_query_publish_events: subscription.merge_query_publish_events,
        },
        queries,
        orphaned_logs,
    })
}
fn collect(value: &Value, depth: usize, now: i64, out: &mut Vec<Subscription>) -> Result<()> {
    if depth > 32 || out.len() >= MAX_OBJECTS {
        return Err(Error::Limit);
    }
    if object(value)?.kind.code() == 26 {
        for value in values(value)? {
            collect(&value, depth + 1, now, out)?;
        }
    } else {
        out.push(decode_container(value, now)?);
    }
    Ok(())
}
/// Decode the complete selection before staging any changes in its list owner.
pub fn decode_text(text: &str) -> Result<Vec<Subscription>> {
    decode_text_at(text, hydrus_core::time::TimestampMs::now().millis() / 1000)
}
/// Decode with the reference header-cache clock supplied by a caller or replay.
pub fn decode_text_at(text: &str, now: i64) -> Result<Vec<Subscription>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut out = Vec::new();
    collect(&value, 0, now, &mut out)?;
    if out.is_empty() {
        return Err(Error::Invalid(
            "The package contains no subscriptions.".into(),
        ));
    }
    Ok(out)
}
fn service_tags(tags: &[(String, Vec<String>)]) -> Value {
    json!([77, 1, tags])
}
fn headers(headers: &[(String, String)]) -> Value {
    Value::Object(headers.iter().map(|(k, v)| (k.clone(), json!(v))).collect())
}
fn file(seed: &legacy::LegacyFileSeed) -> Value {
    json!([
        57,
        8,
        [
            seed.seed_type,
            seed.data,
            seed.data_for_comparison.as_ref().unwrap_or(&seed.data),
            seed.created,
            seed.modified,
            seed.source_time,
            seed.status,
            seed.note,
            seed.referral_url,
            headers(&seed.request_headers),
            seed.external_filterable_tags,
            service_tags(&seed.external_additional_tags),
            seed.primary_urls,
            seed.source_urls,
            seed.tags,
            seed.notes,
            seed.hashes
        ]
    ])
}
fn gallery(seed: &legacy::LegacyGallerySeed) -> Value {
    json!([
        66,
        4,
        [
            seed.url,
            seed.can_generate_more_pages,
            seed.external_filterable_tags,
            service_tags(&seed.external_additional_tags),
            seed.created,
            seed.modified,
            seed.status,
            seed.note,
            seed.referral_url,
            headers(&seed.request_headers)
        ]
    ])
}
/// Encode a log without discarding ignored reasons, notes, hashes or URL metadata.
pub fn log_tuple(log: &legacy::QueryLog) -> Value {
    json!([
        86,
        log.name,
        1,
        [
            [
                67,
                1,
                encode::serialisable_list(log.gallery_seeds.iter().map(gallery).collect())
            ],
            [
                8,
                8,
                encode::serialisable_list(log.file_seeds.iter().map(file).collect())
            ]
        ]
    ])
}
/// Encode native query fields while preserving cached reference header data.
pub fn query_header_tuple(query: &Query) -> Result<Value> {
    let s = &query.state;
    let tags = tag_tuple(&s.tag_import_options)?;
    let cached = |index: usize, default: Value| {
        query.reference_header.as_ref().map_or(default, |h| {
            let index = if h[1] == json!(1) && index >= 13 {
                index - 2
            } else {
                index
            };
            h[2][index].clone()
        })
    };
    Ok(json!([
        87,
        3,
        [
            query.log_name,
            s.query_text,
            s.display_name,
            s.check_now,
            s.last_check_time,
            s.next_check_time,
            s.paused,
            i64::from(s.dead),
            cached(8, json!(0)),
            cached(9, json!([89, 1, [0, [], 0]])),
            s.file_seed_compaction_number,
            s.gallery_seed_compaction_number,
            tags,
            cached(13, json!([0, 1])),
            cached(14, json!("unknown")),
            cached(15, Value::Null),
            cached(16, Value::Null)
        ]
    ]))
}
/// Current reference container format with the complete selected histories.
pub fn tuple(subscription: &Subscription) -> Result<Value> {
    let s = &subscription.settings;
    let headers = subscription
        .queries
        .iter()
        .map(query_header_tuple)
        .collect::<Result<Vec<_>>>()?;
    let checker = &s.checker;
    let intended: Value = serde_json::from_str(&checker.intended_files_per_check.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?;
    let sub = json!([
        88,
        subscription.name,
        4,
        [
            [s.gug_key, s.gug_name],
            headers,
            [
                52,
                1,
                [
                    intended,
                    checker.never_faster_than,
                    checker.never_slower_than,
                    checker.death_file_velocity
                ]
            ],
            s.initial_file_limit,
            s.periodic_file_limit,
            s.this_is_a_random_sample,
            s.paused,
            import_options::tuple(&s.import_options)?,
            s.no_work_until,
            s.no_work_until_reason,
            s.show_a_popup_while_working,
            s.publish_files_to_popup_button,
            s.publish_files_to_page,
            s.publish_label_override,
            s.merge_query_publish_events
        ]
    ]);
    let logs = subscription
        .queries
        .iter()
        .filter_map(|q| q.log.as_ref())
        .chain(subscription.orphaned_logs.iter())
        .map(log_tuple)
        .collect();
    Ok(json!([90, 1, [sub, encode::serialisable_list(logs)]]))
}
/// Export one selected subscription or an ordered selection package.
pub fn encode_text(subscriptions: &[Subscription]) -> Result<String> {
    if subscriptions.is_empty() || subscriptions.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let values = subscriptions
        .iter()
        .map(tuple)
        .collect::<Result<Vec<_>>>()?;
    let value = if values.len() == 1 {
        values[0].clone()
    } else {
        encode::serialisable_list(values)
    };
    let text = hydrus_core::pyjson::PyJson::parse(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}
/// Encode the reference compressed PNG carrier.
pub fn encode_png(subscriptions: &[Subscription]) -> Result<Vec<u8>> {
    transport::encode_payload(&encode_text(subscriptions)?)
}
/// Decode reference PNG subscription containers.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<Subscription>> {
    decode_text(&transport::decode_payload(bytes)?)
}
