//! Safe format upgrades at the interchange boundary. Unknown versions fail
//! through the legacy decoder, retaining the original user's source payload.
use crate::encode::serialisable_list;
use crate::{Error, Result};
use serde_json::{Value, json};

pub(crate) fn object(value: &mut Value, depth: usize) -> Result<()> {
    if depth > 64 {
        return Err(Error::Limit);
    }
    let Some(items) = value.as_array_mut() else {
        return Ok(());
    };
    let kind = items.first().and_then(Value::as_u64).unwrap_or(0);
    let vi = if items.len() == 4 && items[1].is_string() {
        2
    } else {
        1
    };
    if matches!(items.len(), 3 | 4)
        && matches!(kind, 27 | 30 | 31 | 50 | 58 | 59 | 60 | 62 | 133 | 135)
        && items.get(vi).is_some_and(Value::is_u64)
    {
        let version = items[vi].as_u64().unwrap_or(0);
        let info = &mut items[vi + 1];
        match (kind, version) {
            (27, 7) | (31, 3) | (59, 2) | (133, 1) => {
                let expected = if kind == 27 { 4 } else { 3 };
                let a = fields(info, expected)?;
                let at = if kind == 31 { 2 } else { a.len() - 1 };
                a.insert(at, json!(""));
                items[vi] = json!(version + 1);
            }
            (60, 2) => {
                fields(info, 2)?.insert(1, json!(""));
                items[vi] = json!(3);
            }
            (62, 1) => {
                fields(info, 5)?.extend([
                    json!(false),
                    json!([51, 1, [3, null, null, null, "example string"]]),
                ]);
                items[vi] = json!(3);
            }
            (135, 1) => {
                fields(info, 2)?.insert(1, json!(false));
                items[vi] = json!(2);
            }
            (50, 14) => {
                let a = array(info)?;
                if a.len() != 17 {
                    return Err(Error::Invalid("Malformed old URL class.".into()));
                }
                let booleans = a[4]
                    .as_array()
                    .filter(|b| b.len() == 9)
                    .ok_or_else(|| Error::Invalid("Malformed old URL class flags.".into()))?
                    .clone();
                a[3] = json!([139, 1, [[a[3].clone()], [], booleans[0], booleans[1]]]);
                a[4] = json!(&booleans[2..]);
                items[vi] = json!(15);
            }
            (30, 6) => {
                let a = array(info)?;
                if a.len() != 4 {
                    return Err(Error::Invalid("Malformed old content parser.".into()));
                }
                if a[1] == json!(0) && a[3] == json!("") {
                    a[3] = Value::Null;
                }
                items[vi] = json!(7);
            }
            (58, 1 | 2) => {
                let a = array(info)?;
                if version == 1 {
                    if a.len() != 6 {
                        return Err(Error::Invalid("Malformed old page parser.".into()));
                    }
                    a.push(json!({"url":"https://example.com/posts/index.php?id=123456"}));
                }
                if a.len() != 7 {
                    return Err(Error::Invalid("Malformed old page parser.".into()));
                }
                let old = array(&mut a[3])?.clone();
                let mut sub = Vec::new();
                for pair in old {
                    let p = pair
                        .as_array()
                        .filter(|p| p.len() == 2)
                        .ok_or_else(|| Error::Invalid("Malformed old subsidiary parser.".into()))?;
                    sub.push(json!([135, 2, [p[0], false, p[1]]]));
                }
                a[3] = serialisable_list(sub);
                items[vi] = json!(3);
            }
            _ => (),
        }
    }
    for item in items {
        object(item, depth + 1)?;
    }
    Ok(())
}
fn array(value: &mut Value) -> Result<&mut Vec<Value>> {
    value
        .as_array_mut()
        .ok_or_else(|| Error::Invalid("Malformed version upgrade fields.".into()))
}

fn fields(value: &mut Value, expected: usize) -> Result<&mut Vec<Value>> {
    let fields = array(value)?;
    if fields.len() != expected {
        return Err(Error::Invalid(format!(
            "Malformed old definition: expected {expected} fields, got {}.",
            fields.len()
        )));
    }
    Ok(fields)
}
