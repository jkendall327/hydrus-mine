//! The about window against the reference's (`oracle/fixtures/about_window.json`,
//! from `oracle/record_about_window.py`): its title, site, tabs, credits and
//! license as the reference has them, and the description's and libraries'
//! lines in the reference's forms (their values are the machine's).

use hydrus_gui_model::about::{About, Facts, SITE, TITLE, about};

fn facts() -> Facts {
    Facts {
        version: "0.1.0".into(),
        arch: "x86_64".into(),
        os: "Linux".into(),
        ffmpeg: Some("6.1.1-3ubuntu5".into()),
        sqlite: "3.45.1".into(),
        boot_ms: 1_791_043_373_014,
        now_ms: 1_791_043_387_000,
        install_dir: "/opt/hydrus-rs".into(),
        db_dir: "/home/me/store".into(),
        temp_dir: "/tmp".into(),
        cache_mb: 256,
        journal_mode: "wal".into(),
        synchronous: 2,
    }
}

/// A line's label: before its ": ", or its first two words ("running on").
fn label(line: &str) -> String {
    line.split_once(": ").map_or_else(
        || line.split(' ').take(2).collect::<Vec<_>>().join(" "),
        |(l, _)| l.to_owned(),
    )
}

#[test]
fn the_about_window_says_what_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("about_window.json");
    assert_eq!(TITLE, recorded["title"]);
    let license = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../LICENSE"),
    )
    .unwrap();
    let About {
        name,
        version,
        site,
        tabs,
    } = about(&facts(), Some(&license));
    assert_eq!(site, SITE);
    let header = recorded["labels"].as_array().unwrap();
    assert!(header[2].as_str().unwrap().contains(SITE));
    assert_eq!(name, "hydrus-rs");
    // ("v688, using network version 20", with our own version first)
    let theirs = header[1].as_str().unwrap();
    assert_eq!(version, format!("v0.1.0, porting hydrus {theirs}"));
    let recorded_tabs = recorded["tabs"].as_array().unwrap();
    let names: Vec<&str> = tabs.iter().map(|(n, _)| n.as_str()).collect();
    let theirs: Vec<&str> = recorded_tabs
        .iter()
        .map(|t| t[0].as_str().unwrap())
        .collect();
    assert_eq!(names, theirs);
    // the description: the reference's sentence, and each line one of its
    // own labels, in its order, with the boot time in its form
    let description = &tabs[0].1;
    let their_description = recorded_tabs[0][1].as_str().unwrap();
    let (sentence, rest) = description.split_once("\n\n").unwrap();
    let (their_sentence, their_rest) = their_description.split_once("\n\n").unwrap();
    assert!(sentence.starts_with(their_sentence.trim_end_matches('.')));
    let their_labels: Vec<String> = their_rest.lines().map(label).collect();
    let mut at = 0;
    for line in rest.lines() {
        let found = their_labels[at..]
            .iter()
            .position(|l| *l == label(line))
            .unwrap_or_else(|| panic!("{line:?} isn't among {their_labels:?} after {at}"));
        at += found + 1;
    }
    assert!(rest.contains("\nboot time: 14 seconds ago (2026-10-03 16:02:53.014)\n"));
    // the libraries in the reference's form
    assert_eq!(tabs[1].1, "ffmpeg: yes");
    let no_ffmpeg = about(
        &Facts {
            ffmpeg: None,
            ..facts()
        },
        None,
    );
    assert_eq!(no_ffmpeg.tabs[1].1, "ffmpeg: not available");
    assert_eq!(no_ffmpeg.tabs[3].1, "no license file found!");
    // the credits start as the reference's; the license is its file
    assert!(tabs[2].1.starts_with(recorded_tabs[2][1].as_str().unwrap()));
    assert_eq!(tabs[3].1, recorded_tabs[3][1]);
}
