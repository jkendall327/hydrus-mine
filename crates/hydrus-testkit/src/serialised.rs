//! Comparing exports in the reference's serialised form with recordings of
//! the reference's own.

use serde_json::Value;

/// A serialised auto-resolution rule or comparator export (one object, or a
/// `SerialisableList`), normalised where the reference's text is not
/// determined by the object:
///
/// - a rule's id (`DuplicatesAutoResolutionRule`, type 128, the first item
///   of its info) is set to 0: it is a database or session number, and the
///   reference replaces it on every import (`_ImportRule`);
/// - lists the reference builds from Python sets are sorted: a location
///   context's (type 103) current and deleted service keys, and the hashes
///   of `system:hash` (12), `system:similar to files` (19) and
///   `system:similar to data` (48), whose order follows the process's
///   string hashing.
///
/// # Panics
/// If the text is not JSON.
pub fn normalised_rule_export(text: &str) -> Value {
    let mut value: Value = serde_json::from_str(text).expect("an export is JSON");
    normalise(&mut value);
    value
}

fn sort(value: &mut Value) {
    if let Some(items) = value.as_array_mut() {
        items.sort_by_key(ToString::to_string);
    }
}

fn normalise(value: &mut Value) {
    let Some(items) = value.as_array_mut() else {
        return;
    };
    let kind = items.first().and_then(Value::as_i64);
    match (kind, items.len()) {
        // [128, name, version, [id, ...]]
        (Some(128), 4) => {
            if let Some(id) = items[3].get_mut(0) {
                *id = Value::from(0);
            }
        }
        // [103, 1, [current, deleted]]
        (Some(103), 3) => {
            if let Some(parts) = items[2].as_array_mut() {
                parts.iter_mut().for_each(sort);
            }
        }
        // [14, 8, [predicate type, value, inclusive]]
        (Some(14), 3) => {
            if let Some(info) = items[2].as_array_mut()
                && info.len() == 3
            {
                match info[0].as_i64() {
                    Some(12 | 19) => {
                        if let Some(v) = info[1].get_mut(0) {
                            sort(v);
                        }
                    }
                    Some(48) => {
                        for i in 0..2 {
                            if let Some(v) = info[1].get_mut(i) {
                                sort(v);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
    for item in items.iter_mut() {
        normalise(item);
    }
}

/// Where two JSON values first differ (a path of indices and keys, and the
/// two values there), for an assertion's message; `None` if they are equal.
pub fn first_difference(a: &Value, b: &Value) -> Option<String> {
    fn walk(a: &Value, b: &Value, path: &mut Vec<String>) -> Option<String> {
        match (a, b) {
            (Value::Array(x), Value::Array(y)) => {
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    path.push(i.to_string());
                    if let Some(d) = walk(p, q, path) {
                        return Some(d);
                    }
                    path.pop();
                }
                (x.len() != y.len()).then(|| {
                    format!(
                        "at /{}: {} items against {}",
                        path.join("/"),
                        x.len(),
                        y.len()
                    )
                })
            }
            _ if a == b => None,
            _ => Some(format!("at /{}: {a} against {b}", path.join("/"))),
        }
    }
    walk(a, b, &mut Vec::new())
}
