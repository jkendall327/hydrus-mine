//! Parity with the reference implementation on the oracle corpus.
//!
//! `oracle/dump_media.py` generated `oracle/fixtures/media/*` and recorded
//! what the reference client computes for each file in
//! `oracle/fixtures/media.json`. These tests run the same files through this
//! crate and compare. Needs `ffmpeg` on PATH (the same build the fixtures
//! were recorded with, for the ffmpeg-derived values).
//!
//! Everything must match exactly except the differences listed in
//! [`KNOWN_DIFFERENCES`], each with its reason; see the crate README.

use std::collections::BTreeMap;
use std::path::PathBuf;

use hydrus_core::Mime;
use hydrus_media::{MediaError, MediaTools, ThumbnailFormat, ThumbnailScale, ThumbnailSpec};
use rayon::prelude::*;
use serde_json::Value;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../oracle/fixtures")
}

fn fixture() -> Value {
    let text = std::fs::read_to_string(fixtures().join("media.json")).expect("media.json exists");
    serde_json::from_str(&text).expect("valid json")
}

fn mime(v: &Value) -> Mime {
    Mime::from_code(u8::try_from(v.as_u64().expect("mime code")).expect("small"))
        .expect("known mime")
}

fn error_name(e: &MediaError) -> &'static str {
    match e {
        MediaError::ZeroSize => "ZeroSizeFileException",
        MediaError::Unsupported { .. } => "UnsupportedFileException",
        MediaError::Damaged(_) => "DamagedOrUnusualFileException",
        _ => "other",
    }
}

/// Differences found for one file, by category.
#[derive(Default, Debug)]
struct Report {
    /// category -> (file, detail)
    mismatches: BTreeMap<&'static str, Vec<(String, String)>>,
    counts: BTreeMap<&'static str, (usize, usize)>,
    phash_distances: Vec<(String, u32)>,
}

impl Report {
    fn check(
        &mut self,
        category: &'static str,
        file: &str,
        ok: bool,
        detail: impl FnOnce() -> String,
    ) {
        let c = self.counts.entry(category).or_default();
        c.1 += 1;
        if ok {
            c.0 += 1;
        } else {
            self.mismatches
                .entry(category)
                .or_default()
                .push((file.to_owned(), detail()));
        }
    }

    fn merge(mut self, other: Report) -> Report {
        for (k, v) in other.mismatches {
            self.mismatches.entry(k).or_default().extend(v);
        }
        for (k, (a, b)) in other.counts {
            let c = self.counts.entry(k).or_default();
            c.0 += a;
            c.1 += b;
        }
        self.phash_distances.extend(other.phash_distances);
        self
    }
}

fn opt_u64(v: &Value) -> Option<u64> {
    v.as_u64().or_else(|| v.as_f64().map(|f| f.trunc() as u64))
}

