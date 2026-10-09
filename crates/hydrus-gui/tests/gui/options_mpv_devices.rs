//! Options > media playback > mpv > "--Fetch list of mpv audio device
//! strings:" against `oracle/record_mpv_audio_devices.py`: the real button
//! in the real options window asks for mpv's device list (stubbed, parsed
//! from mpv's own JSON as the recording gives it), offers the chooser, and
//! the choice fills "Preferred audio output device:", which Apply saves.

use std::sync::Arc;

use slint::{ComponentHandle as _, Model as _};

use hydrus_gui::{MainWindow, OptionsWindow, Pages, bind, headless};
use hydrus_gui_model::mpv_audio_devices as model;
use hydrus_store::Store;
use hydrus_store::reference_options::ReferenceOptions;

const DEVICE: &str = "Preferred audio output device:";

fn recorded_devices() -> Option<Vec<model::Device>> {
    let recording = hydrus_testkit::fixture_json("mpv_audio_devices.json");
    Some(model::parse(
        recording["device_list_json"].as_str().unwrap(),
    ))
}

fn no_mpv() -> Option<Vec<model::Device>> {
    None
}

fn device(store: &Store) -> Option<String> {
    store
        .read(hydrus_store::settings::get::<ReferenceOptions>)
        .unwrap()
        .string("mpv_preferred_audio_device")
}

fn open_media_playback(ui: &MainWindow, bound: &hydrus_gui::Bound) -> OptionsWindow {
    ui.invoke_menu_title_pressed(0, 20.0, 22.0);
    let lines = ui.get_menu_panes().row_data(0).unwrap().lines;
    let at = (0..lines.row_count())
        .position(|i| lines.row_data(i).unwrap().label == "options\u{2026}")
        .unwrap();
    ui.invoke_menu_line_clicked(0, i32::try_from(at).unwrap(), 0.0, 0.0, 0.0);
    let options = bound.options.borrow().as_ref().unwrap().clone_strong();
    let page = options
        .get_pages()
        .iter()
        .position(|p| p.text == "media playback")
        .unwrap() as i32;
    options.set_page(page);
    options.invoke_page_chosen(page);
    options
}

fn row(options: &OptionsWindow, label: &str) -> (i32, hydrus_gui::OptionRow) {
    let rows = options.get_rows();
    (0..rows.row_count())
        .map(|i| (i32::try_from(i).unwrap(), rows.row_data(i).unwrap()))
        .find(|(_, r)| r.label == label)
        .unwrap_or_else(|| panic!("{label:?}"))
}

// leaf: audit-options-media-playback-mpv-fetch-list-of-mpv-audio-device-strings
#[test]
fn the_fetch_button_offers_mpv_s_devices_and_fills_the_preferred_device_as_recorded() {
    let recording = hydrus_testkit::fixture_json("mpv_audio_devices.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let _windows = headless::init();
    let ui = MainWindow::new().unwrap();
    ui.show().unwrap();
    let bound = bind(&ui, Pages::single(super::common::all_local_page(store.clone())));

    for run in recording["runs"].as_array().unwrap() {
        let what = run.to_string();
        let available = run["available"].as_bool().unwrap();
        let start = run["start"].as_str().map(str::to_owned);
        store
            .write({
                let start = start.clone();
                move |ctx| {
                    let mut options: ReferenceOptions = hydrus_store::settings::get(ctx.conn())?;
                    options.set_string("mpv_preferred_audio_device", start);
                    hydrus_store::settings::set(ctx.conn(), &options)
                }
            })
            .unwrap();
        hydrus_gui::options_mpv_devices::set_fetch(if available {
            recorded_devices
        } else {
            no_mpv
        });
        let options = open_media_playback(&ui, &bound);
        let (_, button) = row(&options, "--Fetch list of mpv audio device strings:");
        assert_eq!(button.text, recording["button"].as_str().unwrap());
        assert_eq!(button.kind, 40);

        options.invoke_fetch_mpv_audio_devices_clicked();
        let log = run["log"].as_array().unwrap();
        let dialog = hydrus_gui::options_mpv_devices::last_dialog().expect("something is shown");
        assert!(dialog.window().is_visible(), "{what}");
        if let Some(info) = log.iter().find_map(|e| e["information"].as_str()) {
            assert_eq!(dialog.get_message(), info, "{what}");
            assert_eq!(dialog.get_choices().row_count(), 0);
            dialog.invoke_cancelled();
        } else {
            let asked = &log[0];
            assert_eq!(dialog.get_window_title(), asked["chooser"].as_str().unwrap());
            assert_eq!(dialog.get_message(), asked["message"].as_str().unwrap());
            let shown: Vec<String> = dialog.get_choices().iter().map(|c| c.to_string()).collect();
            let recorded: Vec<&str> = asked["choices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c[0].as_str().unwrap())
                .collect();
            assert_eq!(shown, recorded, "{what}");
            match run["choice"].as_i64() {
                Some(i) => dialog.invoke_chosen(i32::try_from(i).unwrap()),
                None => dialog.invoke_cancelled(),
            }
        }
        // the draft, as the reference's text box holds it
        let (_, field) = row(&options, DEVICE);
        let value = run["value"].as_str();
        assert_eq!(field.is_none, value.is_none(), "{what}");
        if let Some(value) = value {
            assert_eq!(field.text, value, "{what}");
        }
        // nothing is saved until Apply
        assert_eq!(device(&store), start, "{what}");
        options.invoke_apply();
        assert_eq!(device(&store), value.map(str::to_owned), "{what}");
    }
}
