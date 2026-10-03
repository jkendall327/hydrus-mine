//! The visual-duplicates computations against the reference's
//! (`oracle/fixtures/visual_data.json`, made by
//! `oracle/dump_visual_data.py`): OpenCV's building blocks stage by stage,
//! each image's summaries, and the verdicts on pairs of images.

use std::path::PathBuf;

use base64::Engine as _;
use hydrus_media::resample::{self, Interpolation};
use hydrus_media::visual::{self, VisualData, VisualDataTiled};
use hydrus_media::{MediaTools, Raster};
use serde_json::Value;
use sha2::{Digest, Sha256};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/fixtures")
}

fn fixture() -> Value {
    let text = std::fs::read_to_string(fixtures().join("visual_data.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn sha_f32(values: &[f32]) -> String {
    let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    sha(&bytes)
}

fn floats(b64: &Value) -> Vec<f32> {
    base64::engine::general_purpose::STANDARD
        .decode(b64.as_str().unwrap())
        .unwrap()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

/// The oracle's `random_image`: a gradient with hashed noise.
fn test_image(seed: u64, w: usize, h: usize, cn: usize) -> Vec<u8> {
    let mask = 0xffff_ffffu64;
    let mut out = vec![0u8; w * h * cn];
    for y in 0..h as u64 {
        for x in 0..w as u64 {
            for c in 0..cn as u64 {
                let mut v = ((x.wrapping_mul(73_856_093))
                    ^ (y.wrapping_mul(19_349_663))
                    ^ ((c + 1) * 83_492_791)
                    ^ seed.wrapping_mul(2_654_435_761))
                    & mask;
                v = v.wrapping_mul(2_246_822_519) & mask;
                let noise = (v >> 24) & 255;
                let smooth = u64::midpoint(
                    x * 255 / (w as u64 - 1).max(1),
                    y * 255 / (h as u64 - 1).max(1),
                );
                out[((y as usize * w) + x as usize) * cn + c as usize] =
                    ((smooth * 3 + noise) / 4) as u8;
            }
        }
    }
    out
}

fn dims(case: &Value) -> (u64, usize, usize) {
    (
        case["seed"].as_u64().unwrap(),
        case["w"].as_u64().unwrap() as usize,
        case["h"].as_u64().unwrap() as usize,
    )
}

#[test]
fn gaussian_blur_of_8_bit_images_is_exact() {
    let f = fixture();
    for case in f["stages"]["blur_u8"].as_array().unwrap() {
        let (seed, w, h) = dims(case);
        let image = test_image(seed, w, h, 1);
        let blurred = resample::gaussian_blur_u8(&image, w, h, case["sigma"].as_f64().unwrap());
        assert_eq!(sha(&blurred), case["sha"], "{w}x{h}");
    }
}

#[test]
fn rgb_to_lab_is_exact_for_every_colour() {
    let f = fixture();
    let rgb: Vec<u8> = (0u32..1 << 24)
        .flat_map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .collect();
    assert_eq!(
        sha(&resample::rgb_to_lab(&rgb)),
        f["stages"]["lab_cube_sha"]
    );
}

#[test]
fn area_resizes_of_8_bit_images_are_exact() {
    let f = fixture();
    for case in f["stages"]["resize_u8"].as_array().unwrap() {
        let (seed, w, h) = dims(case);
        let cn = case["cn"].as_u64().unwrap() as usize;
        let image = Raster::new(w as u32, h as u32, cn as u8, test_image(seed, w, h, cn)).unwrap();
        let to = case["to"].as_array().unwrap();
        let (tw, th) = (
            to[0].as_u64().unwrap() as u32,
            to[1].as_u64().unwrap() as u32,
        );
        let resized = resample::resize(&image, tw, th, Interpolation::Area);
        assert_eq!(
            sha(resized.data()),
            case["sha"],
            "{w}x{h}x{cn} to {tw}x{th}"
        );
    }
}

/// Float stages, bit for bit: the edge map's blur and resize.
#[test]
fn float_blur_and_resize_are_exact() {
    let f = fixture();
    for case in f["stages"]["float"].as_array().unwrap() {
        let (seed, w, h) = dims(case);
        let image: Vec<f32> = test_image(seed, w, h, 3)
            .iter()
            .map(|&v| f32::from(v))
            .collect();
        let mut blurred = vec![0f32; image.len()];
        let mut dog = vec![0f32; image.len()];
        for c in 0..3 {
            let plane: Vec<f32> = image.iter().skip(c).step_by(3).copied().collect();
            let b = resample::gaussian_blur_f32(&plane, w, h, 10.0);
            for (i, v) in b.iter().enumerate() {
                blurred[i * 3 + c] = *v;
                dog[i * 3 + c] = plane[i] - v;
            }
        }
        let sample: Vec<f32> = blurred.iter().step_by(37).copied().collect();
        assert_eq!(sample, floats(&case["blur_sample"]), "blur {w}x{h}");
        assert_eq!(sha_f32(&blurred), case["blur_sha"], "blur {w}x{h}");
        let resized = resample::resize_area_f32(&dog, w, h, 3, 256, 256);
        let sample: Vec<f32> = resized.iter().step_by(97).copied().collect();
        assert_eq!(sample, floats(&case["resized_sample"]), "resize {w}x{h}");
        assert_eq!(sha_f32(&resized), case["resized_sha"], "resize {w}x{h}");
    }
}

fn load(tools: &MediaTools, name: &str) -> Raster {
    let path = fixtures().join(name);
    let mime = tools.detect_mime(&path).unwrap();
    tools.load_image(&path, mime).unwrap()
}

#[test]
fn image_summaries_and_verdicts_match() {
    let f = fixture();
    let tools = MediaTools::new();
    let mut summaries: std::collections::BTreeMap<String, (VisualData, VisualDataTiled)> =
        std::collections::BTreeMap::new();
    let mut report = String::new();
    for (name, expected) in f["files"].as_object().unwrap() {
        let image = load(&tools, name);
        let v = visual::visual_data(&image);
        let t = visual::visual_data_tiled(&image);
        let lab = expected["lab"].as_array().unwrap();
        for (ours, theirs) in [&v.lab.l, &v.lab.a, &v.lab.b].into_iter().zip(lab) {
            if *ours != floats(theirs) {
                report.push_str(&format!("{name}: Lab histograms differ\n"));
            }
        }
        for (c, (ours, theirs)) in t
            .edges
            .iter()
            .zip(expected["edge_sha"].as_array().unwrap())
            .enumerate()
        {
            if sha_f32(ours) != *theirs {
                report.push_str(&format!("{name}: edge map channel {c} differs\n"));
            }
        }
        let tiles: Vec<f32> = t
            .tiles
            .iter()
            .flat_map(|h| h.l.iter().chain(&h.a).chain(&h.b).copied())
            .collect();
        if sha_f32(&tiles) != expected["tile_hist_sha"] {
            report.push_str(&format!("{name}: tile histograms differ\n"));
        }
        summaries.insert(name.clone(), (v, t));
    }
    let mut verdicts = [0usize; 2];
    for pair in f["pairs"].as_array().unwrap() {
        let (a, b) = (pair["a"].as_str().unwrap(), pair["b"].as_str().unwrap());
        let (va, ta) = &summaries[a];
        let (vb, tb) = &summaries[b];
        let verdict = |v: visual::Verdict| (v.similar, v.result, v.statement.to_owned());
        let recorded = |v: &Value| {
            (
                v[0].as_bool().unwrap(),
                v[1].as_u64().unwrap() as u8,
                v[2].as_str().unwrap().to_owned(),
            )
        };
        let simple = verdict(visual::similar_simple(va, vb));
        let expected_simple = recorded(&pair["simple"]);
        if simple != expected_simple {
            report.push_str(&format!(
                "{a} / {b}: simple {simple:?}, reference {expected_simple:?}\n"
            ));
        }
        if let Some(regional) = pair.get("regional") {
            let ours = verdict(visual::similar_regional(ta, tb));
            let theirs = recorded(regional);
            verdicts[usize::from(ours == theirs)] += 1;
            if ours != theirs {
                let (largest, pull) = visual::edge_map_raw(ta, tb);
                report.push_str(&format!(
                    "{a} / {b}: regional {ours:?}, reference {theirs:?} (edge {largest:.4}/{pull:.4}, reference {}; tiles {:?}, reference {})\n",
                    pair["edge"], visual::tile_stats(ta, tb), pair["lab"]
                ));
            }
        }
    }
    println!(
        "regional verdicts: {} differ, {} agree",
        verdicts[0], verdicts[1]
    );
    assert!(report.is_empty(), "{report}");
}

#[test]
fn jpeg_quality_matches() {
    use hydrus_media::jpeg::{Subsampling, jpeg_quality};
    let f = fixture();
    for (name, expected) in f["jpegs"].as_object().unwrap() {
        let data = std::fs::read(fixtures().join("visual_jpegs").join(name)).unwrap();
        let q = jpeg_quality(&data);
        let code = match q.subsampling {
            Subsampling::S444 => 0,
            Subsampling::S422 => 1,
            Subsampling::S420 => 2,
            Subsampling::Unknown => 3,
            Subsampling::Greyscale => 4,
        };
        assert_eq!(code, expected["subsampling"].as_u64().unwrap(), "{name}");
        assert_eq!(
            q.progressive,
            expected["progressive"].as_bool().unwrap(),
            "{name}"
        );
        let theirs = expected["quality"].as_f64().unwrap();
        let ours = q.quality.unwrap();
        assert!(
            (ours - theirs).abs() <= 1e-9 * theirs,
            "{name}: {ours} vs {theirs}"
        );
    }
}
