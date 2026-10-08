//! The about window against the reference's (`oracle/fixtures/about_window.json`,
//! from `oracle/record_about_window.py`): its title, site, tabs, credits and
//! license as the reference has them, and the description's and libraries'
//! lines in the reference's forms (their values are the machine's).

use hydrus_gui_model::about::{
    About, Availability, Facts, Library, SITE, TITLE, about, availability_line, libraries_text,
};

fn facts() -> Facts {
    Facts {
        version: "0.1.0".into(),
        arch: "x86_64".into(),
        os: "Linux".into(),
        ffmpeg: Some("6.1.1-3ubuntu5".into()),
        sqlite: "3.45.1".into(),
        running_as: "from a release build".into(),
        locale: "en_US".into(),
        sqlite_temp_dir: None,
        temp_in_memory: true,
        libraries: vec![vec![Library {
            name: "ffmpeg".into(),
            state: Availability::Yes(None),
        }]],
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
    let no_license = about(&facts(), None);
    assert_eq!(no_license.tabs[3].1, "no license file found!");
    // the credits start as the reference's; the license is its file
    assert!(tabs[2].1.starts_with(recorded_tabs[2][1].as_str().unwrap()));
    assert_eq!(tabs[3].1, recorded_tabs[3][1]);
}

/// The recorded libraries tab, line by line as libraries: a line is "name:
/// yes", "name: yes (how)", "name: not available" or "name: no - error",
/// and a blank line starts a new group.
fn recorded_libraries(text: &str) -> Vec<Vec<Library>> {
    let mut groups = vec![Vec::new()];
    for line in text.lines() {
        if line.is_empty() {
            groups.push(Vec::new());
            continue;
        }
        let (name, state) = line.split_once(": ").unwrap();
        let state = match state {
            "yes" => Availability::Yes(None),
            "not available" => Availability::Missing,
            "no - error" => Availability::Broken,
            how => Availability::Yes(Some(
                how.strip_prefix("yes (")
                    .and_then(|h| h.strip_suffix(')'))
                    .unwrap_or_else(|| panic!("a form not seen before: {line:?}"))
                    .to_owned(),
            )),
        };
        groups.last_mut().unwrap().push(Library {
            name: name.to_owned(),
            state,
        });
    }
    groups
}

#[test]
fn the_optional_libraries_tab_renders_availability_as_the_references_does() {
    let recorded = hydrus_testkit::fixture_json("about_window.json");
    let theirs = recorded["tabs"][1][1].as_str().unwrap();
    // every line the reference printed, in each of its forms, rebuilt
    let groups = recorded_libraries(theirs);
    assert!(groups.len() >= 3, "the reference groups its libraries");
    let forms: std::collections::BTreeSet<String> = groups
        .iter()
        .flatten()
        .map(|l| match &l.state {
            Availability::Yes(None) => "yes".into(),
            Availability::Yes(Some(how)) => format!("yes ({how})"),
            Availability::Missing => "not available".into(),
            Availability::Broken => "no - error".into(),
        })
        .collect();
    for form in [
        "yes",
        "not available",
        "no - error",
        "yes (via plugin)",
        "yes (native)",
    ] {
        assert!(forms.contains(form), "the recording has {form:?}");
    }
    assert_eq!(libraries_text(&groups), theirs);
    for line in theirs.lines().filter(|l| !l.is_empty()) {
        let (name, _) = line.split_once(": ").unwrap();
        let one = recorded_libraries(line);
        assert_eq!(availability_line(&one[0][0]), line, "{name}");
    }
    // and our own tab is that, over our own libraries
    let ours = about(
        &Facts {
            libraries: groups,
            ..facts()
        },
        None,
    );
    assert_eq!(ours.tabs[1].1, theirs);
}

#[test]
fn the_description_has_the_references_lines_in_its_order_and_forms() {
    let recorded = hydrus_testkit::fixture_json("about_window.json");
    let theirs = recorded["tabs"][0][1].as_str().unwrap();
    let (_, their_rest) = theirs.split_once("\n\n").unwrap();
    let their_lines: Vec<&str> = their_rest.lines().collect();
    let their_labels: Vec<String> = their_lines.iter().map(|l| label(l)).collect();
    let ours = about(
        &Facts {
            sqlite_temp_dir: Some("/var/tmp/sqlite".into()),
            ..facts()
        },
        None,
    );
    let description = &ours.tabs[0].1;
    let (_, lines) = description.split_once("\n\n").unwrap();
    // every line of ours is one of theirs, in their order (the python and
    // Qt library versions and the commit period have no equivalent)
    let mut at = 0;
    let mut seen = Vec::new();
    for line in lines.lines() {
        let l = label(line);
        let found = their_labels[at..]
            .iter()
            .position(|t| *t == l || (l.starts_with("sqlite temp dir") && t.starts_with("locale")))
            .unwrap_or_else(|| panic!("{line:?} isn't among {their_labels:?} after {at}"));
        at += found;
        seen.push(l);
    }
    // the platform, ffmpeg and sqlite, the boot time, the directories, the
    // locale and the database settings are all there
    for wanted in [
        "running on",
        "FFMPEG",
        "sqlite",
        "boot time",
        "install dir",
        "db dir",
        "temp dir",
        "sqlite temp dir (from SQLITE_TMPDIR env)",
        "locale",
        "db cache size per file",
        "db journal mode",
        "db synchronous mode",
        "db using memory for temp?",
    ] {
        assert!(seen.iter().any(|s| s == wanted), "{wanted:?} in {seen:?}");
    }
    assert!(description.contains("\nrunning on x86_64 Linux from a release build\n"));
    assert!(description.contains("\nlocale: en_US\n"));
    assert!(description.contains("\ndb using memory for temp?: True"));
    assert!(description.contains("\nsqlite temp dir (from SQLITE_TMPDIR env): /var/tmp/sqlite\n"));
    // the sqlite temp dir line is only for one that differs from the temp dir
    let same = about(
        &Facts {
            sqlite_temp_dir: Some("/tmp".into()),
            ..facts()
        },
        None,
    );
    assert!(!same.tabs[0].1.contains("sqlite temp dir"));
    assert!(!about(&facts(), None).tabs[0].1.contains("sqlite temp dir"));
}
