//! Options > media playback > per-filetype handling against
//! `oracle/dump_media_view_options.py`.
use hydrus_core::media_viewer::{ShowAction, default_media_view};
use hydrus_gui_model::media_view_options::{Table, action_text, capability, enabled, pretty};

#[test]
fn rows_addable_types_and_editor_choices_are_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("media_view_options.json");
    let table = Table::new(&default_media_view());
    // (the recording's client had no mpv, so its defaults used QtMediaPlayer)
    let recorded: Vec<Vec<String>> = fixture["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r["display"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().replace("QtMediaPlayer", "mpv"))
                .collect()
        })
        .collect();
    let rows: Vec<Vec<String>> = table.rows.iter().map(|r| r.cells().to_vec()).collect();
    assert_eq!(rows, recorded);
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
