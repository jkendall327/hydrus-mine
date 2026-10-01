//! The bandwidth manager (`ClientNetworkingBandwidth.NetworkBandwidthManager`,
//! type 94): the bandwidth rules per network context; and its tracker
//! containers (type 97, named), each context's usage
//! (`HydrusNetworking.BandwidthTracker`, type 39).

use super::domain::expect;
use super::util::{DecodeResult, int, list, malformed, opt_int, string, tuple};
use crate::pyjson::PyJson;
use crate::serialisable::{SerialisableObject, SerialisableType};

const MANAGER: SerialisableType = SerialisableType(94);
const RULES: SerialisableType = SerialisableType(38);
const TRACKER: SerialisableType = SerialisableType(39);
const CONTEXT: SerialisableType = SerialisableType(47);
const CONTAINER: SerialisableType = SerialisableType(97);

/// `CC.NETWORK_CONTEXT_DOMAIN`, `CC.NETWORK_CONTEXT_SUBSCRIPTION`: the kinds
/// whose data is text (the rest are bytes, kept as hex).
const TEXT_KINDS: [i64; 2] = [2, 5];

/// A network context: its kind, and its data (`None` for the global context
/// and the kinds' defaults; hex for contexts keyed by bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyNetworkContext {
    pub kind: i64,
    pub data: Option<String>,
}

/// A rule: `HC.BANDWIDTH_TYPE_*`, the span in seconds (`None`: the month),
/// and the most allowed.
pub type LegacyRule = (i64, Option<i64>, i64);

/// The bandwidth manager's rules, and the names of its usage containers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyBandwidthManager {
    pub tracker_names: Vec<String>,
    pub rules: Vec<(LegacyNetworkContext, Vec<LegacyRule>)>,
}

/// One context's usage: per bucket start time, bytes (months, days, hours,
/// minutes, seconds), then requests (the same).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyTrackerContainer {
    pub context: LegacyNetworkContext,
    pub counters: [Vec<(i64, i64)>; 10],
}

/// Decode a stored network context (version 1 kept text data as hex too).
pub fn network_context(value: &PyJson) -> DecodeResult<LegacyNetworkContext> {
    let object = SerialisableObject::from_tuple(value)
        .map_err(|e| malformed(CONTEXT, format!("network context: {e}")))?;
    expect(&object, CONTEXT, &[1, 2])?;
    let info = object.info();
    let [kind, data] = tuple::<2>(CONTEXT, &info, "network context")?;
    let kind = int(CONTEXT, kind, "context type")?;
    let data = match data {
        PyJson::Null => None,
        other => {
            let text = string(CONTEXT, other, "context data")?;
            if object.version == 1 && TEXT_KINDS.contains(&kind) {
                let bytes = hex::decode(&text)
                    .map_err(|_| malformed(CONTEXT, "version 1 context data is not hex"))?;
                Some(
                    String::from_utf8(bytes)
                        .map_err(|_| malformed(CONTEXT, "context data is not text"))?,
                )
            } else {
                Some(text)
            }
        }
    };
    Ok(LegacyNetworkContext { kind, data })
}

fn rules(value: &PyJson) -> DecodeResult<Vec<LegacyRule>> {
    let object = SerialisableObject::from_tuple(value)
        .map_err(|e| malformed(RULES, format!("bandwidth rules: {e}")))?;
    expect(&object, RULES, &[1])?;
    let info = object.info();
    list(RULES, &info, "rules")?
        .iter()
        .map(|rule| {
            let [kind, span, max] = tuple::<3>(RULES, rule, "rule")?;
            Ok((
                int(RULES, kind, "bandwidth type")?,
                opt_int(RULES, span, "time delta")?,
                int(RULES, max, "max allowed")?,
            ))
        })
        .collect()
}

/// Decode the bandwidth manager. (Rules for the long-gone "downloader"
/// context are dropped, as the reference drops them.)
pub fn bandwidth_manager(object: &SerialisableObject) -> DecodeResult<LegacyBandwidthManager> {
    expect(object, MANAGER, &[1])?;
    let info = object.info();
    let [names, all_rules] = tuple::<2>(MANAGER, &info, "bandwidth manager")?;
    let tracker_names = list(MANAGER, names, "tracker names")?
        .iter()
        .map(|n| string(MANAGER, n, "tracker name"))
        .collect::<DecodeResult<_>>()?;
    let mut out = Vec::new();
    for entry in list(MANAGER, all_rules, "rules")? {
        let [context, r] = tuple::<2>(MANAGER, entry, "context's rules")?;
        let context = network_context(context)?;
        if context.kind == 3 {
            continue;
        }
        out.push((context, rules(r)?));
    }
    Ok(LegacyBandwidthManager {
        tracker_names,
        rules: out,
    })
}

/// Decode a tracker container.
pub fn tracker_container(object: &SerialisableObject) -> DecodeResult<LegacyTrackerContainer> {
    expect(object, CONTAINER, &[1])?;
    let info = object.info();
    let [context, tracker] = tuple::<2>(CONTAINER, &info, "tracker container")?;
    let context = network_context(context)?;
    let tracker = SerialisableObject::from_tuple(tracker)
        .map_err(|e| malformed(TRACKER, format!("bandwidth tracker: {e}")))?;
    expect(&tracker, TRACKER, &[1])?;
    let info = tracker.info();
    let flat = list(TRACKER, &info, "counters")?;
    let mut counters: [Vec<(i64, i64)>; 10] = Default::default();
    // (the reference ignores a tracker that isn't ten counters)
    if flat.len() == 10 {
        for (slot, counter) in counters.iter_mut().zip(flat) {
            for pair in list(TRACKER, counter, "counter")? {
                let [t, n] = tuple::<2>(TRACKER, pair, "count")?;
                slot.push((int(TRACKER, t, "bucket")?, int(TRACKER, n, "count")?));
            }
        }
    }
    Ok(LegacyTrackerContainer { context, counters })
}
