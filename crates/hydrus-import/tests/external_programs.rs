//! The programs an import runs on each file, against the reference's own
//! `FileSeed.DoExternalProgramCalls` and `SendSingleFile`
//! (`oracle/fixtures/import_external_programs.json`,
//! `record_import_external_programs.py`): which entries fire for a new file
//! and for one already in the client, what the programs are given, the order,
//! and the pause of every importer when one cannot be run.
#![cfg(unix)]

use std::sync::Arc;

use hydrus_core::external_calls::{
    ActualCall, Callable, Manager, Parameter, Pipeline, Process, Rule,
};
use hydrus_core::import_options::ExternalProgramEntry;
use hydrus_import::{FileImportOptions, FileImporter, ImportStatus};
use hydrus_media::MediaTools;
use hydrus_store::Store;
use hydrus_store::settings::{FolderSettings, Pauses};

fn key(n: u8) -> [u8; 32] {
    [n; 32]
}

fn entry(n: u8, name: &str, on_new: bool, on_already_in: bool) -> ExternalProgramEntry {
    ExternalProgramEntry {
        key: hex(&key(n)),
        name: name.into(),
        on_new,
        on_already_in,
    }
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, b| {
        let _ = write!(text, "{b:02x}");
        text
    })
}

/// A program that appends each argument it is given, then a line of dashes.
fn logger(log: &std::path::Path, first: &str, parameters: &[(Parameter, &str)]) -> Process {
    let mut arguments = vec![
        "-c".to_owned(),
        format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'; done; echo --- >> '{}'",
            log.display(),
            log.display()
        ),
        "sh".to_owned(),
        first.to_owned(),
    ];
    arguments.extend(parameters.iter().map(|(_, token)| (*token).to_owned()));
    Process {
        executable: "/bin/sh".into(),
        arguments,
        rules: parameters.iter().map(|(p, _)| Rule::new(*p)).collect(),
        ..Process::default()
    }
}

fn callable(n: u8, name: &str, pipeline: Pipeline, call: ActualCall) -> Callable {
    Callable {
        key: key(n),
        name: name.into(),
        pipeline,
        call,
    }
}

struct World {
    dir: tempfile::TempDir,
    store: Arc<Store>,
    log: std::path::PathBuf,
    importer: FileImporter,
}

fn world() -> World {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path()).unwrap();
    let log = dir.path().join("programs.log");
    let work = dir.path().to_path_buf();
    let manager = Manager {
        calls: vec![
            callable(
                1,
                "Zulu URL",
                Pipeline::Url,
                ActualCall::Process(Process::default()),
            ),
            callable(
                2,
                "beta log",
                Pipeline::File,
                ActualCall::Process(logger(
                    &log,
                    "beta",
                    &[
                        (Parameter::Path, "%path%"),
                        (Parameter::Uri, "%path_uri%"),
                        (Parameter::Hash, "%hash%"),
                    ],
                )),
            ),
            callable(
                3,
                "Alpha 日本 log",
                Pipeline::File,
                ActualCall::Process(logger(&log, "alpha", &[(Parameter::Path, "%path%")])),
            ),
            callable(
                5,
                "gamma file id",
                Pipeline::File,
                ActualCall::Process(logger(&log, "gamma", &[(Parameter::FileId, "%file_id%")])),
            ),
            callable(
                4,
                "broken",
                Pipeline::File,
                ActualCall::Process(Process {
                    executable: work.join("no-such-program").display().to_string(),
                    arguments: vec!["%path%".into()],
                    rules: vec![Rule::new(Parameter::Path)],
                    ..Process::default()
                }),
            ),
        ],
    };
    store
        .write(move |ctx| hydrus_store::settings::set(ctx.conn(), &manager))
        .unwrap();
    let importer = FileImporter::new(Arc::clone(&store), MediaTools::new());
    World {
        dir,
        store,
        log,
        importer,
    }
}

fn import(
    world: &World,
    seed: u32,
    entries: Vec<ExternalProgramEntry>,
) -> (ImportStatus, hydrus_core::Sha256) {
    let source = world.dir.path().join(format!("source_{seed}.bmp"));
    std::fs::write(&source, hydrus_testkit::bmp(seed, 20000)).unwrap();
    let options = FileImportOptions {
        external_programs: entries,
        ..FileImportOptions::default()
    };
    let result = world.importer.import_path(&source, &options).unwrap();
    (result.status, result.hash.unwrap())
}

/// What the programs logged since the last look: each call's arguments.
fn calls(world: &World) -> Vec<Vec<String>> {
    let text = std::fs::read_to_string(&world.log).unwrap_or_default();
    let _ = std::fs::remove_file(&world.log);
    text.split("---\n")
        .filter(|chunk| !chunk.is_empty())
        .map(|chunk| chunk.lines().map(str::to_owned).collect())
        .collect()
}

