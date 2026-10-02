//! What the folder tests share: the popups shown, as the recorders
//! (`oracle/hydrus_driver.py`, `popups_sent`) describe the reference's.

use hydrus_core::Sha256;
use hydrus_store::Store;

/// The popups showing, described as the recording describes them.
pub fn popups_shown(store: &Store) -> Vec<serde_json::Value> {
    let now = hydrus_core::time::TimestampMs::now().millis() / 1000;
    store
        .read(|conn| hydrus_store::popups::all(conn, now))
        .unwrap()
        .iter()
        .map(|job| {
            serde_json::json!({
                "title": job.status_title,
                "text_1": job.status_text_1,
                "text_2": job.status_text_2,
                "files": job.files.as_ref().map(|(hashes, label)| {
                    let mut hex: Vec<String> = hashes.iter().map(Sha256::to_hex).collect();
                    hex.sort();
                    serde_json::json!([hex, label])
                }),
                "traceback": job.traceback.is_some(),
                "had_error": job.had_error,
                "done": job.done,
                "dismissed": false,
            })
        })
        .collect()
}

/// The recorded popups still showing after the work (a working popup is
/// dismissed when it ends), with the recorded folder's path made ours.
pub fn recorded_shown(
    popups: &serde_json::Value,
    folder: &str,
    ours: &str,
) -> Vec<serde_json::Value> {
    popups
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["dismissed"] == false)
        .map(|p| {
            let mut p = p.clone();
            if let Some(text) = p["text_1"].as_str() {
                p["text_1"] = text.replace(folder, ours).into();
            }
            p
        })
        .collect()
}
