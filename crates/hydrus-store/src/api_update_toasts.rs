//! Saved Client API cookie/header notifications and actual finished popup jobs.
use crate::{
    Result, StoreError, popups,
    settings::{self, Setting},
};
use hydrus_legacy::{
    objects::ClientOptions,
    serialisable::{SerialisableObject, SerialisableType},
};
use rusqlite::Connection;
use std::collections::BTreeSet;

/// The reference checkbox is unchecked by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub enabled: bool,
}
impl Setting for Preferences {
    const KEY: &'static str = "api_update_toasts";
}
impl Preferences {
    pub fn from_legacy(options: &ClientOptions) -> Self {
        Self {
            enabled: options
                .booleans
                .get("notify_client_api_cookies")
                .copied()
                .unwrap_or(false),
        }
    }
}
/// Native preferences win, including when an imported legacy singleton remains.
pub fn load(conn: &Connection) -> Result<Preferences> {
    let native: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM settings WHERE key=?)",
        [Preferences::KEY],
        |r| r.get(0),
    )?;
    if native {
        return settings::get(conn);
    }
    let kind = SerialisableType::CLIENT_OPTIONS;
    let Some((version, info)) = crate::legacy::singleton(conn, u32::from(kind.0))? else {
        return Ok(Preferences::default());
    };
    let object = SerialisableObject::from_stored(kind, None, version, &info)
        .map_err(|e| StoreError::Corrupt(e.to_string()))?;
    let options =
        ClientOptions::from_object(&object).map_err(|e| StoreError::Corrupt(e.to_string()))?;
    Ok(Preferences::from_legacy(&options))
}
/// Successful request categories, deduplicated and sorted exactly as Qt.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changes {
    pub cleared: BTreeSet<String>,
    pub set: BTreeSet<String>,
    pub altered: BTreeSet<String>,
}
/// Cookie domains describe accepted set/clear operations, including same values.
pub fn cookies_message(changes: &Changes) -> Option<String> {
    if changes.cleared.is_empty() && changes.set.is_empty() {
        return None;
    }
    let mut message = "Cookies sent from API:".to_owned();
    if !changes.cleared.is_empty() {
        message.push_str(&format!(
            " ({} cleared)",
            changes
                .cleared
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !changes.set.is_empty() {
        message.push_str(&format!(
            " ({} set)",
            changes.set.iter().cloned().collect::<Vec<_>>().join(", ")
        ));
    }
    Some(message)
}
/// Qt's altered-detail guard intentionally depends on nonempty newly set headers.
pub fn headers_message(changes: &Changes) -> Option<String> {
    if changes.cleared.is_empty() && changes.set.is_empty() && changes.altered.is_empty() {
        return None;
    }
    let mut lines = vec!["Headers sent from API:".to_owned()];
    lines.extend(changes.cleared.iter().map(|key| format!("Cleared: {key}")));
    lines.extend(changes.set.iter().map(|key| format!("Set: {key}")));
    if !changes.set.is_empty() {
        lines.extend(changes.altered.iter().map(|key| format!("Altered: {key}")));
    }
    Some(lines.join("\n"))
}
/// Publish within the successful API writer transaction, never on error/no-op.
/// The existing owned toaster displays this finished job and its five-second expiry.
pub fn publish(conn: &Connection, message: Option<String>, now: i64) -> Result<Option<[u8; 32]>> {
    let Some(message) = message else {
        return Ok(None);
    };
    if !load(conn)?.enabled {
        return Ok(None);
    }
    let mut job = popups::Job::text(message, now as f64);
    job.finish_and_dismiss(Some(5), now);
    let key = job.key;
    popups::add(conn, &job, now)?;
    Ok(Some(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retained_enabled_options_native_override_reopen_corruption_and_exact_expiry() {
        let fixture = hydrus_testkit::fixture_json("api_update_toasts.json");
        let tuple = &fixture["legacy_options"];
        let directory = tempfile::tempdir().unwrap();
        let store = crate::Store::open(directory.path()).unwrap();
        assert!(!store.read(load).unwrap().enabled);
        let version = i64::try_from(tuple[1].as_u64().unwrap()).unwrap();
        let info = tuple[2].to_string();
        store.write(move |ctx| {ctx.conn().execute("INSERT INTO legacy_objects(source,type_id,name,version,timestamp_ms,dump) VALUES('json_dumps',?,'',?,0,?)",rusqlite::params![u32::from(SerialisableType::CLIENT_OPTIONS.0),version,info])?;Ok(())}).unwrap();
        assert!(store.read(load).unwrap().enabled);
        let key = store
            .write(|ctx| publish(ctx.conn(), Some("actual API message".into()), 100))
            .unwrap()
            .unwrap();
        for case in fixture["dismissal"].as_array().unwrap() {
            let now = 100 + case["seconds"].as_i64().unwrap();
            let job = store
                .read(move |conn| popups::get(conn, &key, now))
                .unwrap();
            assert_eq!(job.is_none(), case["dismissed"].as_bool().unwrap());
            if let Some(job) = job {
                assert!(job.done);
                assert!(!job.pausable && !job.cancellable);
                assert_eq!(job.dismiss_at, Some(105));
            }
        }
        store
            .write(|ctx| settings::set(ctx.conn(), &Preferences { enabled: false }))
            .unwrap();
        let reopened = crate::Store::open(directory.path()).unwrap();
        assert!(!reopened.read(load).unwrap().enabled);
        assert!(
            reopened
                .write(|ctx| publish(ctx.conn(), Some("disabled".into()), 200))
                .unwrap()
                .is_none()
        );
        reopened
            .write(|ctx| {
                ctx.conn().execute(
                    "UPDATE settings SET value='{\"enabled\":\"invalid\"}' WHERE key=?",
                    [Preferences::KEY],
                )?;
                Ok(())
            })
            .unwrap();
        assert!(reopened.read(load).is_err());
    }
}
