//! The thumbnail and viewer metadata actions read local files, display the
//! reference's sections and copy raw values; non-local files skip disk I/O.
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use hydrus_core::Mime;
use hydrus_gui::{
    Bound, Clip, EmbeddedMetadataWindow, MainWindow, Pages, SearchPage, bind, headless, set_clipper,
};
use hydrus_store::Store;
use hydrus_store::import::import_legacy;
use slint::{ComponentHandle, Model as _};

fn dialog(bound: &Bound) -> EmbeddedMetadataWindow {
    bound
        .embedded_metadata
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .expect("metadata opens")
}

fn loaded(window: &EmbeddedMetadataWindow) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while window.get_loading() {
        assert!(
            std::time::Instant::now() < deadline,
            "metadata worker completed"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
    assert_eq!(window.get_error(), "");
}

fn thumbnail_open(ui: &MainWindow, bound: &Bound, index: i32) -> EmbeddedMetadataWindow {
    ui.invoke_thumbnail_clicked(index, false, false);
    ui.invoke_thumbnail_menu_requested(index);
    let menu = ui.get_thumbnail_menu();
    assert!(menu.info_is_menu && menu.info_metadata >= 0);
    ui.invoke_menu_chosen(menu.info_metadata);
    let window = dialog(bound);
    assert!(
        window.get_loading(),
        "basics shown before worker publication"
    );
    assert!(!window.get_basics().is_empty());
    loaded(&window);
    window
}

fn check_visibility(window: &EmbeddedMetadataWindow, reference: &serde_json::Value) {
    let visible = [
        true,
        window.get_has_exif(),
        window.get_has_xmp(),
        window.get_has_iptc(),
        window.get_has_text(),
        window.get_extra().row_count() > 0,
    ];
    let expected: Vec<bool> = reference["boxes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["visible"].as_bool().unwrap())
        .collect();
    assert_eq!(visible.as_slice(), expected);
}

fn check_rows(window: &EmbeddedMetadataWindow, sample: &str) {
    let fixture = hydrus_testkit::fixture_json("embedded_metadata.json");
    let expected = &fixture[sample];
    let rows = window.get_rows();
    let exif = expected["exif"].as_array().cloned().unwrap_or_default();
    assert_eq!(window.get_has_exif(), !expected["exif"].is_null());
    assert_eq!(rows.row_count(), exif.len());
    for (i, row) in exif.iter().enumerate() {
        let cells: Vec<String> = rows
            .row_data(i)
            .unwrap()
            .cells
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            cells,
            row.as_array().unwrap()[..3]
                .iter()
                .map(|s| s.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        );
    }
    let extras: Vec<String> = window.get_extra().iter().map(|s| s.to_string()).collect();
    assert_eq!(
        extras,
        expected["extra"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| format!("{}: {}", r[0].as_str().unwrap(), r[1].as_str().unwrap()))
            .collect::<Vec<_>>()
    );
    assert_eq!(window.get_has_xmp(), !expected["xmp"].is_null());
    assert_eq!(window.get_has_iptc(), !expected["iptc"].is_null());
    assert_eq!(window.get_has_text(), !expected["text"].is_null());
    assert_eq!(
        window.get_xmp(),
        expected["xmp"].as_str().unwrap_or_default()
    );
    assert_eq!(
        window.get_iptc(),
        expected["iptc"].as_str().unwrap_or_default()
    );
    assert_eq!(
        window.get_text(),
        expected["text"].as_str().unwrap_or_default()
    );
}

#[test]
#[allow(clippy::too_many_lines)]
fn metadata_opens_from_thumbnails_and_viewer_and_copies_raw_values() {
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let native = tempfile::tempdir().unwrap();
    import_legacy(
        legacy.path(),
        &native.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store: Arc<Store> = Store::open(native.path()).unwrap();
    let windows = headless::init();
    let ui = MainWindow::new().unwrap();
    let bound = bind(&ui, Pages::single(SearchPage::new(store.clone())));
    ui.invoke_search_edited("system:everything".into());
    ui.invoke_search_accepted();
    let files = bound.current.borrow().borrow().results().to_vec();
    let snapshot = store.snapshot();
    let media = store
        .read(|c| hydrus_store::media::load(c, &snapshot.services, None, &files))
        .unwrap()
        .results;
    let index = media
        .iter()
        .position(|r| r.info.as_ref().is_some_and(|i| i.mime == Mime::ImagePng) && r.inbox)
        .unwrap();
    let file = media[index].hash_id;
    let path = snapshot
        .storage
        .file_path(&media[index].hash, Mime::ImagePng)
        .unwrap();
    std::fs::copy(
        hydrus_testkit::fixture_path("metadata/embedded_exif_rich.png"),
        &path,
    )
    .unwrap();
    let index = i32::try_from(files.iter().position(|&f| f == file).unwrap()).unwrap();
    let window = thumbnail_open(&ui, &bound, index);
    let panel_fixture = hydrus_testkit::fixture_json("embedded_metadata_window.json");
    assert_eq!(
        window.get_instruction(),
        panel_fixture["cases"][1]["instruction"].as_str().unwrap()
    );
    assert!(window.get_basics().contains("png"));
    assert!(window.get_has_exif());
    check_rows(&window, "metadata/embedded_exif_rich.png");
    let copied = Rc::new(RefCell::new(Vec::new()));
    set_clipper({
        let copied = copied.clone();
        move |c| {
            if let Clip::Text(text) = c {
                copied.borrow_mut().push(text.clone());
            }
        }
    });
    // Activating an empty selection does nothing.
    window.invoke_row_activated(-1);
    assert!(copied.borrow().is_empty());
    let fixture = hydrus_testkit::fixture_json("embedded_metadata.json");
    let rows = fixture["metadata/embedded_exif_rich.png"]["exif"]
        .as_array()
        .unwrap();
    for id in ["271", "37510"] {
        let row = rows.iter().position(|r| r[0] == id).unwrap();
        window.invoke_row_clicked(i32::try_from(row).unwrap(), false, false);
        window.invoke_row_activated(i32::try_from(row).unwrap());
        assert_eq!(
            copied.borrow().last().unwrap(),
            rows[row][3].as_str().unwrap()
        );
    }
    window.invoke_sort(0, false);
    assert!(!window.get_ascending());
    assert_eq!(
        window
            .get_rows()
            .row_data(0)
            .unwrap()
            .cells
            .row_data(0)
            .unwrap(),
        rows.last().unwrap()[0].as_str().unwrap()
    );
    let (width, height) = (780, 1000);
    let pixels = headless::render(&windows.get(1).unwrap(), width, height);
    headless::save_png(
        &std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("embedded_metadata.png"),
        &pixels,
        width,
        height,
    )
    .unwrap();
    window.invoke_dismissed();
    assert!(bound.embedded_metadata.borrow().is_none());
    // A closed or superseded request must not publish into the new window.
    ui.invoke_thumbnail_menu_requested(index);
    let action = ui.get_thumbnail_menu().info_metadata;
    ui.invoke_menu_chosen(action);
    let old = dialog(&bound);
    assert!(old.get_loading());
    ui.invoke_menu_chosen(action);
    let replacement = dialog(&bound);
    old.invoke_dismissed();
    assert!(bound.embedded_metadata.borrow().is_some());
    loaded(&replacement);
    check_rows(&replacement, "metadata/embedded_exif_rich.png");
    replacement.invoke_dismissed();
    // Other optional sections are supplied by real files from the decoder oracle.
    for sample in ["metadata/png_xmp.png", "metadata/png_comment_plain.png"] {
        std::fs::copy(hydrus_testkit::fixture_path(sample), &path).unwrap();
        let window = thumbnail_open(&ui, &bound, index);
        check_rows(&window, sample);
        window.invoke_dismissed();
    }
    std::fs::copy(
        hydrus_testkit::fixture_path("metadata/embedded_exif_rich.png"),
        &path,
    )
    .unwrap();
    let jpeg = media
        .iter()
        .find(|r| r.info.as_ref().is_some_and(|i| i.mime == Mime::ImageJpeg))
        .unwrap();
    let jpeg_path = snapshot
        .storage
        .file_path(&jpeg.hash, Mime::ImageJpeg)
        .unwrap();
    let jpeg_index = i32::try_from(files.iter().position(|&f| f == jpeg.hash_id).unwrap()).unwrap();
    for sample in [
        "metadata/jpeg_iptc_caption.jpg",
        "metadata/jpeg_xmp_empty.jpg",
    ] {
        std::fs::copy(hydrus_testkit::fixture_path(sample), &jpeg_path).unwrap();
        let window = thumbnail_open(&ui, &bound, jpeg_index);
        check_rows(&window, sample);
        window.invoke_dismissed();
    }
    let pdf = media
        .iter()
        .find(|r| {
            r.info
                .as_ref()
                .is_some_and(|i| i.mime == Mime::ApplicationPdf)
        })
        .unwrap();
    let pdf_path = snapshot
        .storage
        .file_path(&pdf.hash, Mime::ApplicationPdf)
        .unwrap();
    let pdf_index = i32::try_from(files.iter().position(|&f| f == pdf.hash_id).unwrap()).unwrap();
    std::fs::copy(
        hydrus_testkit::fixture_path("media/pdf_text.pdf"),
        &pdf_path,
    )
    .unwrap();
    let window = thumbnail_open(&ui, &bound, pdf_index);
    assert_eq!(
        window.get_text(),
        panel_fixture["pdf"]["pdf_text.pdf"].as_str().unwrap()
    );
    assert!(window.get_has_text() && !window.get_has_exif());
    assert_eq!(window.get_extra().row_count(), 0);
    window.invoke_dismissed();
    std::fs::copy(
        hydrus_testkit::fixture_path("media/pdf_empty_title.pdf"),
        &pdf_path,
    )
    .unwrap();
    let window = thumbnail_open(&ui, &bound, pdf_index);
    check_visibility(&window, &panel_fixture["cases"][0]);
    window.invoke_dismissed();
    ui.invoke_thumbnail_activated(index);
    let viewer = bound
        .viewer
        .borrow()
        .as_ref()
        .map(ComponentHandle::clone_strong)
        .unwrap();
    viewer.invoke_context_menu_requested();
    assert!(viewer.get_context_menu().info_metadata >= 0);
    viewer.invoke_menu_chosen(viewer.get_context_menu().info_metadata);
    let window = dialog(&bound);
    loaded(&window);
    check_rows(&window, "metadata/embedded_exif_rich.png");
    window.invoke_dismissed();
    // Local disk failures preserve basics, finish loading and show an error.
    std::fs::remove_file(&path).unwrap();
    viewer.invoke_context_menu_requested();
    viewer.invoke_menu_chosen(viewer.get_context_menu().info_metadata);
    let failed = dialog(&bound);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while failed.get_loading() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
        slint::platform::update_timers_and_animations();
    }
    assert!(
        failed
            .get_error()
            .starts_with("Could not read embedded metadata:")
    );
    assert!(!failed.get_basics().is_empty());
    failed.invoke_dismissed();
    // Deleting from local storage makes this known file non-local. Missing
    // bytes must not be read and the reference's message is its only section.
    store
        .write_content(move |w| w.delete_files(w.roles().local_file_storage, &[file], None))
        .unwrap();
    viewer.invoke_context_menu_requested();
    viewer.invoke_menu_chosen(viewer.get_context_menu().info_metadata);
    let window = dialog(&bound);
    loaded(&window);
    assert_eq!(
        window.get_text(),
        panel_fixture["non_local_worker_text"].as_str().unwrap()
    );
    check_visibility(&window, &panel_fixture["cases"][2]);
    assert!(window.get_has_text());
    assert!(!window.get_has_exif() && !window.get_has_xmp() && !window.get_has_iptc());
    assert_eq!(window.get_extra().row_count(), 0);
    window.invoke_dismissed();
    viewer.invoke_close_requested();
}
