//! Actual Qt settings and global formatter outputs, replayed over owned preferences.
use hydrus_gui_model::{
    file_log, gui_format,
    options::{Editor, Row, Settings},
    search_log,
};
use hydrus_store::{
    Store,
    queues::{FileSeed, FileSeedMeta, GallerySeed, GallerySeedMeta, SeedStatus, SeedType},
    settings::{self, GuiFormatting},
};
use serde_json::{Value, json};
const ISO: &str = "Prefer ISO time (\"2018-03-01 12:40:23\") to \"5 days ago\": ";
const FIGURES: &str = "EXPERIMENTAL: Bytes strings >1KB pseudo significant figures: ";
fn value(p: &GuiFormatting) -> Value {
    json!({"iso":p.iso,"figures":p.figures})
}
fn row(e: &Editor, label: &str) -> usize {
    e.rows()
        .iter()
        .position(|r| matches!(r,Row::Opt{option,..} if option.label==label))
        .unwrap()
}
#[test]
fn staged_bounds_cancel_save_reopen_and_real_log_columns_match_qt() {
    let fixture = hydrus_testkit::fixture_json("gui_format.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    assert_eq!(value(&gui_format::preferences(&store)), fixture["defaults"]);
    assert_eq!(
        serde_json::from_str::<GuiFormatting>("{}").unwrap(),
        GuiFormatting::default()
    );
    let f = &fixture["seeds"]["file"];
    let file = FileSeed {
        id: 0,
        queue_id: 0,
        seed_type: SeedType::Url,
        data: f["data"].as_str().unwrap().into(),
        data_for_comparison: f["data"].as_str().unwrap().into(),
        created: f["created"].as_i64().unwrap(),
        modified: f["modified"].as_i64().unwrap(),
        source_time: f["source_time"].as_i64(),
        status: SeedStatus::Unknown,
        note: String::new(),
        referral_url: None,
        meta: FileSeedMeta::default(),
    };
    let g = &fixture["seeds"]["gallery"];
    let gallery = GallerySeed {
        id: 0,
        queue_id: 0,
        url: g["url"].as_str().unwrap().into(),
        can_generate_more_pages: true,
        created: g["created"].as_i64().unwrap(),
        modified: g["modified"].as_i64().unwrap(),
        status: SeedStatus::Unknown,
        note: String::new(),
        referral_url: None,
        meta: GallerySeedMeta::default(),
    };
    let now = fixture["now"].as_i64().unwrap();
    for event in fixture["events"].as_array().unwrap() {
        let before = store.read(Settings::load).unwrap();
        assert_eq!(value(&before.gui_formatting), event["before"]);
        let mut editor = Editor::new(before.clone());
        let page = editor
            .page_names()
            .iter()
            .position(|p| *p == "gui")
            .unwrap();
        editor.show_page(page);
        editor.check(row(&editor, ISO), event["input"][0].as_bool().unwrap());
        editor.number(row(&editor, FIGURES), event["input"][1].as_i64().unwrap());
        let (after, _, errors) = editor.applied();
        assert!(errors.is_empty());
        assert_eq!(value(&after.gui_formatting), event["draft"]);
        assert_eq!(
            store.read(Settings::load).unwrap(),
            before,
            "detached edits do not change live formatters"
        );
        let formatting = after.gui_formatting.clone();
        for sample in event["bytes"].as_array().unwrap() {
            assert_eq!(
                gui_format::bytes(&formatting, sample["size"].as_u64().unwrap()),
                sample["text"].as_str().unwrap()
            );
        }
        for sample in event["timestamps"].as_array().unwrap() {
            assert_eq!(
                gui_format::timestamp(&formatting, sample["timestamp"].as_i64(), now),
                sample["text"].as_str().unwrap()
            );
        }
        assert_eq!(
            json!(file_log::row_with_format(&file, 0, now, &formatting)),
            event["file_row"]
        );
        assert_eq!(
            json!(search_log::row_with_format(&gallery, 0, now, &formatting)),
            event["gallery_row"]
        );
        store
            .write(move |ctx| after.save(ctx.conn(), &before))
            .unwrap();
        assert_eq!(
            value(
                &Store::open(dir.path())
                    .unwrap()
                    .read(settings::get)
                    .unwrap()
            ),
            event["reopened"]
        );
    }
    let before = store.read(Settings::load).unwrap();
    let mut cancelled = Editor::new(before.clone());
    let page = cancelled
        .page_names()
        .iter()
        .position(|p| *p == "gui")
        .unwrap();
    cancelled.show_page(page);
    cancelled.check(row(&cancelled, ISO), !before.gui_formatting.iso);
    cancelled.number(row(&cancelled, FIGURES), 2);
    drop(cancelled);
    assert_eq!(store.read(Settings::load).unwrap(), before);
    assert_eq!(value(&before.gui_formatting), fixture["cancel_after"]);
}

#[test]
fn recorded_current_local_offsets_and_ancient_boundaries_match_python() {
    let fixture = hydrus_testkit::fixture_json("gui_format.json");
    let settings = GuiFormatting {
        iso: true,
        figures: 3,
    };
    for zone in fixture["timezone_states"].as_array().unwrap() {
        let offset = jiff::tz::Offset::from_seconds(
            i32::try_from(zone["current_offset"].as_i64().unwrap()).unwrap(),
        )
        .unwrap();
        for sample in zone["samples"].as_array().unwrap() {
            assert_eq!(
                gui_format::timestamp_with_offset(
                    &settings,
                    sample["timestamp"].as_i64(),
                    fixture["now"].as_i64().unwrap(),
                    offset
                ),
                sample["text"].as_str().unwrap(),
                "{}",
                zone["zone"]
            );
        }
    }
}

#[test]
fn precision_reaches_existing_import_png_network_and_service_displays() {
    use hydrus_core::bandwidth::{BandwidthType, Rule, Tracker};
    use hydrus_gui_model::{
        local_import::{Parse, Parsed},
        network_data, png_export, services_review,
    };
    use hydrus_store::network_runtime::{NetworkJob, WaitReason};
    let fixture = hydrus_testkit::fixture_json("gui_format.json");
    let legacy = hydrus_testkit::legacy_fixture("basic");
    let dir = tempfile::tempdir().unwrap();
    hydrus_store::import::import_legacy(
        legacy.path(),
        &dir.path().join(hydrus_store::store::DB_FILE_NAME),
    )
    .unwrap();
    let store = Store::open(dir.path()).unwrap();
    store
        .write(|ctx| {
            ctx.conn().execute("UPDATE files SET size = 0", [])?;
            ctx.conn().execute(
                "UPDATE files SET size = 1536 WHERE hash_id = (SELECT min(hash_id) FROM files)",
                [],
            )?;
            Ok(())
        })
        .unwrap();
    let storage = store
        .snapshot()
        .services
        .all()
        .find(|service| {
            matches!(
                service.kind,
                hydrus_store::services::ServiceKind::LocalFileStorage
            )
        })
        .unwrap()
        .id;
    let parsed = Parsed {
        index: 0,
        path: "/synthetic/format.jpg".into(),
        result: Parse::Good(hydrus_core::Mime::ImageJpeg),
        size: 1536,
    };
    let payload = "x".repeat(1536);
    let job = NetworkJob {
        id: 1,
        url: "https://format.invalid/transfer".into(),
        status: "downloading".into(),
        wait: WaitReason::Downloading,
        speed: 1536,
        bytes_read: 1536,
        bytes_total: Some(1536),
        contexts: Vec::new(),
        obeys_bandwidth: true,
    };
    let now = fixture["now"].as_i64().unwrap();
    for event in fixture["events"].as_array().unwrap() {
        let p: GuiFormatting = serde_json::from_value(event["saved"].clone()).unwrap();
        let saved = p.clone();
        store
            .write(move |ctx| settings::set(ctx.conn(), &saved))
            .unwrap();
        let expected = event["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|sample| sample["size"] == 1536)
            .unwrap()["text"]
            .as_str()
            .unwrap();
        assert_eq!(parsed.row_with_format(&p)[3], expected);
        assert_eq!(
            png_export::payload_description_with_format(&payload, &p),
            format!("String - {expected}")
        );
        assert_eq!(
            png_export::object_payload_description_with_format(&payload, "Synthetic Object", 2, &p),
            format!("A list of 2 Synthetic Object - {expected}")
        );
        assert_eq!(
            network_data::rule_row_with_format(Rule::new(BandwidthType::Data, Some(60), 1536), &p),
            [expected.to_owned(), "1 minute".into()]
        );
        let mut tracker = Tracker::new(now);
        tracker.report_data(1536, now);
        tracker.report_requests(1, now);
        assert_eq!(
            network_data::usage_text_with_format(&mut tracker, Some(60), now, &p),
            format!("{expected} in 1 requests")
        );
        assert_eq!(
            network_data::all_usage_text_with_format(&tracker, &p),
            format!("{expected} in 1 requests")
        );
        assert_eq!(
            network_data::job_row_with_format(&job, &p)[3..],
            [format!("{expected}/s"), format!("{expected}/{expected}")]
        );
        let rows = services_review::rows(&store).unwrap();
        let row = rows.iter().find(|row| row.id == storage).unwrap();
        assert!(
            row.statistics.contains(&format!("totalling {expected}")),
            "{}",
            row.statistics
        );
    }
}
