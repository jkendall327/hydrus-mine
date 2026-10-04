//! Basics and EXIF list behavior against the live reference panel recording.
use hydrus_core::Mime;
use hydrus_gui_model::embedded_metadata::{ExifList, basics, exif_instruction};
use hydrus_gui_model::info_lines::InfoLine;
use hydrus_media::ExifRow;

fn line(text: &str, submenu: Option<Vec<InfoLine>>) -> InfoLine {
    InfoLine {
        text: text.into(),
        interesting: false,
        submenu,
    }
}

#[test]
fn basics_and_png_instruction_match_the_reference() {
    let fixture = hydrus_testkit::fixture_json("embedded_metadata_window.json");
    let lines = [
        line("one file", None),
        line(
            "all modified times",
            Some(vec![
                line("local: yesterday", None),
                line("sources", Some(vec![line("example.com: today", None)])),
            ]),
        ),
    ];
    assert_eq!(basics(&lines), fixture["basics"].as_str().unwrap());
    assert_eq!(basics(&[]), "");
    assert_eq!(
        exif_instruction(Mime::ImagePng),
        fixture["cases"][1]["instruction"]
    );
    assert_eq!(
        exif_instruction(Mime::ImageJpeg),
        fixture["cases"][0]["instruction"]
    );
}

#[test]
fn exif_sort_selection_and_raw_copy_match_the_reference() {
    let fixture = hydrus_testkit::fixture_json("embedded_metadata_window.json");
    let rows = fixture["list_rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| ExifRow {
            id: r[0].as_str().unwrap().parse().unwrap(),
            label: r[1].as_str().unwrap().into(),
            value: r[2].as_str().unwrap().into(),
            copy: r[3].as_str().unwrap().into(),
        })
        .collect();
    let mut list = ExifList::new(rows);
    assert_eq!(list.order, [0, 1, 2, 3, 4]);
    assert_eq!(list.copy(), None);
    list.selection.click(&list.order, 0, false, false);
    assert_eq!(list.copy(), Some("Camera\0Maker\0"));
    list.selection.click(&list.order, 1, false, false);
    assert_eq!(list.copy(), Some("41534349490000006120636f6d6d656e74"));
    list.selection.click(&list.order, 0, true, false);
    assert_eq!(list.copy(), Some("Camera\0Maker\0"));
    for sort in fixture["sort_orders"].as_array().unwrap() {
        list.sort(
            sort["column"].as_u64().unwrap().try_into().unwrap(),
            sort["ascending"].as_bool().unwrap(),
        );
        let order: Vec<usize> = sort["order"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i.as_u64().unwrap().try_into().unwrap())
            .collect();
        assert_eq!(list.order, order);
        assert_eq!(
            list.copy(),
            list.order
                .iter()
                .find(|&&i| i == 0 || i == 1)
                .map(|&i| list.rows[i].copy.as_str())
        );
    }
}