fn check_file(tools: &MediaTools, rec: &Value) -> Report {
    let mut r = Report::default();
    let name = rec["file"].as_str().expect("file name");
    let path = fixtures().join("media").join(name);
    let detected = tools.detect_mime(&path);
    if let Some(err) = rec.get("mime_error") {
        let got = detected.as_ref().err().map(error_name);
        r.check("mime", name, got == err.as_str(), || {
            format!("want {err}, got {detected:?}")
        });
        return r;
    }
    let want_mime = mime(&rec["mime"]);
    let ok = matches!(detected, Ok(m) if m == want_mime);
    r.check("mime", name, ok, || {
        format!("want {want_mime:?}, got {detected:?}")
    });

    let hashes = hydrus_media::hash_file(&path).expect("hashable");
    let h = &rec["hashes"];
    let ok = hashes.sha256.to_hex() == h["sha256"]
        && hashes.md5.to_hex() == h["md5"]
        && hashes.sha1.to_hex() == h["sha1"]
        && hashes.sha512.to_hex() == h["sha512"];
    r.check("hashes", name, ok, String::new);

    let info = tools.inspect_as(&path, want_mime);
    if let Some(err) = rec.get("info_error") {
        let got = info.as_ref().err().map(error_name);
        r.check("info", name, got == err.as_str(), || {
            format!("want {err}, got {info:?}")
        });
        return r;
    }
    let want = &rec["info"];
    let info = match info {
        Ok(i) => i,
        Err(e) => {
            r.check("info", name, false, || format!("error {e}"));
            return r;
        }
    };
    let ok = info.size == want["size"].as_u64().unwrap_or(0)
        && info.width.map(u64::from) == want["width"].as_u64()
        && info.height.map(u64::from) == want["height"].as_u64()
        && info.duration_ms == opt_u64(&want["duration_ms"])
        && info.num_frames == want["num_frames"].as_u64()
        && info.has_audio == want["has_audio"].as_bool().unwrap_or(false)
        && info.num_words == want["num_words"].as_u64();
    r.check("info", name, ok, || format!("want {want}, got {info:?}"));

    if let Some(t) = rec.get("thumbnail") {
        let spec = ThumbnailSpec::default();
        match tools.thumbnail(&path, &info, &spec) {
            Ok(thumb) => {
                let dims_ok = u64::from(thumb.pixels.width()) == t["width"].as_u64().unwrap_or(0)
                    && u64::from(thumb.pixels.height()) == t["height"].as_u64().unwrap_or(0);
                r.check("thumbnail size", name, dims_ok, || {
                    format!(
                        "want {}x{}, got {}x{}",
                        t["width"],
                        t["height"],
                        thumb.pixels.width(),
                        thumb.pixels.height()
                    )
                });
                let want_format = if t["format"] == "png" {
                    ThumbnailFormat::Png
                } else {
                    ThumbnailFormat::Jpeg
                };
                r.check(
                    "thumbnail format",
                    name,
                    thumb.format == want_format,
                    || format!("want {want_format:?}, got {:?}", thumb.format),
                );
                r.check(
                    "thumbnail default",
                    name,
                    thumb.is_default == t["is_default"].as_bool().unwrap_or(false),
                    || format!("want default={}, got {}", t["is_default"], thumb.is_default),
                );
                let px = hex::encode(sha2::Digest::finalize(
                    <sha2::Sha256 as sha2::Digest>::new_with_prefix(thumb.pixels.data()),
                ));
                r.check(
                    "thumbnail pixels",
                    name,
                    px == t["pixels_sha256"],
                    String::new,
                );
                let bh = hydrus_media::blurhash(&thumb.pixels);
                r.check(
                    "thumbnail blurhash",
                    name,
                    bh.as_deref() == t["blurhash"].as_str(),
                    || format!("want {}, got {bh:?}", t["blurhash"]),
                );
            }
            Err(e) => r.check("thumbnail size", name, false, || format!("error {e}")),
        }
    }

    if let Some(want) = rec.get("perceptual_hashes") {
        let got = tools.perceptual_hashes(&path, want_mime);
        let want: Vec<&str> = want
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let got_hex: Vec<String> = got
            .iter()
            .map(hydrus_core::PerceptualHash::to_hex)
            .collect();
        r.check("perceptual hash", name, got_hex == want, || {
            format!("want {want:?}, got {got_hex:?}")
        });
        if let (Some(w), Some(g)) = (want.first(), got.first()) {
            let w: hydrus_core::PerceptualHash = w.parse().unwrap();
            r.phash_distances.push((name.to_owned(), w.distance(g)));
        }
    }

    if let Some(d) = rec.get("decoded")
        && let Some(want_sha) = d.get("sha256")
    {
        match tools.load_image(&path, want_mime) {
            Ok(img) => {
                let got = hex::encode(sha2::Digest::finalize(
                    <sha2::Sha256 as sha2::Digest>::new_with_prefix(img.data()),
                ));
                r.check("decoded pixels", name, got == *want_sha, || {
                    format!(
                        "want shape {}, got {}x{}x{}",
                        d["shape"],
                        img.height(),
                        img.width(),
                        img.channels()
                    )
                });
                let bh = hydrus_media::blurhash(&img);
                r.check(
                    "decoded blurhash",
                    name,
                    bh.as_deref() == d["blurhash"].as_str(),
                    || format!("want {}, got {bh:?}", d["blurhash"]),
                );
            }
            Err(e) => r.check("decoded pixels", name, false, || format!("error {e}")),
        }
    }

    if let Some(want) = rec.get("pixel_hash") {
        let got = tools.pixel_hash(&path, &info).map(|h| h.to_hex());
        r.check("pixel hash", name, got.as_deref() == want.as_str(), || {
            format!("want {want}, got {got:?}")
        });
    }

    let flags = tools.flags(&path, &info);
    let flag = |k: &str| rec[k].as_bool().unwrap_or(false);
    r.check(
        "has_transparency",
        name,
        flags.has_transparency == flag("has_transparency"),
        || format!("want {}", flag("has_transparency")),
    );
    r.check("has_exif", name, flags.has_exif == flag("has_exif"), || {
        format!("want {}", flag("has_exif"))
    });
    r.check(
        "has_icc_profile",
        name,
        flags.has_icc_profile == flag("has_icc_profile"),
        || format!("want {}", flag("has_icc_profile")),
    );
    r.check(
        "has_human_readable_embedded_metadata",
        name,
        flags.has_human_readable_embedded_metadata == flag("has_human_readable_embedded_metadata"),
        || format!("want {}", flag("has_human_readable_embedded_metadata")),
    );
    r
}

