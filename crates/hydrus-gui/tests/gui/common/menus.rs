//! The reference's right-click menus as hydrus-rs has them so far: the
//! entries it lacks left out, and its menus described alike.

#![allow(dead_code)]

use hydrus_gui::thumbnail_menu::Entry;
use serde_json::{Value, json};

/// Which of the reference's entries hydrus-rs has.
pub enum Kept {
    No,
    /// The entry, and all of a submenu's entries.
    All,
    /// A submenu with just these of its entries.
    Only(&'static [&'static str]),
}

pub fn kept(label: &str) -> Kept {
    match label {
        "manage" => Kept::Only(&[
            "tags",
            "ratings",
            "notes",
            "notes (",
            "times",
            "force filetype",
        ]),
        "open" => Kept::Only(&[
            "in a new page",
            "in a new duplicate filter page",
            "similar files in a new page",
            "using Default OS File Launch",
            "in web browser",
            "focused file using Default OS File Launch",
            "focused file in web browser",
        ]),
        "urls" => Kept::Only(&["manage", "open in browser", "open in a new page", "copy"]),
        "share" => Kept::Only(&[
            "copy files",
            "copy file",
            "export files",
            "copy paths",
            "copy hashes",
            "copy file ids",
            "copy path",
            "copy hash",
            "copy file id (",
        ]),
        "select"
        | "remove"
        | "rearrange"
        | "delete"
        | "delete selected"
        | "refresh"
        | "archive/delete filter"
        | "archive"
        | "archive selected"
        | "re-inbox"
        | "re-inbox selected"
        | "delete trash physically now"
        | "delete physically now"
        | "delete selected physically now"
        | "undelete"
        | "undelete selected" => Kept::All,
        l if l.starts_with("delete from ") => Kept::All,
        // the media viewer's
        "volume" | "remove from view" | "go fullscreen" | "exit fullscreen" | "return to inbox"
        | "player" | "start slideshow" | "slideshow running" => Kept::All,
        l if l.starts_with("zoom: ") => Kept::All,
        _ => Kept::No,
    }
}

/// Separators as Qt shows them: none first or last, none doubled.
pub fn tidy(entries: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();
    for e in entries {
        if e == "---" && out.last().is_none_or(|l| *l == "---") {
            continue;
        }
        out.push(e);
    }
    while out.last().is_some_and(|l| *l == "---") {
        out.pop();
    }
    out
}

pub fn pruned(entries: &[Value]) -> Vec<Value> {
    tidy(
        entries
            .iter()
            .filter_map(|e| {
                if e == "---" {
                    return Some(e.clone());
                }
                if let Some(text) = e.as_str() {
                    return match kept(text) {
                        Kept::No => None,
                        _ => Some(e.clone()),
                    };
                }
                let label = e["menu"].as_str().unwrap();
                let only = match kept(label) {
                    Kept::No => return None,
                    Kept::All => None,
                    Kept::Only(only) => Some(only),
                };
                let inner: Vec<Value> = e["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|x| {
                        // (a pattern ending "(" takes labels it starts)
                        let label = x.as_str().or_else(|| x["menu"].as_str()).unwrap_or("");
                        *x == "---"
                            || only.is_none_or(|o| {
                                o.iter().any(|p| {
                                    label == *p || (p.ends_with('(') && label.starts_with(p))
                                })
                            })
                    })
                    .map(|x| match x["menu"].as_str() {
                        // (less the distance chooser)
                        Some(sub @ "similar files in a new page") => {
                            let inner: Vec<Value> = x["entries"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .filter(|y| *y != "custom")
                                .cloned()
                                .collect();
                            json!({ "menu": sub, "entries": tidy(inner) })
                        }
                        _ => x.clone(),
                    })
                    .collect();
                // (a menu left with nothing hydrus-rs has isn't shown)
                let inner = tidy(inner);
                (only.is_none() || !inner.is_empty())
                    .then(|| json!({ "menu": label, "entries": inner }))
            })
            .collect(),
    )
}

/// The menu's texts as shown: Qt's labels double an `&`.
pub fn unescaped(entries: &[Value]) -> Vec<Value> {
    entries
        .iter()
        .map(|e| match e {
            Value::String(text) => json!(text.replace("&&", "&")),
            // (a slider, as hydrus-rs shows one in a menu: as its value)
            _ if e.get("slider").is_some() => {
                json!(format!("{}: {}", e["slider"].as_str().unwrap(), e["value"]))
            }
            _ if e.get("check").is_some() => json!({
                "check": e["check"].as_str().unwrap().replace("&&", "&"),
                "checked": e["checked"],
            }),
            _ => json!({
                "menu": e["menu"].as_str().unwrap().replace("&&", "&"),
                "entries": unescaped(e["entries"].as_array().unwrap()),
            }),
        })
        .collect()
}

pub fn described(entries: &[Entry]) -> Vec<Value> {
    tidy(
        entries
            .iter()
            .map(|e| match e {
                Entry::Separator => json!("---"),
                Entry::Item(label, _) | Entry::Label(label) => json!(label),
                Entry::Check(label, _, checked) => json!({ "check": label, "checked": checked }),
                Entry::Menu(label, inner) => json!({ "menu": label, "entries": described(inner) }),
            })
            .collect(),
    )
}

/// Our menu as the recordings have it: they were made without hydrus's
/// advanced mode, which its "in file browser" needs, and hydrus-rs offers
/// that always. Each must follow its "in web browser", as hydrus puts it
/// (less Windows' "in another program", which hydrus-rs doesn't have).
pub fn as_recorded(entries: &[Value]) -> Vec<Value> {
    for pair in entries.windows(2) {
        if let Some(label) = pair[1].as_str().filter(|l| l.ends_with("in file browser")) {
            let web = label.replace("in file browser", "in web browser");
            assert_eq!(pair[0], json!(web), "{label} follows {web}");
        }
    }
    assert!(
        entries
            .first()
            .and_then(Value::as_str)
            .is_none_or(|l| !l.ends_with("in file browser")),
        "in file browser first"
    );
    tidy(
        entries
            .iter()
            .filter(|e| !e.as_str().is_some_and(|l| l.ends_with("in file browser")))
            .map(|e| match e.get("entries") {
                Some(inner) => json!({
                    "menu": e["menu"],
                    "entries": as_recorded(inner.as_array().unwrap()),
                }),
                None => e.clone(),
            })
            .collect(),
    )
}
