//! Options > media playback > per-filetype handling against
//! `oracle/dump_media_view_options.py`.
use hydrus_core::media_viewer::{
    MediaView, ScaleAction, ShowAction, ZoomRules, default_media_view,
};
use hydrus_gui_model::media_view_options::{Table, action_text, capability, enabled, pretty};
use std::collections::BTreeMap;

// Decode the actual reference inputs, not display strings or adapted defaults.
fn recorded_views(context: &serde_json::Value) -> BTreeMap<u8, MediaView> {
    context["views"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| {
            let mime = u8::try_from(entry[0].as_u64().unwrap()).unwrap();
            let view = &entry[1];
            let zoom = &view[6];
            (
                mime,
                MediaView {
                    media_show_action: ShowAction::from_code(view[0].as_i64().unwrap()).unwrap(),
                    media_start_paused: view[1].as_bool().unwrap(),
                    media_start_with_embed: view[2].as_bool().unwrap(),
                    preview_show_action: ShowAction::from_code(view[3].as_i64().unwrap()).unwrap(),
                    preview_start_paused: view[4].as_bool().unwrap(),
                    preview_start_with_embed: view[5].as_bool().unwrap(),
                    zoom: ZoomRules {
                        media_scale_up: ScaleAction::from_code(zoom[0].as_i64().unwrap()).unwrap(),
                        media_scale_down: ScaleAction::from_code(zoom[1].as_i64().unwrap())
                            .unwrap(),
                        preview_scale_up: ScaleAction::from_code(zoom[2].as_i64().unwrap())
                            .unwrap(),
                        preview_scale_down: ScaleAction::from_code(zoom[3].as_i64().unwrap())
                            .unwrap(),
                        exact_zooms_only: zoom[4].as_bool().unwrap(),
                        scale_up_quality: u8::try_from(zoom[5].as_u64().unwrap()).unwrap(),
                        scale_down_quality: u8::try_from(zoom[6].as_u64().unwrap()).unwrap(),
                    },
                },
            )
        })
        .collect()
}

#[test]
fn rows_addable_types_and_editor_choices_are_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("media_view_options.json");
    // Native defaults intentionally use the reference's MPV/non-macOS branch.
    let mpv = &fixture["default_contexts"]["mpv"];
    assert_eq!(default_media_view(), recorded_views(mpv));
    assert_eq!(fixture["rows"], fixture["default_contexts"]["qt"]["rows"]);
    let table = Table::new(&default_media_view());
    for name in ["qt", "mpv"] {
        let context = &fixture["default_contexts"][name];
        let reference_table = Table::new(&recorded_views(context));
        let recorded: Vec<Vec<String>> = context["rows"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                row["display"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|cell| cell.as_str().unwrap().to_owned())
                    .collect()
            })
            .collect();
        let rows: Vec<Vec<String>> = reference_table
            .rows
            .iter()
            .map(|row| row.cells().to_vec())
            .collect();
        assert_eq!(rows, recorded, "{name}");
    }
    let addable: Vec<(String, u8)> = fixture["addable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            (
                a[0].as_str().unwrap().to_owned(),
                u8::try_from(a[1].as_u64().unwrap()).unwrap(),
            )
        })
        .collect();
    assert_eq!(table.addable(), addable);
    for editor in fixture["editors"].as_array().unwrap() {
        let code = u8::try_from(editor["mime"].as_u64().unwrap()).unwrap();
        let capability = capability(code).unwrap();
        assert_eq!(capability.intro, editor["intro"]);
        let words = |actions: &[ShowAction]| {
            actions
                .iter()
                .map(|a| action_text(*a).to_owned())
                .collect::<Vec<_>>()
        };
        let recorded = |key: &str| -> Vec<String> {
            editor[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| {
                    c[0].as_str()
                        .unwrap()
                        .trim_end_matches(" (no audio support)")
                        .to_owned()
                })
                .collect()
        };
        assert_eq!(words(capability.media), recorded("media"), "{code}");
        assert_eq!(words(capability.preview), recorded("preview"), "{code}");
        if editor["pretty"].as_str().unwrap().contains(':') {
            assert_eq!(pretty(code), editor["pretty"], "{code}");
        }
    }
}

#[test]
fn delete_spares_classes_and_add_copies_the_class() {
    let mut table = Table::new(&default_media_view());
    let class = table
        .rows
        .iter()
        .position(|r| pretty(r.code) == "image")
        .unwrap();
    table.click(class, false, false);
    assert!(!table.can_delete());
    let (_, png) = table
        .addable()
        .into_iter()
        .find(|(p, _)| p == "image: png")
        .unwrap();
    let row = table.new_row(png).unwrap();
    assert_eq!(
        row.view,
        default_media_view()[&hydrus_core::Mime::GeneralImage.code()]
    );
    table.put(row);
    assert_eq!(table.selected().map(|r| r.code), Some(png));
    assert!(table.can_delete());
    table.delete_selected();
    assert!(table.rows.iter().all(|r| r.code != png));
    let e = enabled(png, ShowAction::Native, ShowAction::DoNotShow);
    assert!(e.media_scales && !e.preview_scales && !e.media_paused && e.qualities);
}
