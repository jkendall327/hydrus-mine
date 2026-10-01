//! The media viewer's options migrate, and give the zooms the reference
//! gives (`oracle/record_viewer_zoom.py`): a new client's, and one whose
//! user changed the zoom steps and some file types' zoom rules.

use hydrus_core::Mime;
use hydrus_core::media_viewer::{
    MediaViewerSettings, ZoomCentre, ZoomType, canvas_zooms, next_zoom,
};
use hydrus_legacy::objects::ClientOptions;
use hydrus_legacy::serialisable::SerialisableObject;
use serde_json::Value;

fn settings_of(phase: &Value) -> MediaViewerSettings {
    let defaults = ClientOptions::defaults().unwrap();
    let Some(stored) = phase.get("options") else {
        return defaults.media_viewer_settings();
    };
    let object = SerialisableObject::from_tuple_str(&stored.to_string()).unwrap();
    let mut options = ClientOptions::from_object(&object).unwrap();
    options.fill_defaults(&defaults);
    options.media_viewer_settings()
}

#[test]
fn zooms_are_the_references() {
    let fixture = hydrus_testkit::fixture_json("viewer_zoom.json");
    for name in ["defaults", "custom"] {
        let phase = &fixture[name];
        let settings = settings_of(phase);
        let zooms: Vec<f64> = serde_json::from_value(phase["media_zooms"].clone()).unwrap();
        assert_eq!(settings.media_zooms, zooms, "{name}");
        assert_eq!(
            settings.zoom_centre as i64,
            phase["zoom_center"].as_i64().unwrap(),
            "{name}"
        );
        assert_eq!(
            settings.default_zoom_type as i64,
            phase["default_zoom_type"].as_i64().unwrap(),
            "{name}"
        );
        let cases = phase["cases"].as_array().unwrap();
        assert!(cases.len() > 50);
        for case in cases {
            let mime =
                Mime::from_code(u8::try_from(case["mime"].as_u64().unwrap()).unwrap()).unwrap();
            let resolution = case["resolution"][0].as_u64().map(|w| {
                (
                    u32::try_from(w).unwrap(),
                    u32::try_from(case["resolution"][1].as_u64().unwrap()).unwrap(),
                )
            });
            let canvas = (
                u32::try_from(case["canvas"][0].as_u64().unwrap()).unwrap(),
                u32::try_from(case["canvas"][1].as_u64().unwrap()).unwrap(),
            );
            let dpr = case["device_pixel_ratio"].as_f64().unwrap();
            let what = format!("{name}: {} in {canvas:?} at {dpr}", case["file"]);
            assert_eq!(
                settings.view(mime).media_show_action as i64,
                case["show_action"].as_i64().unwrap(),
                "{what}"
            );
            let ours = canvas_zooms(&settings, mime, resolution, canvas, dpr);
            for zoom_type in ZoomType::ALL {
                let theirs = case["zooms"][(zoom_type as i64).to_string()]
                    .as_f64()
                    .unwrap();
                let ours = ours[&zoom_type];
                assert!(
                    (ours - theirs).abs() < 1e-12,
                    "{what}: {zoom_type:?} is {ours}, not {theirs}"
                );
            }
        }
    }
}

#[test]
fn zoom_steps_go_through_the_regular_zooms_and_canvas_fit() {
    let settings = MediaViewerSettings::default();
    assert_eq!(settings.zoom_centre, ZoomCentre::Mouse);
    let rules = settings.view(Mime::ImageJpeg).zoom;
    // canvas fit is a step too
    assert_eq!(next_zoom(&settings, &rules, 1.0, 1.3, true), Some(1.1));
    assert_eq!(next_zoom(&settings, &rules, 1.2, 1.3, true), Some(1.3));
    assert_eq!(next_zoom(&settings, &rules, 1.3, 1.3, true), Some(1.5));
    assert_eq!(next_zoom(&settings, &rules, 1.5, 1.3, false), Some(1.3));
    assert_eq!(next_zoom(&settings, &rules, 20.0, 0.4, true), None);
    assert_eq!(next_zoom(&settings, &rules, 0.01, 0.4, false), None);
    assert_eq!(next_zoom(&settings, &rules, 0.004, 0.004, true), Some(0.01));
    // exact zooms only: powers of two, none past the largest regular zoom
    let exact = hydrus_core::media_viewer::ZoomRules {
        exact_zooms_only: true,
        ..rules
    };
    assert_eq!(next_zoom(&settings, &exact, 1.0, 0.3, true), Some(2.0));
    assert_eq!(next_zoom(&settings, &exact, 0.3, 0.3, true), Some(0.5));
    assert_eq!(next_zoom(&settings, &exact, 3.0, 0.3, false), Some(2.0));
    assert_eq!(next_zoom(&settings, &exact, 1.0, 0.3, false), Some(0.5));
    assert_eq!(next_zoom(&settings, &exact, 0.6, 0.3, false), Some(0.5));
    assert_eq!(next_zoom(&settings, &exact, 0.5, 0.3, false), Some(0.3));
    assert_eq!(next_zoom(&settings, &exact, 16.0, 0.3, true), None);
}
