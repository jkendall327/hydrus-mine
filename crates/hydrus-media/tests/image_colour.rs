//! Actual PIL pixel behavior for the finite embedded ICC switch.
use hydrus_core::Mime;
use hydrus_media::{MediaTools, ThumbnailSpec, animation::Frames};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
fn pixels(row: &serde_json::Value) -> Vec<u8> {
    serde_json::from_value(row["pixels"].clone()).unwrap()
}
#[test]
fn embedded_policy_preserves_gamma_fallback_and_real_png_jpeg_webp_pixels() {
    let recording = hydrus_testkit::fixture_json("image_decoder_policies.json");
    let policy = Arc::new(AtomicBool::new(true));
    let tools = MediaTools::new().with_icc_reader(Arc::new({
        let policy = policy.clone();
        move || policy.load(Ordering::Acquire)
    }));
    for case in recording["cases"].as_array().unwrap() {
        let enabled = case["icc"].as_bool().unwrap();
        policy.store(enabled, Ordering::Release);
        for row in case["decode"].as_array().unwrap() {
            let name = row["file"].as_str().unwrap();
            if name.contains("short") || name.contains("no-eoi") {
                continue;
            }
            let path = hydrus_testkit::fixture_path(format!("image_decoder_policies/{name}"));
            let data = std::fs::read(&path).unwrap();
            let decoded = hydrus_media::decode_image_with_icc(&data, enabled).unwrap();
            assert_eq!(decoded.data(), pixels(row), "{name}, ICC={enabled}");
            if enabled {
                assert_eq!(hydrus_media::decode_image(&data).unwrap(), decoded);
            }
            let mime = tools.detect_mime(&path).unwrap();
            assert_eq!(tools.load_image(&path, mime).unwrap(), decoded);
            let analysis = tools.analyse(&path, &ThumbnailSpec::default()).unwrap();
            assert_eq!(analysis.thumbnail.unwrap().pixels.data(), pixels(row));
            if name == "embedded-linear.png" {
                assert_eq!(analysis.pixel_hash.unwrap().to_hex(), row["pixel_sha256"]);
                assert!(
                    analysis.flags.has_icc_profile,
                    "ignoring a profile does not erase metadata"
                );
            }
        }
        assert!(!case["thumbnail"]["embedded_icc"].as_bool().unwrap());
    }
}
#[test]
fn native_owned_animation_policy_changes_future_frames_without_resetting_position() {
    let recording = hydrus_testkit::fixture_json("image_decoder_policies.json");
    for (name, mime) in [
        ("embedded-animation.webp", Mime::AnimationWebp),
        ("embedded-ugoira.zip", Mime::AnimationUgoira),
    ] {
        let path = hydrus_testkit::fixture_path(format!("image_decoder_policies/{name}"));
        let policy = Arc::new(AtomicBool::new(false));
        let mut frames = Frames::open(&path, mime, &[], Some(2))
            .unwrap()
            .with_icc_reader(Arc::new({
                let policy = policy.clone();
                move || policy.load(Ordering::Acquire)
            }));
        let off = &recording["cases"][1]["frames"];
        let on = &recording["cases"][0]["frames"];
        let (first, duration) = frames.next_frame().unwrap();
        assert_eq!(first.data(), pixels(&off[0]), "{name}");
        assert_eq!(duration, 100);
        policy.store(true, Ordering::Release);
        let (second, duration) = frames.next_frame().unwrap();
        assert_eq!(
            second.data(),
            pixels(&on[1]),
            "policy change must retain next frame index: {name}"
        );
        assert_eq!(duration, 150);
        policy.store(false, Ordering::Release);
        assert_eq!(frames.next_frame().unwrap().0.data(), pixels(&off[0]));
        assert_eq!(
            frames.next_frame().unwrap().0.data(),
            pixels(&off[1]),
            "opaque second-frame pixels remain exact with ICC disabled: {name}"
        );
        assert_eq!(frames.durations(), [100, 150]);
    }
}

#[test]
fn alpha_bearing_animation_retains_existing_blend_disposal_pixels_and_durations() {
    use std::io::Write;
    let path = hydrus_testkit::fixture_path("media/webp_anim_alpha.webp");
    let mut data = std::fs::read(path).unwrap();
    let mut offset = 12;
    let mut frames_seen = 0;
    while offset + 8 <= data.len() {
        let size = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if &data[offset..offset + 4] == b"ANMF" {
            // First frame disposal and second frame blending are real decoder consumers.
            data[offset + 8 + 15] = if frames_seen == 0 { 3 } else { 0 };
            frames_seen += 1;
        }
        offset += 8 + size + size % 2;
    }
    assert_eq!(frames_seen, 2);
    let mut file = tempfile::NamedTempFile::new().unwrap();
    file.write_all(&data).unwrap();
    let mut original = image_webp::WebPDecoder::new(std::io::Cursor::new(&data)).unwrap();
    assert!(original.has_alpha());
    let mut frames =
        Frames::open_with_icc(file.path(), Mime::AnimationWebp, &[], None, false).unwrap();
    let mut expected = vec![0; original.output_buffer_size().unwrap()];
    for _ in 0..original.num_frames() {
        let duration = original.read_frame(&mut expected).unwrap();
        let (actual, actual_duration) = frames.next_frame().unwrap();
        assert_eq!(
            actual.data(),
            expected,
            "alpha-bearing blend/disposal pixels are unchanged"
        );
        assert_eq!(actual_duration, if duration == 0 { 83 } else { duration });
    }
}
