//! The preview pane plays the file it shows, at the preview's own volume or
//! the global one, as the Options window says, with the volume control the
//! reference puts on the preview (`oracle/record_preview_audio.py`).
//!
//! These run without libmpv: the preview's player records what it is asked
//! to play and at what volume and mute, and the decisions are checked there.
//! What libmpv does with them (the sound, the frames) is not tested here.
use std::time::{Duration, Instant};

use hydrus_core::HashId;
use hydrus_core::media_viewer::AudioSettings;
use hydrus_gui::headless;
use hydrus_gui_model::audio;
use hydrus_store::reference_options::ReferenceOptions;
use slint::ComponentHandle as _;

use crate::options_gui_support::{Client, row, show_page};

const OWN: &str = "The preview window has its own volume: ";

fn hash_of(name: &str) -> hydrus_core::Sha256 {
    let manifest: serde_json::Value = serde_json::from_str(
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
        .parse()
        .unwrap()
}

struct Preview {
    client: Client,
}

impl Preview {
    fn new() -> Self {
        let client = Client::basic();
        // (a real mpv, where libmpv is installed, must not open a sound
        // device: the store's mpv.conf is loaded by every player)
        std::fs::write(client.store.dir().join("mpv.conf"), "ao=null\n").unwrap();
        client.ui.invoke_search_edited("system:everything".into());
        client.ui.invoke_search_accepted();
        for _ in 0..3 {
            headless::render(&client.native(), 1000, 900);
        }
        client.bound.preview.refresh();
        assert!(client.ui.get_preview_actual_height() > 0.0);
        Self { client }
    }

    fn id(&self, name: &str) -> HashId {
        self.client
            .store
            .read(|c| hydrus_store::master::hash_id(c, &hash_of(name)))
            .unwrap()
            .unwrap()
    }

    /// Select a file and wait for the preview to have accepted it.
    fn select(&self, name: &str) {
        let file = self.id(name);
        let index = self
            .client
            .bound
            .current
            .borrow()
            .borrow()
            .results()
            .iter()
            .position(|&f| f == file)
            .unwrap();
        self.client
            .ui
            .invoke_thumbnail_clicked(i32::try_from(index).unwrap(), false, false);
        self.until(|| self.client.bound.preview.displayed_file() == Some(file));
    }

    /// Refresh the preview until `done` (its player and control follow the
    /// settings on a short timer).
    fn until(&self, done: impl Fn() -> bool) {
        let started = Instant::now();
        while !done() {
            self.client.bound.preview.refresh();
            slint::platform::update_timers_and_animations();
            assert!(started.elapsed() < Duration::from_secs(10), "timed out");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn playing(&self) -> (Option<String>, (u8, bool)) {
        let (path, sound) = self.client.bound.preview.playing();
        (
            path.map(|p| p.file_stem().unwrap().to_string_lossy().into_owned()),
            sound,
        )
    }

    fn settings(&self) -> AudioSettings {
        audio::settings(&self.client.store)
    }

    fn set_own(&self, own: bool) {
        self.client
            .store
            .write_and_refresh(move |ctx| {
                let mut options: ReferenceOptions = hydrus_store::settings::get(ctx.conn())?;
                options.set_boolean("preview_uses_its_own_audio_volume", own);
                hydrus_store::settings::set(ctx.conn(), &options)
            })
            .unwrap();
    }

    fn set_audio(&self, audio: AudioSettings) {
        self.client
            .store
            .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &audio))
            .unwrap();
    }
}

// leaf: audit-options-audio-the-preview-window-has-its-own-volume
#[test]
fn the_preview_plays_what_it_shows_at_the_volume_the_option_picks() {
    let p = Preview::new();
    let recording = hydrus_testkit::fixture_json("preview_audio.json");
    let volumes = &recording["volumes"];
    let volume = |key: &str| u8::try_from(volumes[key].as_u64().unwrap()).unwrap();

    // a fresh client: the preview has its own volume, 70, nothing muted
    assert_eq!(p.settings(), AudioSettings::default());
    assert!(audio::preview_uses_its_own_volume(&p.client.store));

    // a still plays nothing, and has no volume control
    p.select("jpeg_00.jpg");
    assert_eq!(p.playing().0, None);
    assert!(!p.client.ui.get_preview_volume_shown());

    // a file with sound plays, at the preview's volume; the control shows
    p.select("audio.flac");
    assert_eq!(
        p.playing(),
        (Some(hash_of("audio.flac").to_string()), (70, false))
    );
    p.until(|| p.client.ui.get_preview_volume_shown());
    assert_eq!(p.client.ui.get_preview_volume(), 70);

    // a silent animation plays, with no volume control
    p.select("gif_animated.gif");
    assert_eq!(p.playing().0, Some(hash_of("gif_animated.gif").to_string()));
    p.until(|| !p.client.ui.get_preview_volume_shown());

    // audio plays; going back to a still stops it
    p.select("audio.mp3");
    assert_eq!(p.playing().0, Some(hash_of("audio.mp3").to_string()));
    p.until(|| p.client.ui.get_preview_volume_shown());
    p.select("png_00.png");
    assert_eq!(p.playing().0, None);
    p.select("audio.flac");

    // the volume and mute for every combination of options, as the
    // reference picks them
    for case in recording["cases"].as_array().unwrap() {
        let flag = |key: &str| case[key].as_bool().unwrap();
        p.set_own(flag("preview_uses_its_own_volume"));
        p.set_audio(AudioSettings {
            global_volume: volume("global"),
            preview_volume: volume("preview"),
            viewer_volume: volume("viewer"),
            global_mute: flag("global_mute"),
            preview_mute: flag("preview_mute"),
            viewer_mute: flag("viewer_mute"),
            viewer_uses_its_own_volume: flag("viewer_uses_its_own_volume"),
        });
        let expected = (
            u8::try_from(case["preview_volume"].as_u64().unwrap()).unwrap(),
            flag("preview_muted"),
        );
        // (the player and the control, which shows the volume its slider
        // moves and both mutes)
        p.until(|| {
            let ui = &p.client.ui;
            p.playing().1 == expected
                && ui.get_preview_volume() == i32::from(expected.0)
                && ui.get_preview_global_muted() == flag("global_mute")
                && ui.get_preview_muted() == flag("preview_mute")
        });
    }

    // the control, against each setting of the option
    for control in recording["controls"].as_array().unwrap() {
        let flag = |value: &serde_json::Value, key: &str| value[key].as_bool().unwrap();
        let own = flag(control, "preview_uses_its_own_volume");
        let option = |name: &str| {
            u8::try_from(control["after_slider_set_to_17"][name].as_u64().unwrap()).unwrap()
        };
        p.set_own(own);
        p.set_audio(AudioSettings {
            global_volume: volume("global"),
            preview_volume: volume("preview"),
            viewer_volume: volume("viewer"),
            ..AudioSettings::default()
        });
        let shows = i32::try_from(control["slider_shows"].as_u64().unwrap()).unwrap();
        p.until(|| p.client.ui.get_preview_volume() == shows);

        p.client.ui.invoke_preview_volume_changed(17);
        let now = p.settings();
        assert_eq!(now.global_volume, option("global_audio_volume"));
        assert_eq!(now.preview_volume, option("preview_audio_volume"));
        assert_eq!(now.viewer_volume, option("media_viewer_audio_volume"));
        assert_eq!(p.client.ui.get_preview_volume(), 17);
        assert_eq!(p.playing().1.0, 17);

        let muted = |after: &str, name: &str| flag(&control[after], name);
        p.client.ui.invoke_preview_flip_mute();
        let now = p.settings();
        assert_eq!(
            now.preview_mute,
            muted("after_preview_mute_clicked", "preview_audio_mute")
        );
        assert_eq!(
            now.global_mute,
            muted("after_preview_mute_clicked", "global_audio_mute")
        );
        assert!(p.client.ui.get_preview_muted());
        assert!(p.playing().1.1);
        p.client.ui.invoke_preview_flip_global_mute();
        let now = p.settings();
        assert_eq!(
            now.global_mute,
            muted("after_global_mute_clicked", "global_audio_mute")
        );
        assert!(p.client.ui.get_preview_global_muted());
    }
}

// leaf: audit-options-audio-the-preview-window-has-its-own-volume
#[test]
fn the_options_row_chooses_the_previews_own_volume_or_the_global_one_and_is_kept() {
    let p = Preview::new();
    p.set_audio(AudioSettings {
        global_volume: 30,
        preview_volume: 60,
        ..AudioSettings::default()
    });
    p.select("audio.flac");
    // (on by default: the preview's own)
    p.until(|| p.playing().1 == (60, false));

    // staged until OK
    let options = p.client.open_options();
    show_page(&options, "audio");
    let (at, shown) = row(&options, OWN);
    assert_eq!(shown.kind, 1);
    assert!(shown.checked);
    options.invoke_check_toggled(at, false);
    options.invoke_cancel();
    assert!(audio::preview_uses_its_own_volume(&p.client.store));
    p.client.bound.preview.refresh();
    assert_eq!(p.playing().1, (60, false));

    // OK: the preview plays at the global volume, and its slider moves it
    let options = p.client.open_options();
    show_page(&options, "audio");
    options.invoke_check_toggled(row(&options, OWN).0, false);
    options.invoke_apply();
    assert!(!audio::preview_uses_its_own_volume(&p.client.store));
    p.until(|| p.playing().1 == (30, false));
    p.until(|| p.client.ui.get_preview_volume() == 30);
    p.client.ui.invoke_preview_volume_changed(45);
    assert_eq!(p.settings().global_volume, 45);
    assert_eq!(p.settings().preview_volume, 60);
    assert_eq!(p.playing().1, (45, false));

    // the global mute silences the preview either way, ctrl+g included
    p.client.ui.invoke_flip_global_mute();
    p.until(|| p.playing().1 == (45, true));

    // and back: the preview's own volume again, kept after reopening
    let options = p.client.open_options();
    show_page(&options, "audio");
    assert!(!row(&options, OWN).1.checked);
    options.invoke_check_toggled(row(&options, OWN).0, true);
    options.invoke_apply();
    p.until(|| p.playing().1 == (60, true));
    let options = p.client.open_options();
    show_page(&options, "audio");
    assert!(row(&options, OWN).1.checked);
    options.invoke_cancel();
}

// leaf: audit-options-audio-the-preview-window-has-its-own-volume
#[test]
fn the_previews_volume_control_opens_under_the_pointer_and_moves_the_volume() {
    use slint::platform::{PointerEventButton, WindowEvent};
    let p = Preview::new();
    let ui = &p.client.ui;
    p.select("audio.mp3");
    p.until(|| ui.get_preview_volume_shown());
    let native = p.client.native();
    headless::render(&native, 1000, 900);
    let at = |x: f32, y: f32| slint::LogicalPosition::new(x, y);
    // closed, it is the global mute alone; the pointer on it opens it
    assert!(!ui.get_preview_volume_open());
    let (x, y) = (ui.get_preview_volume_x(), ui.get_preview_volume_y());
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(x + 12.0, y + 10.0),
    });
    assert!(ui.get_preview_volume_open());
    headless::render(&native, 1000, 900);
    // (it opens upwards: its track is 10px under its top)
    let top = ui.get_preview_volume_y();
    // (the pointer moves up the track, then presses)
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(x + 12.0, top + 10.0),
    });
    assert!(ui.get_preview_volume_open());
    for event in [
        WindowEvent::PointerPressed {
            position: at(x + 12.0, top + 10.0),
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: at(x + 12.0, top + 10.0),
            button: PointerEventButton::Left,
        },
    ] {
        ui.window().dispatch_event(event);
    }
    // the preview has its own volume: the loudest is the preview's
    assert_eq!(p.settings().preview_volume, 100);
    assert_eq!(p.settings().global_volume, 70);
    assert_eq!(ui.get_preview_volume(), 100);
    // the row under the track is the preview's own mute
    let click = |y: f32| {
        for event in [
            WindowEvent::PointerMoved {
                position: at(x + 12.0, y),
            },
            WindowEvent::PointerPressed {
                position: at(x + 12.0, y),
                button: PointerEventButton::Left,
            },
            WindowEvent::PointerReleased {
                position: at(x + 12.0, y),
                button: PointerEventButton::Left,
            },
        ] {
            ui.window().dispatch_event(event);
        }
    };
    assert!(!p.settings().preview_mute);
    click(top + 10.0 + 100.0 + 10.0 + 10.0);
    assert!(p.settings().preview_mute);
    assert!(ui.get_preview_muted());
    assert!(!p.settings().global_mute);
    // its bottom row is the global mute
    let bottom = top + 10.0 + 100.0 + 10.0 + 20.0 + 10.0;
    ui.window().dispatch_event(WindowEvent::PointerMoved {
        position: at(x + 12.0, bottom),
    });
    for event in [
        WindowEvent::PointerPressed {
            position: at(x + 12.0, bottom),
            button: PointerEventButton::Left,
        },
        WindowEvent::PointerReleased {
            position: at(x + 12.0, bottom),
            button: PointerEventButton::Left,
        },
    ] {
        ui.window().dispatch_event(event);
    }
    assert!(p.settings().global_mute);
    p.until(|| p.playing().1 == (100, true));
}

