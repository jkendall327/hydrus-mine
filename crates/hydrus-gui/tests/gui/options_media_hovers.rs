//! The media viewer hovers page (and the audio label) against their
//! consumers: each option is changed in the real options window, applied,
//! and the viewer's top hover line and the status bar are what the saved
//! value makes them.

use slint::ComponentHandle as _;

use hydrus_core::media_viewer::InfoLineSettings;
use hydrus_gui::info_lines::top_line_with_format;

use crate::options_gui_support::{Client, box_of, row, show_page};

/// Show all the files.
fn search_everything(client: &Client, trash: bool) {
    if trash {
        let roles =
            hydrus_store::content::DomainRoles::new(&client.store.snapshot().services).unwrap();
        let key = client
            .store
            .snapshot()
            .services
            .get(roles.trash)
            .unwrap()
            .key
            .clone();
        client
            .bound
            .current
            .borrow()
            .borrow_mut()
            .choose_location(hydrus_search::LocationContext::single(key));
    }
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
}

/// Every file of `system:everything`'s top hover line, read from the real
/// viewer opened on each in turn.
fn viewer_lines(client: &Client) -> Vec<String> {
    let count = client.bound.current.borrow().borrow().results().len();
    assert!(count > 0, "files are shown");
    (0..count)
        .map(|i| {
            client.ui.invoke_thumbnail_activated(i as i32);
            let viewer = client
                .bound
                .viewer
                .borrow()
                .as_ref()
                .map(slint::ComponentHandle::clone_strong)
                .expect("the viewer opens");
            let line = viewer.get_info_line().to_string();
            viewer.invoke_close_requested();
            line
        })
        .collect()
}

/// A line with how long ago each time was left out (the viewer reads the
/// clock as it is, the recording as it was).
fn untimed(line: &str) -> String {
    let is_time = |word: &str| {
        word.parse::<u64>().is_ok()
            || ["second", "minute", "hour", "day", "week", "month", "year"]
                .iter()
                .any(|unit| word.trim_end_matches('s') == *unit)
    };
    let mut words: Vec<&str> = line.split(' ').collect();
    let mut at = 0;
    while at < words.len() {
        if words[at] == "ago" {
            let mut start = at;
            while start > 0 && is_time(words[start - 1]) {
                start -= 1;
            }
            words.splice(start..=at, ["<time>"]);
            at = start;
        }
        at += 1;
    }
    words.join(" ")
}

fn untimed_all(lines: &[String]) -> Vec<String> {
    lines.iter().map(|l| untimed(l)).collect()
}

/// The top hover line the reference made for each file shown (by hash) in the
/// recording's `phase` (`oracle/record_info_lines.py`).
fn recorded_lines(client: &Client, phase: &str) -> Vec<String> {
    let fixture = hydrus_testkit::fixture_json("info_lines.json");
    let files = client.bound.current.borrow().borrow().results().to_vec();
    let hashes = client
        .store
        .read(|c| hydrus_store::master::hashes(c, &files))
        .unwrap();
    files
        .iter()
        .map(|file| {
            fixture["phases"][phase]["files"][hashes[file].to_hex()]["top"]
                .as_str()
                .unwrap_or_else(|| panic!("{phase}: {file:?}"))
                .to_owned()
        })
        .collect()
}

/// The lines the reference's rule makes with these settings.
fn expected_lines(client: &Client, settings: &InfoLineSettings) -> Vec<String> {
    let snapshot = client.store.snapshot();
    let files = client.bound.current.borrow().borrow().results().to_vec();
    files
        .iter()
        .map(|file| {
            let media = client
                .store
                .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[*file]))
                .unwrap()
                .results
                .remove(0);
            top_line_with_format(
                &media,
                &snapshot.services,
                settings,
                hydrus_core::TimestampMs::now().0,
                &hydrus_gui_model::gui_format::preferences(&client.store),
            )
        })
        .collect()
}

/// Flip one check of `page`, as the reference lays it out, and apply.
fn flip(client: &Client, page: &str, box_title: &str, label: &str) {
    let options = client.open_options();
    show_page(&options, page);
    let (i, found) = row(&options, label);
    assert_eq!(found.kind, 1, "{label:?} is a checkbox");
    assert_eq!(box_of(&options, label), box_title);
    options.invoke_check_toggled(i, !found.checked);
    options.invoke_apply();
    options.hide().unwrap();
}

/// Where an option shows: in the trash, or with the file services shown.
#[derive(Clone, Copy, Default)]
struct Base {
    trash: bool,
    services: bool,
}

