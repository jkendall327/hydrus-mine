//! "Allow loading of truncated images" (`enable_truncated_images_pil`, which
//! sets Pillow's `LOAD_TRUNCATED_IMAGES`): the reference's recorded decoding
//! of cut-off JPEG, PNG, GIF, WebP and BMP files with the option on and off
//! (oracle/record_image_decoder_policies.py), replayed through thumbnail
//! generation and image loading.
use hydrus_media::{MediaTools, ThumbnailSpec};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

fn cut_off(name: &str) -> bool {
    ["short", "half", "big-cut", "raw-cut", "no-eoi"]
        .iter()
        .any(|part| name.contains(part))
}

fn pixels(row: &serde_json::Value) -> Vec<u8> {
    serde_json::from_value(row["pixels"].clone()).unwrap()
}

// leaf: audit-options-media-playback-system-allow-loading-of-truncated-images
#[test]
fn cut_off_images_load_or_are_refused_as_the_reference_decodes_them() {
    let recording = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let policy = Arc::new(AtomicBool::new(true));
    let tools = MediaTools::new().with_truncated_reader(Arc::new({
        let policy = policy.clone();
        move || policy.load(Ordering::Acquire)
    }));
    let mut loaded_cut_off = 0;
    let mut refused_cut_off = 0;
    for case in recording["cases"].as_array().unwrap() {
        let enabled = case["truncated"].as_bool().unwrap();
        policy.store(enabled, Ordering::Release);
        for row in case["decode"].as_array().unwrap() {
            let name = row["file"].as_str().unwrap();
            if !cut_off(name) && !name.contains("whole") {
                continue;
            }
            let path = hydrus_testkit::fixture_path(format!("image_decoder_policies/{name}"));
            let mime = tools.detect_mime(&path).unwrap();
            let loaded = tools.load_image(&path, mime);
            let analysed = tools.analyse(&path, &ThumbnailSpec::default());
            let what = format!("{name}, truncated images allowed: {enabled}");
            if row["ok"].as_bool().unwrap() {
                assert_eq!(loaded.unwrap().data(), pixels(row), "{what}");
                let thumbnail = analysed.unwrap().thumbnail.unwrap();
                assert_eq!(thumbnail.pixels.data(), pixels(row), "{what}");
                loaded_cut_off += usize::from(cut_off(name));
            } else {
                assert!(loaded.is_err(), "{what}");
                // (a thumbnail from another decoder is not what is asked
                // here: the import's pixel hash is from the decode above)
                let thumbnail_from_the_decode = match &analysed {
                    Ok(analysis) => analysis.pixel_hash.is_some(),
                    Err(_) => false,
                };
                assert!(!thumbnail_from_the_decode, "{what}");
                refused_cut_off += usize::from(cut_off(name));
            }
        }
    }
    // (the recording has loads and refusals of cut-off files in both
    // settings, so neither half of the test is empty)
    assert!(
        loaded_cut_off >= 18 && refused_cut_off >= 20,
        "{loaded_cut_off} {refused_cut_off}"
    );
}
