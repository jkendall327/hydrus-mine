//! Reference external-call clipboard objects. Packages decode completely before
//! the owner may append calls and generate fresh native identities.
use crate::{Error, MAX_BYTES, MAX_OBJECTS, Result, encode};
use hydrus_core::external_calls::{
    ActualCall, Callable, Manager, Parameter, Pipeline, Process, Rule,
};
use hydrus_legacy::{objects::domain, serialisable::SerialisableObject};
use serde_json::{Value, json};

fn invalid(message: &str) -> Error {
    Error::Invalid(message.into())
}
fn tuple(value: &Value, code: i64, len: usize) -> Result<&[Value]> {
    let array = value
        .as_array()
        .filter(|a| a.len() == 3 && a[0] == code && a[1] == 1)
        .ok_or_else(|| invalid("Expected a supported external-call object/version."))?;
    array[2]
        .as_array()
        .filter(|a| a.len() == len)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("Invalid external-call fields."))
}
fn number(value: &Value) -> Result<i64> {
    value
        .as_i64()
        .ok_or_else(|| invalid("Expected an external-call integer."))
}
fn text(value: &Value) -> Result<String> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Expected external-call text."))
}
fn boolean(value: &Value) -> Result<bool> {
    value
        .as_bool()
        .ok_or_else(|| invalid("Expected an external-call boolean."))
}
fn list(value: &Value) -> Result<Vec<&Value>> {
    let a = value
        .as_array()
        .filter(|a| a.len() == 3 && a[0] == 26 && a[1] == 3)
        .ok_or_else(|| invalid("Expected a reference SerialisableList."))?;
    let items = a[2]
        .as_array()
        .ok_or_else(|| invalid("Invalid external-call list."))?;
    if items.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    items
        .iter()
        .map(|item| {
            item.as_array()
                .filter(|a| a.len() == 2 && a[0] == 2)
                .map(|a| &a[1])
                .ok_or_else(|| invalid("Expected serialisable external-call list entries."))
        })
        .collect()
}
fn decode_call(value: &Value) -> Result<Callable> {
    let array = value
        .as_array()
        .filter(|a| a.len() == 4 && a[0] == 156 && a[2] == 1)
        .ok_or_else(|| invalid("Expected an Executable Manager Callable (version 1)."))?;
    let info = array[3]
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or_else(|| invalid("Invalid callable fields."))?;
    let key: [u8; 32] = hex::decode(text(&info[0])?)
        .map_err(|_| invalid("Invalid callable key."))?
        .try_into()
        .map_err(|_| invalid("A callable key must contain 32 bytes."))?;
    let pipeline = match number(&info[1])? {
        1 => Pipeline::File,
        2 => Pipeline::Url,
        code => return Err(Error::Unsupported(format!("external-call job {code}"))),
    };
    let actual = &info[2];
    let call = match number(&actual[0])? {
        157 => {
            let a = tuple(actual, 157, 7)?;
            let args = a[1]
                .as_array()
                .ok_or_else(|| invalid("Invalid process parameter templates."))?
                .iter()
                .map(text)
                .collect::<Result<Vec<_>>>()?;
            let mut rules = Vec::new();
            for value in list(&a[2])? {
                let r = tuple(value, 159, 3)?;
                let parameter = Parameter::from_code(number(&r[0])?)
                    .ok_or_else(|| invalid("Unsupported process input parameter."))?;
                let object = SerialisableObject::from_tuple_str(&r[2].to_string())
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                let processor =
                    domain::string_processor(&object).map_err(|e| Error::Invalid(e.to_string()))?;
                encode::processor(&processor)?;
                rules.push(Rule {
                    parameter,
                    token: text(&r[1])?,
                    processor,
                });
            }
            ActualCall::Process(Process {
                executable: text(&a[0])?,
                arguments: hydrus_core::external_calls::clean_arguments(&args),
                rules,
                timeout_seconds: a[3]
                    .as_u64()
                    .filter(|n| *n > 0)
                    .ok_or_else(|| invalid("The process timeout must be positive."))?,
                long_lived: boolean(&a[4])?,
                hide_terminal: boolean(&a[5])?,
                text: boolean(&a[6])?,
            })
        }
        160 => {
            tuple(actual, 160, 0)?;
            ActualCall::DefaultFile
        }
        161 => {
            tuple(actual, 161, 0)?;
            ActualCall::DefaultUrl
        }
        code => {
            return Err(Error::Unsupported(format!(
                "actual external-call type {code}"
            )));
        }
    };
    Ok(Callable {
        key,
        name: text(&array[1])?,
        pipeline,
        call,
    })
}
/// Import a single callable or a reference list atomically.
pub fn decode_text(raw: &str) -> Result<Vec<Callable>> {
    if raw.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value: Value = serde_json::from_str(raw).map_err(|e| Error::Invalid(e.to_string()))?;
    let calls = if value[0] == 26 {
        list(&value)?
            .into_iter()
            .map(decode_call)
            .collect::<Result<Vec<_>>>()?
    } else {
        vec![decode_call(&value)?]
    };
    if calls.is_empty() {
        return Err(invalid("The package contains no external calls."));
    }
    Ok(calls)
}
fn call_tuple(call: &Callable) -> Result<Value> {
    let actual = match &call.call {
        ActualCall::DefaultFile => json!([160, 1, []]),
        ActualCall::DefaultUrl => json!([161, 1, []]),
        ActualCall::Process(p) => {
            let rules = p
                .rules
                .iter()
                .map(|r| {
                    Ok(json!([
                        2,
                        [
                            159,
                            1,
                            [
                                r.parameter.code(),
                                r.token,
                                encode::processor(&r.processor)?
                            ]
                        ]
                    ]))
                })
                .collect::<Result<Vec<_>>>()?;
            json!([
                157,
                1,
                [
                    p.executable,
                    p.arguments,
                    [26, 3, rules],
                    p.timeout_seconds,
                    p.long_lived,
                    p.hide_terminal,
                    p.text
                ]
            ])
        }
    };
    Ok(json!([
        156,
        call.name,
        1,
        [
            hex::encode(call.key),
            match call.pipeline {
                Pipeline::File => 1,
                Pipeline::Url => 2,
            },
            actual
        ]
    ]))
}
/// Export selected calls using the same single-object/list rule as reference Qt.
pub fn encode_text(calls: &[Callable]) -> Result<String> {
    if calls.is_empty() {
        return Err(invalid("Select external calls to export."));
    }
    if calls.len() > MAX_OBJECTS {
        return Err(Error::Limit);
    }
    let value = if calls.len() == 1 {
        call_tuple(&calls[0])?
    } else {
        json!([
            26,
            3,
            calls
                .iter()
                .map(|c| Ok(json!([2, call_tuple(c)?])))
                .collect::<Result<Vec<_>>>()?
        ])
    };
    let text = hydrus_core::pyjson::PyJson::parse(&value.to_string())
        .map_err(|e| Error::Invalid(e.to_string()))?
        .to_python_string();
    if text.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    Ok(text)
}
/// Decode the manager saved by the reference database without changing keys.
pub fn decode_manager(raw: &str) -> Result<Manager> {
    if raw.len() > MAX_BYTES {
        return Err(Error::Limit);
    }
    let value: Value = serde_json::from_str(raw).map_err(|e| Error::Invalid(e.to_string()))?;
    let array = value
        .as_array()
        .filter(|a| a.len() == 3 && a[0] == 155 && a[1] == 1)
        .ok_or_else(|| invalid("Expected Executable Manager version 1."))?;
    Ok(Manager {
        calls: list(&array[2])?
            .into_iter()
            .map(decode_call)
            .collect::<Result<_>>()?,
    })
}

/// Defaults emitted by the executed reference factory, for every platform.
/// The list owner generates new keys and resolves duplicate names on insertion.
pub fn defaults(platform_only: bool) -> Result<Vec<Callable>> {
    let mut calls = decode_text(include_str!("external_call_defaults.json"))?;
    if platform_only {
        calls.retain(|call| {
            let name = call.name.as_str();
            if name.starts_with("Default OS ") {
                return true;
            }
            if cfg!(target_os = "macos") {
                name.ends_with("(macOS)")
            } else if cfg!(windows) {
                name.ends_with("(Windows)")
                    || name.starts_with("firefox ") && !name.ends_with("(macOS)")
            } else {
                !name.ends_with("(Windows)") && !name.ends_with("(macOS)")
            }
        });
    }
    Ok(calls)
}

/// Export calls in the reference compressed PNG carrier.
pub fn encode_png(calls: &[Callable]) -> Result<Vec<u8>> {
    crate::transport::encode_payload(&encode_text(calls)?)
}
/// Decode a bounded reference PNG before the owner stages any calls.
pub fn decode_png(bytes: &[u8]) -> Result<Vec<Callable>> {
    decode_text(&crate::transport::decode_payload(bytes)?)
}
