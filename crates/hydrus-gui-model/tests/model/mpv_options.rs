//! What the reference's mpv players are set to from the options
//! (`MPVWidget.UpdateConfAndCoreOptions`, `SetAudioDeviceFromOptions`, and
//! the null audio device for silent media), worked from its source: libmpv is
//! not in the sandbox to record with.
use hydrus_gui_model::mpv_options::Plan;
use hydrus_store::reference_options::ReferenceOptions;

fn command(name: &str, value: &str) -> Vec<String> {
    vec!["set".into(), name.into(), value.into()]
}

#[test]
fn the_defaults_loop_the_file_on_the_default_device() {
    let plan = Plan::for_file(&ReferenceOptions::default(), true);
    assert_eq!(
        plan.commands(),
        [
            command("loop", "inf"),
            command("loop-playlist", "no"),
            command("audio-device", "auto"),
        ]
    );
}

#[test]
fn looping_the_playlist_and_a_preferred_device_set_what_the_reference_sets() {
    let mut options = ReferenceOptions::default();
    options.set_boolean("mpv_loop_playlist_instead_of_file", true);
    options.set_string("mpv_preferred_audio_device", Some("alsa/hw:1".into()));
    let plan = Plan::for_file(&options, true);
    assert_eq!(
        plan.commands(),
        [
            command("loop", "no"),
            command("loop-playlist", "inf"),
            command("audio-device", "alsa/hw:1"),
        ]
    );
}

#[test]
fn silent_media_gets_the_null_device_only_when_asked() {
    let mut options = ReferenceOptions::default();
    options.set_string("mpv_preferred_audio_device", Some("pulse".into()));
    // not asked: the preferred device, sound or none
    assert_eq!(Plan::for_file(&options, false).audio_device, "pulse");
    options.set_boolean("mpv_null_audio_on_silent_media", true);
    assert_eq!(Plan::for_file(&options, false).audio_device, "null");
    // a file with sound goes back to the preferred device
    assert_eq!(Plan::for_file(&options, true).audio_device, "pulse");
}

#[test]
fn a_new_mpv_conf_is_copied_over_the_databases_when_it_names_a_file() {
    let directory = tempfile::tempdir().unwrap();
    let source = directory.path().join("mine.conf");
    let destination = directory.path().join("mpv.conf");
    std::fs::write(&source, "volume-max=150\n").unwrap();
    let copy = |source: &str| hydrus_gui_model::options::set_mpv_conf(source, &destination);
    // blank, or no such file, or a folder: nothing is copied
    assert_eq!(copy(""), Ok(false));
    assert_eq!(copy("/no/such/file.conf"), Ok(false));
    assert_eq!(copy(&directory.path().to_string_lossy()), Ok(false));
    assert!(!destination.exists());
    assert_eq!(copy(&source.to_string_lossy()), Ok(true));
    assert_eq!(
        std::fs::read_to_string(&destination).unwrap(),
        "volume-max=150\n"
    );
}