/// Pixel values come from a different decoder than the reference's, so
/// pixel-derived values are close but not equal.
const APPROXIMATE_PIXELS: &[&str] = &[
    "decoded pixels",
    "decoded blurhash",
    "pixel hash",
    "perceptual hash",
    "thumbnail pixels",
    "thumbnail blurhash",
];

/// Only the thumbnail is rasterised by other code (Qt there, resvg/hayro here).
const APPROXIMATE_RENDER: &[&str] = &["thumbnail pixels", "thumbnail blurhash"];

/// (file, categories allowed to differ, why)
const KNOWN_DIFFERENCES: &[(&str, &[&str], &str)] = &[
    (
        "avif_still.avif",
        APPROXIMATE_PIXELS,
        "AVIF decoded by ffmpeg, not libavif+libyuv",
    ),
    (
        "avif_alpha.avif",
        APPROXIMATE_PIXELS,
        "AVIF decoded by ffmpeg, not libavif+libyuv",
    ),
    (
        "avif_alpha.avif",
        &["has_transparency", "thumbnail format"],
        "ffmpeg 6 does not decode AVIF alpha planes",
    ),
    (
        "jxl_still.jxl",
        APPROXIMATE_PIXELS,
        "JPEG XL decoded by ffmpeg's libjxl, not pillow-jxl's",
    ),
    ("heic_still.heic", APPROXIMATE_PIXELS, "no HEIF decoder"),
    ("heic_alpha.heic", APPROXIMATE_PIXELS, "no HEIF decoder"),
    (
        "heic_still.heic",
        &["thumbnail default", "thumbnail format"],
        "no HEIF decoder: default thumbnail",
    ),
    (
        "heic_alpha.heic",
        &["thumbnail default", "has_transparency"],
        "no HEIF decoder: default thumbnail",
    ),
    (
        "pdf_image.pdf",
        APPROXIMATE_RENDER,
        "rendered by hayro, not pdfium",
    ),
    (
        "pdf_text.pdf",
        APPROXIMATE_RENDER,
        "rendered by hayro, not pdfium",
    ),
    (
        "pdf_multipage.pdf",
        APPROXIMATE_RENDER,
        "rendered by hayro, not pdfium",
    ),
    (
        "svg_sized.svg",
        APPROXIMATE_RENDER,
        "rendered by resvg, not Qt",
    ),
    (
        "svg_viewbox.svg",
        APPROXIMATE_RENDER,
        "rendered by resvg, not Qt",
    ),
];

fn is_known(file: &str, category: &str) -> bool {
    KNOWN_DIFFERENCES
        .iter()
        .any(|(f, cats, _)| *f == file && cats.contains(&category))
}

/// Files whose values (beyond their bytes and type) come from ffmpeg.
fn through_ffmpeg(mime: Mime) -> bool {
    use hydrus_media::mimes;
    mimes::is_video(mime)
        || mimes::is_audio(mime)
        || matches!(
            mime,
            Mime::ImageAvif
                | Mime::ImageAvifSequence
                | Mime::ImageHeic
                | Mime::ImageHeif
                | Mime::ImageHeicSequence
                | Mime::ImageHeifSequence
                | Mime::ImageJxl
                | Mime::ApplicationPsd
        )
}