#[test]
fn the_previews_mute_buttons_show_the_references_tooltips_once_the_pointer_rests_on_them() {
    use slint::platform::WindowEvent;
    let recording = hydrus_testkit::fixture_json("tag_filter_tooltips.json");
    let p = Preview::new();
    let ui = &p.client.ui;
    p.select("audio.mp3");
    p.until(|| ui.get_preview_volume_shown());
    assert_eq!(
        ui.get_preview_global_tooltip(),
        recording["global_mute"].as_str().unwrap()
    );
    assert_eq!(
        ui.get_preview_mute_tooltip(),
        recording["preview_mute"].as_str().unwrap()
    );
    let native = p.client.native();
    headless::render(&native, 1000, 900);
    let at = |x: f32, y: f32| slint::LogicalPosition::new(x, y);
    let (x, y) = (ui.get_preview_volume_x(), ui.get_preview_volume_y());
    // the pointer opens the control (upwards from the global mute), moves to one
    // of its parts and rests there: the picture of the corner of the pane beside it
    let rests = |zone: f32| {
        ui.window().dispatch_event(WindowEvent::PointerExited);
        headless::render(&native, 1000, 900);
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: at(x + 12.0, y + 10.0),
        });
        assert!(ui.get_preview_volume_open());
        headless::render(&native, 1000, 900);
        let top = ui.get_preview_volume_y();
        // (one move only: each move restarts the tooltip's delay)
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: at(x + 12.0, top + zone),
        });
        for _ in 0..8 {
            std::thread::sleep(Duration::from_millis(100));
            slint::platform::update_timers_and_animations();
        }
        let picture = headless::render(&native, 1000, 900);
        let mut corner = Vec::new();
        for row in 760..900 {
            corner.extend_from_slice(&picture[(row * 1000 + 300) * 4..(row * 1000 + 700) * 4]);
        }
        corner
    };
    // the slider's track has no tooltip; each mute button draws its own (the
    // preview's own mute is at the top of the open control, the global one at its
    // bottom), and the recording says what each says
    let track = rests(60.0);
    let own = rests(130.0);
    let global = rests(150.0);
    assert_ne!(track, own);
    assert_ne!(track, global);
    assert_ne!(own, global);
    assert_eq!(track, rests(60.0));
}
