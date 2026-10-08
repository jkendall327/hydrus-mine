//! Compiles `crates/hydrus-gui/ui/main.slint` with the same compiler the
//! build script uses, writing the generated Rust to a scratch file. Errors
//! (and, with `--deny-warnings`, warnings) fail in seconds instead of after a
//! seven-minute Rust build.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    // (optional arguments, for experiments: another entry file, and where the
    // generated Rust goes)
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .map_or_else(|| root.join("crates/hydrus-gui/ui/main.slint"), PathBuf::from);
    let out = args
        .next()
        .map_or_else(|| std::env::temp_dir().join("hydrus-slint-check.rs"), PathBuf::from);
    let config = slint_build::CompilerConfiguration::new();
    match slint_build::compile_with_output_path(&input, &out, config) {
        Ok(_) => {
            println!("slint-check: ok ({})", input.display());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("slint-check: {error}");
            ExitCode::FAILURE
        }
    }
}
