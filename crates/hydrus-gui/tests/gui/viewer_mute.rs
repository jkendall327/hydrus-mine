//! The media viewer's volume submenu and the viewer's forced mute, as the
//! reference's (`oracle/record_viewer_mute.py`: its real
//! `AddAudioVolumeMenu`, and the mute its media container hands the player,
//! for every combination of the global mute, the viewer's mute, the viewer
//! using its own volume, and the viewer's forced mute). Replayed through a
//! real viewer: the saved audio settings are set, the forced mute is reached
//! by the menu's own entries, and the volume submenu it then shows and the
//! mute the viewer tells its player to play with are compared.

use hydrus_core::media_viewer::AudioSettings;
use hydrus_gui::{MainWindow, MediaViewerWindow, Pages, SearchPage, bind, headless};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use serde_json::Value;

fn hash_of(name: &str) -> String {
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(hydrus_testkit::fixture_path(
            "legacy_db/basic.manifest.json",
        ))
        .unwrap(),
    )
    .unwrap();
    manifest["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == name)
        .unwrap()["hash"]
        .as_str()
        .unwrap()
        .to_string()
}

/// The volume submenu's rows: ids and labels, in order, with the groups
/// between its separators.
fn volume_rows(viewer: &MediaViewerWindow) -> Vec<Vec<(String, i32)>> {
    use slint::Model as _;
    viewer.invoke_context_menu_requested();
    let menu = viewer.get_context_menu();
    [&menu.volume.g1, &menu.volume.g2, &menu.volume.g3]
        .into_iter()
        .map(|rows| rows.iter().map(|r| (r.label.to_string(), r.id)).collect())
        .collect()
}

/// The recorded submenu's entries, in groups split at its separators, with
/// the slider as the "volume: n" row hydrus-rs shows it as.
fn recorded_groups(menu: &Value) -> Vec<Vec<String>> {
    let entries = menu[0]["entries"].as_array().unwrap();
    let mut groups = vec![Vec::new()];
    for entry in entries {
        match entry {
            Value::String(s) if s == "---" => groups.push(Vec::new()),
            Value::String(s) => groups.last_mut().unwrap().push(s.clone()),
            slider => groups.last_mut().unwrap().push(format!(
                "{}: {}",
                slider["slider"].as_str().unwrap(),
                slider["value"]
            )),
        }
    }
    groups
}

// leaf: audit-media-viewer-audio
#[test]
fn the_volume_menu_and_the_forced_mute_are_the_reference_s() {
    let fixture = hydrus_testkit::fixture_json("viewer_mute.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited(
        format!(
            "system:hash = {}",
            hash_of(fixture["file"].as_str().unwrap())
        )
        .into(),
    );
    ui.invoke_search_accepted();
    ui.invoke_thumbnail_activated(0);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(slint::ComponentHandle::clone_strong)
        .expect("the viewer opened");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 24);
    let (mut driven, mut shown_only) = (0, 0);
    for case in cases {
        let audio = AudioSettings {
            global_mute: case["global_mute"].as_bool().unwrap(),
            viewer_mute: case["viewer_mute"].as_bool().unwrap(),
            viewer_uses_its_own_volume: case["viewer_uses_its_own_volume"].as_bool().unwrap(),
            ..AudioSettings::default()
        };
        store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &audio))
            .unwrap();
        viewer.invoke_audio_changed();
        // back to following the options, by the menu's own entry
        let stop = volume_rows(&viewer)
            .into_iter()
            .flatten()
            .find(|(label, _)| label.starts_with("stop forcing"));
        if let Some((_, id)) = stop {
            viewer.invoke_menu_chosen(id);
        }
        // forced as the case is, if the menu offers it
        let forced = case["forced"].as_bool();
        let mut reached = true;
        if let Some(mute) = forced {
            let want = if mute {
                "force mute just here"
            } else {
                "force unmute just here"
            };
            match volume_rows(&viewer)
                .into_iter()
                .flatten()
                .find(|(label, _)| label == want)
            {
                Some((_, id)) => viewer.invoke_menu_chosen(id),
                // (the reference offers only the force that isn't what the
                // options already say; the other it has only on a shortcut)
                None => reached = false,
            }
        }
        if !reached {
            shown_only += 1;
            continue;
        }
        driven += 1;
        // the submenu now
        let ours: Vec<Vec<String>> = volume_rows(&viewer)
            .into_iter()
            .map(|g| g.into_iter().map(|(l, _)| l).collect())
            .collect();
        assert_eq!(ours, recorded_groups(&case["menu"]), "{case}");
        // and what the viewer tells its player to do
        assert_eq!(
            viewer.get_playing_muted(),
            case["plays_muted"].as_bool().unwrap(),
            "{case}"
        );
    }
    // every case that the menu can reach was driven
    assert_eq!((driven, shown_only), (16, 8));
    viewer.invoke_close_requested();
}
