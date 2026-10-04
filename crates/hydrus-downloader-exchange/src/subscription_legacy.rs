//! Reference legacy subscription conversion, before list-owner name handling.
use crate::{Error, Result, encode, subscriptions};
use hydrus_core::subscriptions::SeedTime;
use serde_json::{Value, json};

fn empty_tags() -> Value {
    json!([
        6,
        9,
        [
            false,
            false,
            [150, 1, [[44, 1, []], []]],
            [151, 1, []],
            false
        ]
    ])
}

fn upgrade_info(value: &Value, version: u32) -> Result<Vec<Value>> {
    let mut info = value[3]
        .as_array()
        .cloned()
        .ok_or_else(|| Error::Invalid("Malformed legacy subscription.".into()))?;
    let expected = match version {
        1 | 8 => 13,
        2 | 6 | 7 | 9 => 14,
        3 => 16,
        4 => 12,
        5 => 11,
        10 => 15,
        _ => {
            return Err(Error::Unsupported(format!(
                "legacy subscription version {version}"
            )));
        }
    };
    if info.len() != expected {
        return Err(Error::Invalid(
            "Malformed legacy subscription fields.".into(),
        ));
    }
    if version == 1 {
        info.insert(11, json!(false));
    }
    if version <= 2 {
        info.insert(13, json!(0));
        info.insert(14, json!(""));
    }
    if version <= 3 {
        let period = info[3]
            .as_i64()
            .filter(|p| *p > 0)
            .ok_or_else(|| Error::Invalid("Invalid legacy subscription period.".into()))?;
        let slow = period
            .checked_mul(10)
            .ok_or_else(|| Error::Invalid("Legacy subscription period is too large.".into()))?;
        let query = json!([
            54,
            3,
            [
                info[2],
                null,
                false,
                info[10],
                0,
                false,
                0,
                [67, 1, [26, 3, []]],
                info[15],
                empty_tags()
            ]
        ]);
        info = vec![
            info[0].clone(),
            info[1].clone(),
            json!([query]),
            json!([52, 1, [5, period / 5, slow, [1, slow]]]),
            info[4].clone(),
            info[5].clone(),
            info[6].clone(),
            info[7].clone(),
            info[8].clone(),
            info[9].clone(),
            info[13].clone(),
            info[14].clone(),
        ];
    }
    if version <= 4 {
        info.remove(4);
    }
    if version <= 5 {
        info.extend([json!(true), json!(false), json!(true)]);
    }
    if version <= 6 {
        for limit in &mut info[4..=5] {
            if limit.is_null() || limit.as_i64().is_some_and(|n| n > 1000) {
                *limit = json!(1000);
            }
        }
    }
    if version <= 7 {
        drop(info.drain(0..2));
        info.insert(
            0,
            json!([
                hydrus_core::pages::PageKey::random().to_hex(),
                "unknown downloader"
            ]),
        );
        info[5] = json!(true);
    }
    if version <= 8 {
        info.insert(10, json!(true));
    }
    if version <= 9 {
        info.insert(13, Value::Null);
    }
    Ok(info)
}

