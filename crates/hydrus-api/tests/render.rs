//! `/get_files/render` against the reference's
//! (`oracle/fixtures/render.json`, made by `oracle/record_render.py`): the
//! same status, content type and image size and mode, and exactly the same
//! pixels: libjpeg-turbo encodes JPEG on both sides, so even those match.
//! Our WebP is always lossless (the reference encodes lossily unless asked
//! for quality over 100), so it is compared with the reference's lossless
//! render of the same file at the same size.

use axum::body::Body;
use axum::http::Request;
use base64::Engine as _;
use http_body_util::BodyExt as _;
use serde_json::Value as Json;
use sha2::Digest as _;
use tower::ServiceExt as _;

mod common;

/// (mode, width, height, pixels) as Pillow's `tobytes` gives them.
fn decode(content_type: &str, bytes: &[u8]) -> (String, u32, u32, Vec<u8>) {
    match content_type {
        "image/png" => {
            let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
            let mut reader = decoder.read_info().unwrap();
            let mut buf = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut buf).unwrap();
            buf.truncate(info.buffer_size());
            let mode = match info.color_type {
                png::ColorType::Rgb => "RGB",
                png::ColorType::Rgba => "RGBA",
                other => panic!("png of {other:?}"),
            };
            (mode.into(), info.width, info.height, buf)
        }
        "image/jpeg" => {
            let image = turbojpeg::decompress(bytes, turbojpeg::PixelFormat::RGB).unwrap();
            (
                "RGB".into(),
                image.width as u32,
                image.height as u32,
                image.pixels,
            )
        }
        "image/webp" => {
            let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
            let (w, h) = decoder.dimensions();
            let mut buf = vec![0; decoder.output_buffer_size().unwrap()];
            decoder.read_image(&mut buf).unwrap();
            let mode = if decoder.has_alpha() { "RGBA" } else { "RGB" };
            (mode.into(), w, h, buf)
        }
        other => panic!("rendered as {other}"),
    }
}

#[tokio::test]
async fn renders_match_the_reference() {
    let recorded = hydrus_testkit::fixture_json("render.json");
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let fixture = common::imported_store("basic");
    let router = hydrus_api::router(fixture.state.clone());
    let mut report = String::new();
    let mut compared = 0;
    for case in recorded["renders"].as_array().unwrap() {
        let mut query = form_urlencoded::Serializer::new(String::new());
        for (k, v) in case["params"].as_object().unwrap() {
            let v = match v {
                Json::String(s) => s.clone(),
                other => other.to_string(),
            };
            query.append_pair(k, &v);
        }
        let uri = format!("/get_files/render?{}", query.finish());
        let key = manifest["access_keys"][case["key"].as_str().unwrap()]
            .as_str()
            .unwrap();
        let request = Request::get(&uri)
            .header("Hydrus-Client-API-Access-Key", key)
            .body(Body::empty())
            .unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        let status = response.status().as_u16();
        let content_type = response.headers()["content-type"]
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let what = format!("{} {uri}", case["file"]);
        if status != case["status"].as_u64().unwrap() as u16
            || content_type != case["content_type"].as_str().unwrap()
        {
            report.push_str(&format!(
                "{what}: ours {status} {content_type} ({}), theirs {} {}\n",
                String::from_utf8_lossy(&bytes[..bytes.len().min(200)]),
                case["status"],
                case["content_type"]
            ));
            continue;
        }
        if let Some(expected) = case.get("json") {
            let actual: Json = serde_json::from_slice(&bytes).unwrap();
            if actual["exception_type"] != expected["exception_type"] {
                report.push_str(&format!("{what}: ours {actual}, theirs {expected}\n"));
            }
            continue;
        }
        let theirs = &case["image"];
        let (mode, w, h, pixels) = decode(&content_type, &bytes);
        if mode != theirs["mode"] || serde_json::json!([w, h]) != theirs["size"] {
            report.push_str(&format!(
                "{what}: ours {mode} {w}x{h}, theirs {} {}\n",
                theirs["mode"], theirs["size"]
            ));
            continue;
        }
        compared += 1;
        if content_type == "image/png" {
            if hex::encode(sha2::Sha256::digest(&pixels)) != theirs["pixels_sha256"] {
                report.push_str(&format!("{what}: pixels differ\n"));
            }
            continue;
        }
        // our WebP is always lossless: it must be the reference's lossless
        // render of the same file at the same size
        let mut theirs = theirs.clone();
        if content_type == "image/webp" && case["params"]["render_quality"].as_i64() != Some(101) {
            let lossless = recorded["renders"].as_array().unwrap().iter().find(|c| {
                c["file"] == case["file"]
                    && c["params"]["render_format"] == 33
                    && c["params"]["render_quality"] == 101
                    && c["params"]["width"] == case["params"]["width"]
                    && c["params"]["height"] == case["params"]["height"]
            });
            theirs = lossless.expect("a lossless render to compare with")["image"].clone();
        }
        let Some(expected) = theirs["pixels"].as_str() else {
            continue;
        };
        let expected = base64::engine::general_purpose::STANDARD
            .decode(expected)
            .unwrap();
        if pixels != expected {
            let differing = pixels.iter().zip(&expected).filter(|(a, b)| a != b).count();
            report.push_str(&format!("{what}: {differing} bytes of pixels differ\n"));
        }
    }
    assert!(compared > 60, "only {compared} images compared");
    assert!(report.is_empty(), "{report}");
}
