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

/// An animation's frames: (duration in ms, (width, height), mode, pixels'
/// sha256), as Pillow reports them, and its loop count.
fn decode_animation(content_type: &str, bytes: &[u8]) -> (Option<u64>, Vec<Json>) {
    let frame = |ms: u64, w: u32, h: u32, mode: &str, pixels: &[u8]| {
        serde_json::json!({
            "duration": ms,
            "size": [w, h],
            "mode": mode,
            "pixels_sha256": hex::encode(sha2::Sha256::digest(pixels)),
        })
    };
    match content_type {
        "image/apng" => {
            let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
                .read_info()
                .unwrap();
            let control = reader.info().animation_control.unwrap();
            let mut frames = Vec::new();
            for _ in 0..control.num_frames {
                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf).unwrap();
                buf.truncate(info.buffer_size());
                let fc = reader.info().frame_control.unwrap();
                let den = if fc.delay_den == 0 { 100 } else { fc.delay_den };
                let ms = (f64::from(fc.delay_num) * 1000.0 / f64::from(den)).round() as u64;
                let mode = match info.color_type {
                    png::ColorType::Rgb => "RGB",
                    png::ColorType::Rgba => "RGBA",
                    other => panic!("apng of {other:?}"),
                };
                frames.push(frame(ms, info.width, info.height, mode, &buf));
            }
            (Some(u64::from(control.num_plays)), frames)
        }
        "image/webp" => {
            let mut decoder = image_webp::WebPDecoder::new(std::io::Cursor::new(bytes)).unwrap();
            assert!(decoder.is_animated());
            let (w, h) = decoder.dimensions();
            let mode = if decoder.has_alpha() { "RGBA" } else { "RGB" };
            let looping = match decoder.loop_count() {
                image_webp::LoopCount::Forever => 0,
                image_webp::LoopCount::Times(n) => u64::from(n.get()),
            };
            let mut frames = Vec::new();
            for _ in 0..decoder.num_frames() {
                let mut buf = vec![0; decoder.output_buffer_size().unwrap()];
                let ms = decoder.read_frame(&mut buf).unwrap();
                frames.push(frame(u64::from(ms), w, h, mode, &buf));
            }
            (Some(looping), frames)
        }
        other => panic!("rendered as {other}"),
    }
}

/// Ugoiras rendered as APNG and animated WebP, against the reference
/// (`oracle/fixtures/ugoira_render.json`, made by
/// `oracle/record_ugoira_render.py`): frame timings from animation.json
/// or the file's notes, EXIF-rotated frames with exactly the reference's
/// pixels, and the cache lifetime. Our WebP is lossless, so its pixels
/// are compared with the reference's lossless render.
#[tokio::test]
async fn ugoiras_render_as_the_reference_renders_them() {
    let recorded = hydrus_testkit::fixture_json("ugoira_render.json");
    let manifest = hydrus_testkit::fixture_json("legacy_db/basic.manifest.json");
    let key = manifest["access_keys"]["full"].as_str().unwrap().to_owned();
    let fixture = common::imported_store("basic");
    let router = hydrus_api::router(fixture.state.clone());
    let send = |request: Request<Body>| {
        let router = router.clone();
        async move { router.oneshot(request).await.unwrap() }
    };
    let post = |path: &str, body: Json| {
        Request::post(path)
            .header("Hydrus-Client-API-Access-Key", &key)
            .header("Content-Type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    };
    for (name, hash) in recorded["hashes"].as_object().unwrap() {
        let path = hydrus_testkit::fixture_path(format!("media/{name}"));
        let response = send(post(
            "/add_files/add_file",
            serde_json::json!({"path": path.to_str().unwrap()}),
        ))
        .await;
        let body: Json =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(&body["hash"], hash, "{name}: {body}");
    }
    let mut report = String::new();
    let mut state = (String::new(), String::new());
    let mut compared = 0;
    for case in recorded["renders"].as_array().unwrap() {
        let (file, case_state) = (
            case["file"].as_str().unwrap(),
            case["state"].as_str().unwrap(),
        );
        let hash = recorded["hashes"][file].as_str().unwrap();
        if (file.to_owned(), case_state.to_owned()) != state {
            state = (file.to_owned(), case_state.to_owned());
            if let Some(step) = recorded["notes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["file"] == file && n["state"] == case_state)
            {
                let response = send(post(
                    "/add_notes/set_notes",
                    serde_json::json!({"hash": hash, "notes": step["notes"]}),
                ))
                .await;
                assert_eq!(response.status(), 200);
            }
        }
        let mut query = form_urlencoded::Serializer::new(String::new());
        query.append_pair("hash", hash);
        for (k, v) in case["params"].as_object().unwrap() {
            let v = match v {
                Json::String(s) => s.clone(),
                other => other.to_string(),
            };
            query.append_pair(k, &v);
        }
        let uri = format!("/get_files/render?{}", query.finish());
        let response = send(
            Request::get(&uri)
                .header("Hydrus-Client-API-Access-Key", &key)
                .body(Body::empty())
                .unwrap(),
        )
        .await;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        };
        let what = format!("{file} ({case_state}) {}", case["label"]);
        let ours = serde_json::json!({
            "status": response.status().as_u16(),
            "content_type": header("content-type"),
            "cache_control": header("cache-control"),
            "disposition": header("content-disposition"),
        });
        let theirs = serde_json::json!({
            "status": case["status"],
            "content_type": case["content_type"],
            "cache_control": case["cache_control"],
            "disposition": case["disposition"],
        });
        let content_type = header("content-type").unwrap_or_default();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        if ours != theirs {
            report.push_str(&format!("{what}: ours {ours}, theirs {theirs}\n"));
            continue;
        }
        if let Some(expected) = case.get("json") {
            let actual: Json = serde_json::from_slice(&bytes).unwrap();
            if actual["error"] != expected["error"] {
                report.push_str(&format!("{what}: ours {actual}, theirs {expected}\n"));
            }
            continue;
        }
        let (looping, frames) = decode_animation(&content_type, &bytes);
        let animation = &case["animation"];
        let mut want: Vec<Json> = animation["frames"].as_array().unwrap().clone();
        if case["label"] == "webp" {
            // (theirs is lossy: the timings and sizes must match, and the
            // pixels the lossless render's)
            let lossless = recorded["renders"].as_array().unwrap().iter().find(|c| {
                c["file"] == case["file"]
                    && c["state"] == case["state"]
                    && c["label"] == "webp_lossless"
            });
            want = lossless.expect("a lossless render")["animation"]["frames"]
                .as_array()
                .unwrap()
                .clone();
        }
        compared += 1;
        if looping != animation["loop"].as_u64() || frames != want {
            report.push_str(&format!(
                "{what}:\n  ours   loop {looping:?} {frames:?}\n  theirs loop {} {want:?}\n",
                animation["loop"]
            ));
        }
    }
    assert!(compared >= 20, "only {compared} renders compared");
    assert!(report.is_empty(), "{report}");
}
