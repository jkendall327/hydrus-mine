//! The date-encode conversion on the production path that reads the machine's
//! clock and zone, compared with the reference's converter recorded under the
//! same zones (`oracle/fixtures/date_encode_local.json`).
//!
//! `TZ` is read when a process starts, so each zone is a child run of this
//! test binary. A zone's recorded offset is the one in force when it was
//! recorded; where the zone is in a different season today, the recorded
//! cases for it cannot apply and are skipped (UTC always applies).

use std::process::Command;

use hydrus_core::url::strings::{Conversion, DateTimezone, StringConverter};
use hydrus_gui_model::string_editors::ConversionEditor;

const ZONE_VAR: &str = "HYDRUS_DATE_ENCODE_CHILD_ZONE";

fn fixture() -> serde_json::Value {
    hydrus_testkit::fixture_json("date_encode_local.json")
}

/// The zone's offset right now from the system's own `date`, not from the
/// code under test.
fn offset_now(zone: &str) -> i64 {
    let out = Command::new("date")
        .env("TZ", zone)
        .arg("+%z")
        .output()
        .expect("date runs");
    let text = String::from_utf8(out.stdout).unwrap();
    let text = text.trim();
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let hours: i64 = text[1..3].parse().unwrap();
    let minutes: i64 = text[3..5].parse().unwrap();
    sign * (hours * 3600 + minutes * 60)
}

/// Run in a child with `TZ` set: every recorded case for the zone, through
/// the converter's own `convert` and the conversion editor's live preview
/// (which read the machine's offset themselves).
#[test]
fn child_converts_recorded_cases_for_its_zone() {
    let Ok(zone) = std::env::var(ZONE_VAR) else {
        return; // an ordinary run; the parent test drives this one
    };
    let now = offset_now(&zone);
    let mut compared = 0;
    for case in fixture()["cases"].as_array().unwrap() {
        if case["zone"] != zone.as_str() || case["offset"].as_i64().unwrap() != now {
            continue;
        }
        let conversion = Conversion::DateEncode {
            phrase: case["phrase"].as_str().unwrap().to_owned(),
            timezone: DateTimezone::Local,
        };
        let text = case["text"].as_str().unwrap();
        let converter = StringConverter {
            conversions: vec![conversion.clone()],
            ..StringConverter::default()
        };
        let result = converter.convert(text);
        let preview = ConversionEditor::new(&conversion, text.to_owned()).result();
        if case["error"] == true {
            assert!(result.is_err(), "{zone}: {case}");
            assert!(preview.starts_with("ERROR"), "{zone}: {case}: {preview}");
        } else {
            let want = case["result"].as_str().unwrap();
            assert_eq!(result.unwrap(), want, "{zone}: {case}");
            assert_eq!(preview, want, "{zone}: {case} (the editor's preview)");
        }
        compared += 1;
    }
    println!("compared {compared} recorded cases in {zone}");
    if zone == "UTC" || now == 0 {
        assert!(compared > 0, "UTC cases must always apply");
    }
}

// leaf: audit-network-conversion-date-encode
#[test]
fn encode_reads_the_machine_zone_as_the_reference_does() {
    if std::env::var(ZONE_VAR).is_ok() {
        return;
    }
    let fixture = fixture();
    let mut zones: Vec<&str> = fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["zone"].as_str().unwrap())
        .collect();
    zones.sort_unstable();
    zones.dedup();
    assert!(zones.len() >= 5, "{zones:?}");
    let mut applicable_non_utc = 0;
    for zone in zones {
        let seasonal_match = fixture["cases"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["zone"] == zone && c["offset"].as_i64().unwrap() == offset_now(zone));
        if zone != "UTC" && seasonal_match {
            applicable_non_utc += 1;
        }
        let out = Command::new(std::env::current_exe().unwrap())
            .env("TZ", zone)
            .env(ZONE_VAR, zone)
            .args([
                "--exact",
                "child_converts_recorded_cases_for_its_zone",
                "--nocapture",
            ])
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            out.status.success(),
            "{zone}: {stdout}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(
            stdout.contains("compared "),
            "{zone}: child did not run: {stdout}"
        );
    }
    assert!(
        applicable_non_utc > 0,
        "no non-UTC zone was in its recorded season"
    );
}
