//! The volume and mute the preview plays at, and what its volume control
//! moves, replayed from the reference (`oracle/record_preview_audio.py`).
use hydrus_core::Mime;
use hydrus_core::media_viewer::{AudioSettings, MediaViewerSettings, ShowAction};

fn recording() -> serde_json::Value {
    hydrus_testkit::fixture_json("preview_audio.json")
}

fn number(value: &serde_json::Value) -> u8 {
    u8::try_from(value.as_u64().unwrap()).unwrap()
}

fn flag(value: &serde_json::Value, key: &str) -> bool {
    value[key].as_bool().unwrap()
}

// A fresh client's volumes and mutes are the reference's.
#[test]
fn a_fresh_client_has_the_references_volumes_and_mutes() {
    let defaults = &recording()["defaults"];
    let audio = AudioSettings::default();
    assert_eq!(
        audio.global_volume,
        number(&defaults["global_audio_volume"])
    );
    assert_eq!(
        audio.preview_volume,
        number(&defaults["preview_audio_volume"])
    );
    assert_eq!(
        audio.viewer_volume,
        number(&defaults["media_viewer_audio_volume"])
    );
    assert_eq!(audio.global_mute, flag(defaults, "global_audio_mute"));
    assert_eq!(audio.preview_mute, flag(defaults, "preview_audio_mute"));
    assert_eq!(audio.viewer_mute, flag(defaults, "media_viewer_audio_mute"));
    assert!(flag(defaults, "preview_uses_its_own_audio_volume"));
}

#[test]
fn the_preview_plays_at_the_volume_and_mute_the_reference_picks() {
    let recording = recording();
    let volumes = &recording["volumes"];
    let cases = recording["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 32);
    for case in cases {
        let audio = AudioSettings {
            global_volume: number(&volumes["global"]),
            preview_volume: number(&volumes["preview"]),
            viewer_volume: number(&volumes["viewer"]),
            global_mute: flag(case, "global_mute"),
            preview_mute: flag(case, "preview_mute"),
            viewer_mute: flag(case, "viewer_mute"),
            viewer_uses_its_own_volume: flag(case, "viewer_uses_its_own_volume"),
        };
        let own = flag(case, "preview_uses_its_own_volume");
        assert_eq!(
            audio.current_preview_volume(own),
            number(&case["preview_volume"]),
            "{case}"
        );
        assert_eq!(audio.preview_muted(), flag(case, "preview_muted"), "{case}");
        // (the viewer's, as before)
        assert_eq!(
            audio.current_viewer_volume(),
            number(&case["viewer_volume"]),
            "{case}"
        );
        assert_eq!(audio.viewer_muted(), flag(case, "viewer_muted"), "{case}");
    }
}

// The control's slider moves the volume the preview plays at, and its
// mutes flip their own options.
#[test]
fn the_previews_slider_and_mutes_change_what_the_reference_changes() {
    let recording = recording();
    let volumes = &recording["volumes"];
    for control in recording["controls"].as_array().unwrap() {
        let own = flag(control, "preview_uses_its_own_volume");
        let fresh = AudioSettings {
            global_volume: number(&volumes["global"]),
            preview_volume: number(&volumes["preview"]),
            viewer_volume: number(&volumes["viewer"]),
            ..AudioSettings::default()
        };
        // the slider shows the volume the preview plays at, and moves it
        assert_eq!(
            fresh.current_preview_volume(own),
            number(&control["slider_shows"]),
            "{control}"
        );
        let mut moved = fresh;
        moved.set_preview_volume(own, 17);
        let after = &control["after_slider_set_to_17"];
        assert_eq!(moved.global_volume, number(&after["global_audio_volume"]));
        assert_eq!(moved.preview_volume, number(&after["preview_audio_volume"]));
        assert_eq!(
            moved.viewer_volume,
            number(&after["media_viewer_audio_volume"])
        );
        // a volume above the slider's range stops at its top
        moved.set_preview_volume(own, 250);
        assert_eq!(moved.current_preview_volume(own), 100);
    }
}

// The kinds of file the preview plays are those the reference's view
// options show in mpv (and none starts paused or behind an embed).
#[test]
fn the_preview_plays_the_kinds_the_reference_shows_in_mpv() {
    let settings = MediaViewerSettings::default();
    for entry in recording()["show_actions"].as_array().unwrap() {
        let mime = Mime::from_code(u8::try_from(entry["mime_id"].as_u64().unwrap()).unwrap())
            .unwrap_or_else(|| panic!("{entry}"));
        let view = settings.view(mime);
        assert_eq!(
            view.preview_show_action == ShowAction::Mpv,
            flag(entry, "show_action_is_mpv"),
            "{entry}"
        );
        assert_eq!(
            view.preview_start_paused,
            flag(entry, "start_paused"),
            "{entry}"
        );
        assert_eq!(
            view.preview_start_with_embed,
            flag(entry, "start_with_embed"),
            "{entry}"
        );
    }
}
