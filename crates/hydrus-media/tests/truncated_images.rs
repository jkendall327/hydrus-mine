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

/// A header that claims a huge picture, over a few bytes of data.
fn bmp_claiming(width: u32, height: u32) -> Vec<u8> {
    let mut bmp = b"BM".to_vec();
    bmp.extend_from_slice(&64u32.to_le_bytes());
    bmp.extend_from_slice(&[0; 4]);
    bmp.extend_from_slice(&54u32.to_le_bytes());
    bmp.extend_from_slice(&40u32.to_le_bytes());
    bmp.extend_from_slice(&width.to_le_bytes());
    bmp.extend_from_slice(&height.to_le_bytes());
    bmp.extend_from_slice(&1u16.to_le_bytes());
    bmp.extend_from_slice(&32u16.to_le_bytes());
    bmp.extend_from_slice(&[0; 24]);
    bmp.extend_from_slice(&[7; 10]);
    bmp
}

fn png_claiming(width: u32, height: u32) -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    for (kind, data) in [(b"IHDR", ihdr), (b"IDAT", vec![0x78, 0x9c, 3, 0])] {
        png.extend_from_slice(&u32::try_from(data.len()).unwrap().to_be_bytes());
        png.extend_from_slice(kind);
        png.extend_from_slice(&data);
        png.extend_from_slice(&[0; 4]);
    }
    png
}

// A cut-off picture is filled out to its declared size, so a few bytes that
// claim a huge size must be refused as Pillow's decompression bomb limit
// refuses them, before anything is allocated for them.
#[test]
fn a_tiny_file_claiming_a_huge_picture_is_refused_with_no_allocation() {
    for (name, bytes) in [
        ("bmp", bmp_claiming(20_000, 20_000)),
        ("png", png_claiming(20_000, 20_000)),
    ] {
        for allowed in [true, false] {
            let tools = MediaTools::new().with_truncated_images(allowed);
            let path = std::env::temp_dir().join(format!("claim-{name}-{allowed}.{name}"));
            std::fs::write(&path, &bytes).unwrap();
            let started = std::time::Instant::now();
            let mime = tools.detect_mime(&path);
            let loaded = mime.and_then(|mime| tools.load_image(&path, mime));
            let _ = std::fs::remove_file(&path);
            let error = loaded.expect_err(name).to_string();
            assert!(
                error.contains("decompression bomb") || error.contains("truncated"),
                "{name}: {error}"
            );
            assert!(
                started.elapsed() < std::time::Duration::from_secs(5),
                "{name}"
            );
            let direct = hydrus_media::decode_image_with_policy(&bytes, true, allowed);
            let error = direct.expect_err(name).to_string();
            assert!(error.contains("decompression bomb"), "{name}: {error}");
        }
    }
}