fn paused(world: &World) -> [bool; 3] {
    let pauses: Pauses = world.store.read(hydrus_store::settings::get).unwrap();
    let folders: FolderSettings = world.store.read(hydrus_store::settings::get).unwrap();
    [
        pauses.subscriptions,
        pauses.file_queues,
        folders.pause_import_folders,
    ]
}

fn recorded(label: &str) -> serde_json::Value {
    hydrus_testkit::fixture_json("import_external_programs.json")["runs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|run| run["label"] == label)
        .unwrap()
        .clone()
}

/// A recorded call, with the recording's paths and hash made this file's.
fn expected(recorded: &serde_json::Value, path: &std::path::Path, hash: &str) -> Vec<Vec<String>> {
    recorded["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|line| {
            line.as_array()
                .unwrap()
                .iter()
                .map(|argument| {
                    let argument = argument.as_str().unwrap();
                    if argument.starts_with("file://") {
                        format!("file://{}", path.display())
                    } else if argument.starts_with("{db}") {
                        path.display().to_string()
                    } else if argument.len() == 64 {
                        hash.to_owned()
                    } else {
                        argument.to_owned()
                    }
                })
                .collect()
        })
        .collect()
}

fn stored_path(world: &World, hash: &hydrus_core::Sha256) -> std::path::PathBuf {
    world
        .store
        .snapshot()
        .storage
        .file_path(hash, hydrus_core::Mime::ImageBmp)
        .unwrap()
}

#[test]
fn imports_run_the_programs_the_reference_runs_on_new_and_already_in_files() {
    let w = world();
    let cases: [(&str, Vec<ExternalProgramEntry>, bool); 6] = [
        ("new-all", vec![entry(2, "beta log", true, true)], false),
        (
            "redundant-all",
            vec![entry(2, "beta log", true, true)],
            true,
        ),
        (
            "new-only-on-redundant",
            vec![entry(2, "beta log", true, false)],
            true,
        ),
        (
            "already-only-on-new",
            vec![entry(2, "beta log", false, true)],
            false,
        ),
        (
            "two-in-order",
            vec![
                entry(3, "Alpha 日本 log", true, false),
                entry(2, "beta log", true, false),
            ],
            false,
        ),
        ("none", vec![entry(2, "beta log", false, false)], false),
    ];
    for (seed, (label, entries, already_in)) in (1..).zip(cases) {
        if already_in {
            // (in the client first, with nothing to run)
            let (status, _) = import(&w, seed, Vec::new());
            assert_eq!(status, ImportStatus::SuccessfulAndNew, "{label}");
            assert!(calls(&w).is_empty());
        }
        let (status, hash) = import(&w, seed, entries);
        assert_eq!(
            status,
            if already_in {
                ImportStatus::SuccessfulButRedundant
            } else {
                ImportStatus::SuccessfulAndNew
            },
            "{label}"
        );
        let run = recorded(label);
        assert_eq!(
            calls(&w),
            expected(&run, &stored_path(&w, &hash), &hash.to_hex()),
            "{label}"
        );
        assert_eq!(paused(&w), [false; 3], "{label}");
    }
}

#[test]
fn a_program_that_cannot_be_run_pauses_every_importer_and_stops_the_rest() {
    let w = world();
    // (as recorded: a program that is not there, then one that is registered no more)
    import(
        &w,
        50,
        vec![
            entry(4, "broken", true, true),
            entry(2, "beta log", true, true),
        ],
    );
    let run = recorded("broken-first");
    assert!(run["lines"].as_array().unwrap().is_empty());
    assert!(calls(&w).is_empty(), "the later program is not run");
    assert_eq!(paused(&w), [true; 3]);
    assert!(run["paused"].as_array().unwrap().iter().all(|p| p == true));
    // a program using the file id: the reference's string processing refuses
    // the number, so it fails there (recorded as `file-id-parameter`)
    let w = world();
    import(&w, 54, vec![entry(5, "gamma file id", true, true)]);
    assert!(calls(&w).is_empty());
    assert_eq!(paused(&w), [true; 3]);
    let run = recorded("file-id-parameter");
    assert!(run["lines"].as_array().unwrap().is_empty());
    assert!(run["paused"].as_array().unwrap().iter().all(|p| p == true));
    // a program the manager does not have
    let w = world();
    import(&w, 51, vec![entry(9, "gone", true, true)]);
    assert_eq!(paused(&w), [true; 3]);
    assert!(
        recorded("program-missing")["paused"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p == true)
    );
    // a program of the wrong kind (from the reference's `SendSingleFile`, not
    // recorded)
    let w = world();
    import(&w, 52, vec![entry(1, "Zulu URL", true, true)]);
    assert_eq!(paused(&w), [true; 3]);
    // ... and a failing one that is the only one, with the file imported anyway
    // (the file is in before its programs run, in the reference's source)
    let w = world();
    let (status, _) = import(&w, 53, vec![entry(4, "broken", true, true)]);
    assert_eq!(status, ImportStatus::SuccessfulAndNew);
}
