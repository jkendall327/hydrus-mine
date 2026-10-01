//! The client's file handling options against the reference
//! (`oracle/dump_file_handling.py`): what counts as transparency at each
//! strictness, and zips with comic book detection on and off. They hold
//! for the whole process, so they are tested alone here.

use hydrus_media::{
    MediaTools, TransparencyStrictness, set_comic_book_detection, set_transparency_strictness,
};

#[test]
fn file_handling_options_match_the_reference() {
    let recorded = hydrus_testkit::fixture_json("file_handling.json");
    let tools = MediaTools::new();
    let mut problems = Vec::new();

    let levels = [
        TransparencyStrictness::ChannelPresence,
        TransparencyStrictness::NotBlackOrWhite,
        TransparencyStrictness::Human,
    ];
    let records = recorded["transparency"].as_array().unwrap();
    assert!(records.len() > 20, "{}", records.len());
    // (the reference_parity test's known differences: no alpha planes from
    // ffmpeg 6's AVIF decoder, and no HEIF decoder; and files ffmpeg decodes
    // can differ on other ffmpeg builds)
    let recording_ffmpeg = hydrus_testkit::recording_ffmpeg();
    let records: Vec<_> = records
        .iter()
        .filter(|rec| {
            let name = rec["file"].as_str().unwrap();
            let by_ffmpeg = ["media/apng_", "media/avif_", "media/heic_"]
                .iter()
                .any(|p| name.starts_with(p));
            !["media/avif_alpha.avif", "media/heic_alpha.heic"].contains(&name)
                && (recording_ffmpeg || !by_ffmpeg)
        })
        .collect();
    for (i, level) in levels.into_iter().enumerate() {
        set_transparency_strictness(level);
        for rec in &records {
            let name = rec["file"].as_str().unwrap();
            let path = hydrus_testkit::fixture_path(name);
            let theirs = rec["has_transparency_by_strictness"][i].as_bool().unwrap();
            let ours = tools
                .inspect(&path)
                .is_ok_and(|info| tools.flags(&path, &info).has_transparency);
            if ours != theirs {
                problems.push(format!("{name} at {level:?}: ours {ours}, theirs {theirs}"));
            }
        }
    }
    set_transparency_strictness(TransparencyStrictness::Human);

    for detection in [false, true] {
        set_comic_book_detection(detection);
        for rec in recorded["zips"].as_array().unwrap() {
            let name = rec["file"].as_str().unwrap();
            let path = hydrus_testkit::fixture_path(name);
            let key = if detection {
                "mime"
            } else {
                "mime_without_detection"
            };
            let theirs = rec[key].as_u64().unwrap();
            let ours = tools.inspect(&path).map(|info| u64::from(info.mime.code()));
            if ours.as_ref().ok() != Some(&theirs) {
                problems.push(format!(
                    "{name} with detection {detection}: ours {ours:?}, theirs {theirs}"
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
