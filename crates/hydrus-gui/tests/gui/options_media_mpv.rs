//! The mpv box of the media playback page against the reference's
//! `MediaPlaybackPanel`: what its rows set mpv to as a file loads, and the
//! mpv.conf it can put in place. libmpv is not in the sandbox, so the
//! commands a player is sent are those of the same plan the player runs,
//! and mpv itself is not driven.

use hydrus_gui_model::mpv_options::Plan;
use hydrus_store::reference_options::ReferenceOptions;

use crate::options_gui_support::{Client, box_of, row, show_page};

const CONF: &str = "Set a new mpv.conf on dialog ok?:";
const DEVICE: &str = "Preferred audio output device:";
const NULL_AUDIO: &str = "DEBUG: Set null audio device on silent media:";
const LOOP_PLAYLIST: &str = "DEBUG: Loop Playlist instead of Loop File in mpv:";

fn set(name: &str, value: &str) -> Vec<String> {
    vec!["set".into(), name.into(), value.into()]
}

fn plan(client: &Client, has_audio: bool) -> Plan {
    Plan::for_file(&client.setting::<ReferenceOptions>(), has_audio)
}

// leaf: audit-options-media-playback-mpv-debug-loop-playlist-instead-of-loop-file-in-mpv
// leaf: audit-options-media-playback-mpv-preferred-audio-output-device
// leaf: audit-options-media-playback-mpv-debug-set-null-audio-device-on-silent-media
#[test]
fn the_mpv_rows_set_what_a_player_is_told_as_a_file_loads() {
    let client = Client::basic();
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
    assert_eq!(
        plan(&client, true).commands(),
        [
            set("loop", "inf"),
            set("loop-playlist", "no"),
            set("audio-device", "auto")
        ]
    );

    let options = client.open_options();
    show_page(&options, "media playback");
    options.invoke_none_toggled(row(&options, DEVICE).0, false);
    options.invoke_text_edited(row(&options, DEVICE).0, "alsa/hw:1".into());
    options.invoke_check_toggled(row(&options, LOOP_PLAYLIST).0, true);
    options.invoke_check_toggled(row(&options, NULL_AUDIO).0, true);
    // staged until OK
    options.invoke_cancel();
    assert_eq!(plan(&client, true).audio_device, "auto");

    let options = client.open_options();
    show_page(&options, "media playback");
    options.invoke_none_toggled(row(&options, DEVICE).0, false);
    options.invoke_text_edited(row(&options, DEVICE).0, "alsa/hw:1".into());
    options.invoke_check_toggled(row(&options, LOOP_PLAYLIST).0, true);
    options.invoke_check_toggled(row(&options, NULL_AUDIO).0, true);
    options.invoke_apply();
    // a file with sound goes to the preferred device and loops the playlist
    assert_eq!(
        plan(&client, true).commands(),
        [
            set("loop", "no"),
            set("loop-playlist", "inf"),
            set("audio-device", "alsa/hw:1")
        ]
    );
    // a silent one to the null device
    assert_eq!(plan(&client, false).audio_device, "null");

    // none again: mpv's own choice
    let options = client.open_options();
    show_page(&options, "media playback");
    options.invoke_none_toggled(row(&options, DEVICE).0, true);
    options.invoke_check_toggled(row(&options, NULL_AUDIO).0, false);
    options.invoke_apply();
    assert_eq!(plan(&client, false).audio_device, "auto");
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
