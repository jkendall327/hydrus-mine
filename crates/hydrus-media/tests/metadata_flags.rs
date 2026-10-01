//! The XMP, IPTC and software/source flags against the reference
//! (`oracle/dump_metadata_flags.py`): the media corpus, and files carrying
//! those metadata in the places Pillow finds them (and near misses).

use hydrus_media::MediaTools;

#[test]
fn flags_match_the_reference() {
    let recorded = hydrus_testkit::fixture_json("metadata_flags.json");
    let tools = MediaTools::new();
    let mut problems = Vec::new();
    let mut checked = 0;
    for rec in recorded["files"].as_array().unwrap() {
        if rec.get("mime_error").is_some() {
            continue;
        }
        let name = rec["file"].as_str().unwrap();
        let path = hydrus_testkit::fixture_path(name);
        let Ok(info) = tools.inspect(&path) else {
            continue;
        };
        let flags = tools.flags(&path, &info);
        let ours = [flags.has_xmp, flags.has_iptc, flags.has_software_source];
        let theirs =
            ["has_xmp", "has_iptc", "has_software_source"].map(|k| rec[k].as_bool().unwrap());
        if ours != theirs {
            problems.push(format!(
                "{name}: ours {ours:?}, theirs {theirs:?} (xmp, iptc, software)"
            ));
        }
        checked += 1;
    }
    assert!(checked > 200, "{checked}");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
