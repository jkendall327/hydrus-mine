//! The mpv box of the media playback page against the reference's
//! `MediaPlaybackPanel`: what its rows set mpv to as a file loads, and the
//! mpv.conf it can put in place. The rows are checked on the media viewer's
//! own mpv player (skipped where libmpv is missing).

use hydrus_store::reference_options::ReferenceOptions;

use crate::options_gui_support::{Client, box_of, row, show_page};

const CONF: &str = "Set a new mpv.conf on dialog ok?:";
const DEVICE: &str = "Preferred audio output device:";
const NULL_AUDIO: &str = "DEBUG: Set null audio device on silent media:";
const LOOP_PLAYLIST: &str = "DEBUG: Loop Playlist instead of Loop File in mpv:";

/// mpv's properties as the recording has them (python-mpv reads `no` as
/// false).
fn recorded(player: &serde_json::Value) -> [String; 3] {
    let text = |key: &str| match &player[key] {
        serde_json::Value::String(s) if s == "False" => "no".to_owned(),
        serde_json::Value::String(s) => s.clone(),
        other => panic!("{key}: {other}"),
    };
    [
        text("loop-file"),
        text("loop-playlist"),
        text("audio-device"),
    ]
}

/// The properties of the `n`th player playing (oldest first), once it has
/// them: they are set as the file loads, and an open player looks at the
/// options twice a second.
fn player_has(n: usize, expected: &[String; 3]) -> Option<[String; 3]> {
    let read = || {
        let get = |name: &str| {
            hydrus_gui::live_mpv_property(name)
                .get(n)
                .cloned()
                .flatten()
                .unwrap_or_default()
        };
        [get("loop-file"), get("loop-playlist"), get("audio-device")]
    };
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_secs(10) {
        slint::platform::update_timers_and_animations();
        if read() == *expected {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Some(read())
}

// leaf: audit-options-media-playback-mpv-debug-loop-playlist-instead-of-loop-file-in-mpv
// leaf: audit-options-media-playback-mpv-preferred-audio-output-device
#[test]
fn the_mpv_rows_set_what_a_player_is_told_as_a_file_loads() {
    use crate::options_media_support::Media;

    if hydrus_gui::mpv::skip_without_libmpv() {
        return;
    }
    // what the reference's players are set to (record_mpv_playback_options.py)
    let recording = hydrus_testkit::fixture_json("mpv_playback_options.json");
    let client = Media::basic();
    let importer =
        hydrus_import::FileImporter::new(client.store.clone(), hydrus_media::MediaTools::new());
    let imported = importer
        .import_path(
            &hydrus_testkit::fixture_path("media/gif_anim.gif"),
            &hydrus_import::FileImportOptions::default(),
        )
        .unwrap();
    let hash = imported.hash.expect("imported");
    let id = client
        .store
        .read(move |c| hydrus_store::master::hash_ids(c, &[hash]))
        .unwrap()
        .into_values()
        .next()
        .unwrap();
    client.search("system:everything");
    let at = client
        .results()
        .iter()
        .position(|file| *file == id)
        .unwrap();
    let open_viewer = || {
        client
            .ui
            .invoke_thumbnail_activated(i32::try_from(at).unwrap());
        client
            .bound
            .viewer
            .borrow()
            .as_ref()
            .map(slint::ComponentHandle::clone_strong)
            .expect("the viewer opens")
    };

    let options = client.open_options();
    show_page(&options, "media playback");
    for label in [CONF, DEVICE, NULL_AUDIO, LOOP_PLAYLIST] {
        assert_eq!(box_of(&options, label), "mpv", "{label}");
    }
    // the reference's defaults: nothing preferred, nothing special
    assert!(row(&options, DEVICE).1.is_none);
    assert!(!row(&options, NULL_AUDIO).1.checked);
    assert!(!row(&options, LOOP_PLAYLIST).1.checked);
    options.invoke_cancel();

    // the animation in the media viewer: its player as the reference's starts
    let _first = open_viewer();
    let start = recorded(&recording["open_player_at_start"]);
    assert_eq!(player_has(0, &start), None, "at start");

    // typed but cancelled: nothing changes
    let options = client.open_options();
    show_page(&options, "media playback");
    options.invoke_none_toggled(row(&options, DEVICE).0, false);
    options.invoke_text_edited(row(&options, DEVICE).0, "alsa/hw:1".into());
    options.invoke_check_toggled(row(&options, LOOP_PLAYLIST).0, true);
    options.invoke_cancel();
    std::thread::sleep(std::time::Duration::from_millis(600));
    assert_eq!(player_has(0, &start), None, "cancelled");

    for case in recording["cases"].as_array().unwrap() {
        let device = case["typed"]["device"].as_str();
        let loop_playlist = case["typed"]["loop_playlist"].as_bool().unwrap();
        let options = client.open_options();
        show_page(&options, "media playback");
        let (i, _) = row(&options, DEVICE);
        options.invoke_none_toggled(i, device.is_none());
        if let Some(device) = device {
            options.invoke_text_edited(i, device.into());
        }
        options.invoke_check_toggled(row(&options, LOOP_PLAYLIST).0, loop_playlist);
        options.invoke_apply();
        let saved = client.setting::<ReferenceOptions>();
        assert_eq!(
            saved.string("mpv_preferred_audio_device").as_deref(),
            case["saved"]["mpv_preferred_audio_device"].as_str()
        );
        assert_eq!(
            saved.boolean("mpv_loop_playlist_instead_of_file"),
            case["saved"]["mpv_loop_playlist_instead_of_file"]
                .as_bool()
                .unwrap()
        );
        // the player already open follows, as the reference's on OK
        assert_eq!(
            player_has(0, &recorded(&case["open_player"])),
            None,
            "open player: {case}"
        );
    }

    // and a player made now, in another viewer, starts so
    let last = recording["cases"].as_array().unwrap().last().unwrap();
    let _second = open_viewer();
    assert_eq!(hydrus_gui::live_mpv_property("loop-file").len(), 2);
    assert_eq!(
        player_has(1, &recorded(&last["new_player"])),
        None,
        "new player"
    );
}

// leaf: audit-options-media-playback-mpv-set-a-new-mpv-conf-on-dialog-ok
#[test]
fn a_new_mpv_conf_replaces_the_databases_on_ok_and_nothing_is_kept() {
    let client = Client::basic();
    let conf = client.store.dir().join("mpv.conf");
    let source = tempfile::tempdir().unwrap();
    let mine = source.path().join("mine.conf");
    std::fs::write(&mine, "volume-max=150\n").unwrap();

    // typed but cancelled: nothing is copied
    let options = client.open_options();
    show_page(&options, "media playback");
    let (at, shown) = row(&options, CONF);
    assert_eq!((shown.kind, shown.text.as_str()), (20, ""));
    options.invoke_text_edited(at, mine.to_string_lossy().as_ref().into());
    options.invoke_cancel();
    assert_ne!(
        std::fs::read_to_string(&conf).ok().as_deref(),
        Some("volume-max=150\n")
    );

    // on OK, the file goes over the database's mpv.conf
    let options = client.open_options();
    show_page(&options, "media playback");
    options.invoke_text_edited(
        row(&options, CONF).0,
        mine.to_string_lossy().as_ref().into(),
    );
    options.invoke_apply();
    assert_eq!(std::fs::read_to_string(&conf).unwrap(), "volume-max=150\n");

    // the path is not remembered: reopened, the row is blank
    let options = client.open_options();
    show_page(&options, "media playback");
    assert_eq!(row(&options, CONF).1.text, "");
    options.invoke_cancel();

    // a path that is no file changes nothing
    std::fs::write(&conf, "mine\n").unwrap();
    let options = client.open_options();
    show_page(&options, "media playback");
    options.invoke_text_edited(row(&options, CONF).0, "/no/such/mpv.conf".into());
    options.invoke_apply();
    assert_eq!(std::fs::read_to_string(&conf).unwrap(), "mine\n");
}