#[test]
fn corpus_matches_reference() {
    let json = fixture();
    let tools = MediaTools::new();
    let report = json["files"]
        .as_array()
        .unwrap()
        .par_iter()
        .map(|rec| check_file(&tools, rec))
        .reduce(Report::default, Report::merge);
    for (category, (ok, total)) in &report.counts {
        println!("{category:>40}: {ok}/{total} exact");
    }
    let recording_ffmpeg = hydrus_testkit::recording_ffmpeg();
    if !recording_ffmpeg {
        println!(
            "not the fixtures' ffmpeg ({}... on x86-64): values from ffmpeg are only reported",
            hydrus_testkit::RECORDED_FFMPEG
        );
    }
    let mimes: BTreeMap<&str, Mime> = json["files"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|rec| rec.get("mime_error").is_none())
        .map(|rec| (rec["file"].as_str().unwrap(), mime(&rec["mime"])))
        .collect();
    let mut unexpected = Vec::new();
    for (category, list) in &report.mismatches {
        for (file, detail) in list {
            let known = is_known(file, category);
            let other_ffmpeg = !recording_ffmpeg
                && !["mime", "hashes"].contains(category)
                && mimes.get(file.as_str()).is_some_and(|&m| through_ffmpeg(m));
            println!(
                "{} [{category}] {file}: {detail}",
                if known {
                    "known difference"
                } else if other_ffmpeg {
                    "differs with this ffmpeg"
                } else {
                    "MISMATCH"
                }
            );
            if !known && !other_ffmpeg {
                unexpected.push(format!("[{category}] {file}: {detail}"));
            }
        }
    }
    let mut distances = report.phash_distances.clone();
    distances.sort();
    let exact = distances.iter().filter(|(_, d)| *d == 0).count();
    let off: Vec<_> = distances.iter().filter(|(_, d)| *d > 0).collect();
    println!(
        "perceptual hashes: {exact}/{} bit-exact; hamming distances of the rest: {off:?}",
        distances.len()
    );
    assert!(
        unexpected.is_empty(),
        "unexpected differences:\n{}",
        unexpected.join("\n")
    );
}

#[test]
fn thumbnail_resolutions_match_reference() {
    let json = fixture();
    for case in json["thumbnail_resolutions"].as_array().unwrap() {
        let dim = |k: &str| case[k].as_u64().map(|v| u32::try_from(v).unwrap());
        let b = case["bounding"].as_array().unwrap();
        let spec = ThumbnailSpec {
            bounding: (b[0].as_u64().unwrap() as u32, b[1].as_u64().unwrap() as u32),
            scale: ThumbnailScale::from_code(case["scale_type"].as_u64().unwrap() as u8).unwrap(),
            dpr_percent: case["dpr_percent"].as_u64().unwrap() as u32,
            video_percentage_in: 35,
        };
        let got = hydrus_media::thumbnail_resolution(dim("width"), dim("height"), &spec);
        let want = case["result"]
            .as_array()
            .map(|r| (r[0].as_u64().unwrap() as u32, r[1].as_u64().unwrap() as u32));
        assert_eq!(got, want, "{case}");
    }
}

#[test]
fn opencv_ports_are_bit_exact() {
    use hydrus_media::resample::{Interpolation, resize, rgb_to_gray};
    let json = fixture();
    let mut sources = BTreeMap::new();
    for op in json["cv_ops"].as_array().unwrap() {
        let source = op["source"].as_str().unwrap();
        let img = sources
            .entry(source.to_owned())
            .or_insert_with(|| {
                hydrus_media::decode_image(
                    &std::fs::read(fixtures().join("media").join(source)).unwrap(),
                )
                .unwrap()
            })
            .clone();
        let img = if op["variant"] == "grey" {
            let rgb = if img.channels() == 4 {
                let data = img
                    .data()
                    .chunks_exact(4)
                    .flat_map(|p| [p[0], p[1], p[2]])
                    .collect();
                hydrus_media::Raster::new(img.width(), img.height(), 3, data).unwrap()
            } else {
                img
            };
            rgb_to_gray(&rgb)
        } else {
            img
        };
        let sha = |r: &hydrus_media::Raster| {
            hex::encode(sha2::Digest::finalize(
                <sha2::Sha256 as sha2::Digest>::new_with_prefix(r.data()),
            ))
        };
        match op["op"].as_str().unwrap() {
            "identity" => assert_eq!(sha(&img), op["sha256"], "{op}"),
            "phash" => assert_eq!(
                hydrus_media::perceptual_hash(&img).to_hex(),
                op["phash"],
                "{op}"
            ),
            kind => {
                let s = op["size"].as_array().unwrap();
                let (w, h) = (s[0].as_u64().unwrap() as u32, s[1].as_u64().unwrap() as u32);
                let interp = if kind == "area" {
                    Interpolation::Area
                } else {
                    Interpolation::Linear
                };
                assert_eq!(sha(&resize(&img, w, h, interp)), op["sha256"], "{op}");
            }
        }
    }
}
