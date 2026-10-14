//! "Enable file viewing statistics tracking?:" set in the real options window and
//! met by the real media viewer, replaying the reference's `canvas_events`
//! (`oracle/fixtures/viewing_statistics_options.json`): with tracking off a
//! viewed file leaves no row; with it on it leaves one view of the capped
//! time on the media viewer's canvas.

use slint::ComponentHandle as _;

use hydrus_core::CanvasType;
use hydrus_store::Store;

use super::options_system_consumers::{check, client, row};

fn media_views(store: &Store, file: hydrus_core::HashId) -> Option<(u64, u64)> {
    store
        .read(|conn| hydrus_store::media::viewing_stats(conn, &[file]))
        .unwrap()
        .into_iter()
        .find(|s| s.canvas == CanvasType::MediaViewer)
        .map(|s| (s.views, s.viewtime_ms))
}

// leaf: audit-options-file-viewing-statistics-enable-file-viewing-statistics-tracking
#[test]
fn the_tracking_row_decides_whether_the_real_media_viewer_records_a_view_as_recorded() {
    let recorded = hydrus_testkit::fixture_json("viewing_statistics_options.json");
    let events = recorded["canvas_events"].as_array().unwrap();
    assert_eq!(events[0]["active"], false);
    assert_eq!(events[0]["rows"], serde_json::json!([]));
    assert_eq!(events[1]["active"], true);
    let row_recorded = &events[1]["rows"][0];
    let canvas = &row_recorded[0];
    let (views, viewtime) = (&row_recorded[1][1], &row_recorded[1][2]);
    assert_eq!(*canvas, 0, "the media viewer's canvas");

    let client = client();
    let file = client
        .store
        .read(|conn| {
            hydrus_store::master::hash_id(
                conn,
                &recorded["file"].as_str().unwrap().parse().unwrap(),
            )
        })
        .unwrap()
        .unwrap();
    client.ui.show().unwrap();
    client.ui.invoke_search_edited("system:everything".into());
    client.ui.invoke_search_accepted();
    let index = i32::try_from(
        client
            .bound
            .current
            .borrow()
            .borrow()
            .results()
            .iter()
            .position(|id| *id == file)
            .unwrap(),
    )
    .unwrap();

    // (the recording counts every view, capped at one second)
    let window = client.options("file viewing statistics");
    let (minimum, _) = row(
        &window,
        "Min time to view on media viewer to count as a view:",
    );
    let (maximum, _) = row(
        &window,
        "Cap any view on the media viewer to this maximum time:",
    );
    window.invoke_none_toggled(minimum, true);
    for field in 0..4 {
        window.invoke_field_edited(maximum, field, 0);
    }
    let label = "Enable file viewing statistics tracking?:";
    assert!(
        row(&window, label).1.checked,
        "on by default, as the reference"
    );
    window.invoke_apply();

    let view = || {
        client.ui.invoke_thumbnail_activated(index);
        let viewer = client
            .bound
            .viewer
            .borrow()
            .as_ref()
            .unwrap()
            .clone_strong();
        std::thread::sleep(std::time::Duration::from_millis(1050));
        viewer.invoke_close_requested();
    };
    for event in events {
        let window = client.options("file viewing statistics");
        check(&window, label, event["active"].as_bool().unwrap());
        window.invoke_apply();
        let before = media_views(&client.store, file).unwrap_or((0, 0));
        view();
        let after = media_views(&client.store, file).unwrap_or((0, 0));
        if event["rows"].as_array().unwrap().is_empty() {
            assert_eq!(after, before, "tracking off: nothing recorded");
        } else {
            assert_eq!(
                (after.0 - before.0, after.1 - before.1),
                (views.as_u64().unwrap(), viewtime.as_u64().unwrap()),
                "tracking on: one view, capped"
            );
        }
    }
}