pub(crate) fn convert(value: &Value, now: i64) -> Result<subscriptions::Subscription> {
    let object = subscriptions::object(value)?;
    let name = object
        .name
        .ok_or_else(|| Error::Invalid("Legacy subscription has no name.".into()))?;
    let mut info = upgrade_info(value, object.version)?;
    let legacy_queries = info[1]
        .as_array()
        .ok_or_else(|| Error::Invalid("Malformed legacy query list.".into()))?;
    let mut headers = Vec::new();
    let mut logs = Vec::new();
    for query in legacy_queries {
        let object = subscriptions::object(query)?;
        if object.kind.code() != 54 || !matches!(object.version, 1..=3) {
            return Err(Error::Unsupported(
                "Unsupported legacy subscription query.".into(),
            ));
        }
        let mut q = query[2]
            .as_array()
            .cloned()
            .ok_or_else(|| Error::Invalid("Malformed legacy query.".into()))?;
        let expected = match object.version {
            1 => 7,
            2 => 8,
            _ => 10,
        };
        if q.len() != expected {
            return Err(Error::Invalid("Malformed legacy query fields.".into()));
        }
        if object.version == 1 {
            q.insert(6, json!([67, 1, [26, 3, []]]));
        }
        if object.version <= 2 {
            q.insert(1, Value::Null);
            q.push(empty_tags());
        }
        let log_name = hydrus_core::pages::PageKey::random().to_hex();
        headers.push(json!([
            87,
            2,
            [
                log_name,
                q[0],
                q[1],
                q[2],
                q[3],
                q[4],
                q[5],
                q[6],
                0,
                [89, 1, [now, [], 0]],
                250,
                100,
                q[9],
                [0, 1],
                "unknown",
                null,
                null
            ]
        ]));
        logs.push(json!([86, log_name, 1, [q[7], q[8]]]));
    }
    info[1] = json!(headers);
    // ConvertLegacySubscriptionToNew sets the timestamp but does not copy its reason.
    info[9] = json!("");
    info.insert(5, json!(false));
    let current = json!([
        90,
        1,
        [[88, name, 2, info], encode::serialisable_list(logs)]
    ]);
    let mut subscription = subscriptions::decode_container(&current, now)?;
    for query in &mut subscription.queries {
        let log = query.log.as_ref().expect("legacy history is embedded");
        let times = log
            .file_seeds
            .iter()
            .map(|s| SeedTime {
                source_time: s.source_time,
                created: s.created,
            })
            .collect::<Vec<_>>();
        let checker = &subscription.settings.checker;
        if query.state.check_now {
            query.state.next_check_time = 0;
            query.state.dead = false;
        } else {
            if checker.is_dead(&times, query.state.last_check_time) {
                query.state.dead = true;
                if !log.file_seeds.iter().any(|s| s.status == 0) {
                    query.state.paused = true;
                }
            }
            query.state.next_check_time =
                checker.next_check_time(&times, query.state.last_check_time, now);
        }
        let mut counts: Vec<(i64, usize)> = Vec::new();
        for seed in &log.file_seeds {
            if let Some((_, count)) = counts.iter_mut().find(|(status, _)| *status == seed.status) {
                *count += 1;
            } else {
                counts.push((seed.status, 1));
            }
        }
        let file = log
            .file_seeds
            .iter()
            .enumerate()
            .skip(log.file_seeds.len().saturating_sub(30))
            .find(|(_, s)| matches!(s.status, 1 | 2 | 9))
            .or_else(|| {
                log.file_seeds
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.status == 0)
            })
            .or_else(|| {
                log.file_seeds
                    .iter()
                    .enumerate()
                    .nth(log.file_seeds.len().saturating_sub(10))
            });
        let gallery = log
            .gallery_seeds
            .iter()
            .enumerate()
            .find(|(_, s)| s.status == 0)
            .or_else(|| {
                log.gallery_seeds
                    .iter()
                    .enumerate()
                    .nth(log.gallery_seeds.len().saturating_sub(10))
            });
        let raw_log = subscriptions::log_tuple(log);
        let mut header = subscriptions::query_header_tuple(query)?;
        header[2][9] = json!([
            89,
            1,
            [
                now,
                counts,
                log.file_seeds.iter().map(|s| s.created).max().unwrap_or(0)
            ]
        ]);
        header[2][13] = json!(checker.raw_current_velocity(&times, query.state.last_check_time));
        header[2][14] = json!(checker.pretty_velocity(&times, query.state.last_check_time, false));
        header[2][15] = file
            .filter(|(_, s)| s.seed_type == 1)
            .map_or(Value::Null, |(i, _)| raw_log[3][1][2][2][i][1].clone());
        header[2][16] = gallery.map_or(Value::Null, |(i, _)| raw_log[3][0][2][2][i][1].clone());
        query.reference_header = Some(header);
    }
    Ok(subscription)
}