/// Flip the summary option `label` (and `field` of the settings with it) and
/// compare the viewer's lines with the rule's. The line must change for some file.
fn summary_option(
    label: &str,
    field: fn(&mut InfoLineSettings) -> &mut bool,
    base: Base,
    phases: (&str, &str),
) {
    let client = Client::basic();
    search_everything(&client, base.trash);
    let defaults = InfoLineSettings {
        file_services_interesting: base.services,
        ..InfoLineSettings::default()
    };
    let start = defaults.clone();
    client
        .store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &start))
        .unwrap();
    let before = viewer_lines(&client);
    assert_eq!(before, expected_lines(&client, &defaults));
    assert_eq!(
        untimed_all(&before),
        untimed_all(&recorded_lines(&client, phases.0)),
        "before"
    );
    flip(
        &client,
        "media viewer hovers",
        "top hover file summary",
        label,
    );
    let mut flipped = defaults.clone();
    *field(&mut flipped) = !*field(&mut flipped);
    let saved: InfoLineSettings = client.setting();
    assert_eq!(saved, flipped, "saved");
    let after = viewer_lines(&client);
    assert_eq!(after, expected_lines(&client, &saved));
    // and they are the lines the reference made with that option turned over
    assert_eq!(
        untimed_all(&after),
        untimed_all(&recorded_lines(&client, phases.1)),
        "after"
    );
    assert_ne!(before, after, "{label:?} changes the lines");
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-show-archived-status
#[test]
fn show_archived_status() {
    summary_option(
        "Show archived status: ",
        |s| &mut s.archived_interesting,
        Base::default(),
        (
            "defaults",
            "only file_info_line_consider_archived_interesting",
        ),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-show-archived-time
#[test]
fn show_archived_time() {
    summary_option(
        "Show archived time: ",
        |s| &mut s.archived_time_interesting,
        Base::default(),
        (
            "defaults",
            "only file_info_line_consider_archived_time_interesting",
        ),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-show-file-services
#[test]
fn show_file_services() {
    summary_option(
        "Show file services: ",
        |s| &mut s.file_services_interesting,
        Base::default(),
        (
            "defaults",
            "only file_info_line_consider_file_services_interesting",
        ),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-show-file-service-add-times
#[test]
fn show_file_service_add_times() {
    summary_option(
        "Show file service add times: ",
        |s| &mut s.file_services_import_times_interesting,
        Base {
            services: true,
            ..Base::default()
        },
        (
            "only file_info_line_consider_file_services_interesting",
            "services and their add times",
        ),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-show-file-trash-times
#[test]
fn show_file_trash_times() {
    summary_option(
        "Show file trash times: ",
        |s| &mut s.trash_time_interesting,
        Base {
            trash: true,
            ..Base::default()
        },
        (
            "defaults",
            "only file_info_line_consider_trash_time_interesting",
        ),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-show-file-trash-reasons
#[test]
fn show_file_trash_reasons() {
    summary_option(
        "Show file trash reasons: ",
        |s| &mut s.trash_reason_interesting,
        Base {
            trash: true,
            ..Base::default()
        },
        (
            "defaults",
            "only file_info_line_consider_trash_reason_interesting",
        ),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-hide-uninteresting-modified-times
#[test]
fn hide_uninteresting_modified_times() {
    summary_option(
        "Hide uninteresting modified times: ",
        |s| &mut s.hide_uninteresting_modified_time,
        Base::default(),
        ("defaults", "only hide_uninteresting_modified_time"),
    );
}

// leaf: audit-options-media-viewer-hovers-top-hover-file-summary-swap-in-common-resolution-labels
#[test]
fn swap_in_common_resolution_labels() {
    summary_option(
        "Swap in common resolution labels:",
        |s| &mut s.nice_resolutions,
        Base::default(),
        ("defaults", "only use_nice_resolution_strings"),
    );
}

// leaf: audit-options-audio-label-for-files-with-audio
#[test]
fn the_audio_label_is_the_text_the_viewer_uses_for_files_with_audio() {
    let client = Client::basic();
    search_everything(&client, false);
    let before = viewer_lines(&client);
    let audible =
        |lines: &[String], label: &str| lines.iter().filter(|l| l.contains(label)).count();
    assert_eq!(audible(&before, "\u{1F50A}"), 2, "the default label");

    let options = client.open_options();
    show_page(&options, "audio");
    let (i, found) = row(&options, "Label for files with audio: ");
    assert_eq!(box_of(&options, "Label for files with audio: "), "");
    assert_eq!(found.text, "\u{1F50A}", "the reference's default");
    options.invoke_text_edited(i, "with audio".into());
    // nothing before apply
    assert_eq!(
        client.setting::<InfoLineSettings>().has_audio_label,
        "\u{1F50A}"
    );
    options.invoke_apply();
    options.hide().unwrap();

    let saved: InfoLineSettings = client.setting();
    assert_eq!(saved.has_audio_label, "with audio");
    let after = viewer_lines(&client);
    assert_eq!(audible(&after, "\u{1F50A}"), 0);
    assert_eq!(after.len(), before.len());
    assert_eq!(audible(&after, ", with audio"), 2, "{after:?}");
    assert_eq!(after, expected_lines(&client, &saved));
}

// leaf: audit-options-thumbnails-interaction-when-a-single-thumbnail-is-selected-show-the-media-viewer-s-normal-top-hover-file-text-in-the-status
#[test]
fn a_single_selected_thumbnail_shows_the_top_hover_text_in_the_status_bar_if_asked() {
    const LABEL: &str = "When a single thumbnail is selected, show the media viewer's normal top hover file text in the status bar: ";
    let client = Client::basic();
    search_everything(&client, false);
    let status = |client: &Client| client.bound.current.borrow().borrow().status();
    client.ui.invoke_thumbnail_clicked(0, false, false);
    let on = status(&client);
    let file = client.bound.current.borrow().borrow().selected_files()[0];
    let snapshot = client.store.snapshot();
    let media = client
        .store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &[file]))
        .unwrap()
        .results
        .remove(0);
    let line = hydrus_gui::info_lines::status_line_with_format(
        &media,
        &snapshot.services,
        &InfoLineSettings::default(),
        hydrus_core::TimestampMs::now().0,
        &hydrus_gui_model::gui_format::preferences(&client.store),
    );
    assert!(on.contains(&line), "{on:?} has {line:?}");

    let options = client.open_options();
    show_page(&options, "thumbnails");
    let (i, found) = row(&options, LABEL);
    assert_eq!((found.kind, found.checked), (1, true));
    assert_eq!(box_of(&options, LABEL), "interaction");
    options.invoke_check_toggled(i, false);
    assert!(status(&client).contains(&line), "not before apply");
    options.invoke_apply();
    options.hide().unwrap();
    let saved: InfoLineSettings = client.setting();
    assert!(!saved.single_file_in_status_bar);
    let off = status(&client);
    assert!(!off.contains(&line), "{off:?}");
    assert!(off.len() < on.len());
}
