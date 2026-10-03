//! What the "show detailed embedded file metadata" window shows, against
//! the reference (`oracle/dump_embedded_metadata.py`, over the media corpus
//! and the metadata samples): EXIF rows, XMP, IPTC, human-readable text and
//! extra rows. HEIF, AVIF and JPEG XL files, which hydrus-media doesn't
//! open as Pillow's plugins do, are left out.

use hydrus_core::Mime;
use hydrus_media::embedded_metadata;
use serde_json::{Value, json};

#[test]
fn the_window_shows_what_the_reference_shows() {
    let recorded = hydrus_testkit::fixture_json("embedded_metadata.json");
    let mut problems = Vec::new();
    let mut checked = 0;
    for (name, theirs) in recorded.as_object().unwrap() {
        let mime_name = theirs["mime"].as_str().unwrap();
        let mime = Mime::ALL
            .iter()
            .copied()
            .find(|m| m.human_name() == mime_name)
            .unwrap();
        if matches!(
            mime,
            Mime::ImageHeic
                | Mime::ImageHeif
                | Mime::ImageHeicSequence
                | Mime::ImageHeifSequence
                | Mime::ImageAvif
                | Mime::ImageAvifSequence
                | Mime::ImageJxl
                | Mime::AnimationJxl
        ) {
            continue;
        }
        let data = std::fs::read(hydrus_testkit::fixture_path(name)).unwrap();
        let ours = embedded_metadata(&data, mime, theirs["has_icc"].as_bool().unwrap());
        let ours = json!({
            "exif": ours.exif,
            "xmp": ours.xmp,
            "iptc": ours.iptc,
            "text": ours.text,
            "extra": ours.extra.iter().map(|(k, v)| json!([k, v])).collect::<Vec<Value>>(),
        });
        for key in ["exif", "xmp", "iptc", "text", "extra"] {
            if ours[key] != theirs[key] {
                problems.push(format!(
                    "{name} {key}:\n  ours   {}\n  theirs {}",
                    ours[key], theirs[key]
                ));
            }
        }
        checked += 1;
    }
    assert!(checked > 100, "{checked}");
    assert!(
        problems.is_empty(),
        "{}\n{} problems",
        problems.join("\n"),
        problems.len()
    );
}
