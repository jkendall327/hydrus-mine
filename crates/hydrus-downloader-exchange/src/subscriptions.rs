//! Complete reference subscription containers, including both URL histories.
//! Cached header data is retained alongside native settings and query state.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode, import_options, transport};
use hydrus_core::import_options::ImportOptionsSlice;
use hydrus_core::subscriptions::{QueryState, SubscriptionSettings};
use hydrus_legacy::{
    objects::subscriptions as legacy,
    serialisable::{Body, Meta, SerialisableObject},
};
use serde_json::{Value, json};

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

fn object(value: &Value) -> Result<SerialisableObject> {
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
fn decode_container(value: &Value) -> Result<Subscription> {
    let container = object(value)?;
    if container.kind.code() != 90 || container.version != 1 {
        return Err(Error::Unsupported(format!(
            "expected subscription container type 90 version 1, got type {} version {}",
            container.kind.code(),
            container.version
        )));
    }
    let info = value[2]
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| Error::Invalid("Malformed subscription container.".into()))?;
    let subscription =
        legacy::subscription(&object(&info[0])?).map_err(|e| Error::Unsupported(e.to_string()))?;
    if !subscription.unconverted.is_empty() {
        return Err(Error::Unsupported(subscription.unconverted.join("; ")));
    }
    let logs = values(&info[1])?
        .iter()
        .map(|v| legacy::query_log(&object(v)?).map_err(|e| Error::Unsupported(e.to_string())))
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
/// Decode the complete selection before staging any changes in its list owner.
pub fn decode_text(text: &str) -> Result<Vec<Subscription>> {
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value: Value = serde_json::from_str(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut out = Vec::new();
    fn collect(value: &Value, depth: usize, out: &mut Vec<Subscription>) -> Result<()> {
        if depth > 32 || out.len() >= MAX_OBJECTS {
            return Err(Error::Limit);
        }
        if object(value)?.kind.code() == 26 {
            for value in values(value)? {
                collect(&value, depth + 1, out)?;
            }
        } else {
            out.push(decode_container(value)?);
        }
        Ok(())
    }
    collect(&value, 0, &mut out)?;
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
fn query_tuple(query: &Query) -> Result<Value> {
    let s = &query.state;
    let options = import_options::tuple(&ImportOptionsSlice {
        tags: Some(s.tag_import_options.clone()),
        ..ImportOptionsSlice::default()
    })?;
    let tags = &options[2][2][0][1][1];
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
        .map(query_tuple)
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
