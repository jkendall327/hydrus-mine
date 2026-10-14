//! Compiles `crates/hydrus-gui/ui/main.slint` with the same compiler the
//! build script uses, writing the generated Rust to a scratch file. Errors
//! (and, with `--deny-warnings`, warnings) fail in seconds instead of after a
//! seven-minute Rust build.
//!
//! The UI is built as several crates (see AGENTS.md): the main window, which
//! the others import as the library `@hydrus_ui_core`, and a crate for each
//! `ui/*_group.slint`. With no arguments all of them are checked.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let ui = root.join("crates/hydrus-gui/ui");
    // (optional arguments, for experiments: another entry file, and where the
    // generated Rust goes)
    let mut args = std::env::args_os().skip(1);
    let out = std::env::temp_dir().join("hydrus-slint-check.rs");
    let (inputs, out) = if let Some(input) = args.next() {
        let out = args.next().map_or(out, PathBuf::from);
        (vec![PathBuf::from(input)], out)
    } else {
        let mut inputs = vec![ui.join("main.slint")];
        let mut groups: Vec<_> = std::fs::read_dir(&ui)
            .expect("the UI directory is readable")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.to_string_lossy().ends_with("_group.slint"))
            .collect();
        groups.sort();
        inputs.extend(groups);
        (inputs, out)
    };
    let library = HashMap::from([("hydrus_ui_core".to_owned(), ui.join("main.slint"))]);
    let mut failed = false;
    for input in &inputs {
        let config = slint_build::CompilerConfiguration::new().with_library_paths(library.clone());
        match slint_build::compile_with_output_path(input, &out, config) {
            Ok(_) => println!("slint-check: ok ({})", input.display()),
            Err(error) => {
                eprintln!("slint-check: {error}");
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
