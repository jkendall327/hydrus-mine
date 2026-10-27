//! (spike) With the `interpreter` feature, generate the UI's typed API with
//! slint-build's live-preview generator, from one entry file that exports
//! everything the six compiled crates export. Without it, nothing to do.

use std::fmt::Write as _;
use std::path::PathBuf;

fn main() {
    if std::env::var_os("CARGO_FEATURE_INTERPRETER").is_none() {
        return;
    }
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let ui = manifest.join("../../crates/hydrus-gui/ui").canonicalize().unwrap();
    let out = PathBuf::from(std::env::var_os("OUT_DIR").unwrap());

    // (one `export *` per file is allowed: main.slint's; the group files are
    // only `export { .. } from ".."` lines, copied with absolute paths)
    let mut all = format!("export * from \"{}\";\n", ui.join("main.slint").display());
    let mut groups: Vec<_> = std::fs::read_dir(&ui)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.to_string_lossy().ends_with("_group.slint"))
        .collect();
    groups.sort();
    for group in &groups {
        println!("cargo:rerun-if-changed={}", group.display());
        for line in std::fs::read_to_string(group).unwrap().lines() {
            if let Some(rest) = line.strip_prefix("export {") {
                let (names, file) = rest.split_once("} from \"").unwrap();
                let file = file.trim_end_matches("\";");
                writeln!(all, "export {{{names}}} from \"{}\";", ui.join(file).display()).unwrap();
            }
        }
    }
    let entry = out.join("all.slint");
    std::fs::write(&entry, all).unwrap();

    // (an env var is how slint-build picks the live-preview generator)
    // SAFETY: the build script is single-threaded here.
    unsafe { std::env::set_var("SLINT_LIVE_PREVIEW", "1") };
    let library = std::collections::HashMap::from([("hydrus_ui_core".to_owned(), ui.join("main.slint"))]);
    let config = slint_build::CompilerConfiguration::new().with_library_paths(library);
    slint_build::compile_with_config(&entry, config).expect("the UI compiles");

    if std::env::var_os("CARGO_FEATURE_INTERPRETER_CACHED").is_some() {
        cached(&ui, &entry, &out);
    }
}

/// Embed the sources and rewrite the generated API to [`interp::Component`]
/// (src/interp.rs): compiled once per thread, no file watching.
fn cached(ui: &std::path::Path, entry: &std::path::Path, out: &std::path::Path) {
    let mut table = String::new();
    writeln!(table, "pub(crate) const ENTRY_PATH: &str = {:?};", entry.display().to_string()).unwrap();
    writeln!(table, "pub(crate) const ENTRY_SOURCE: &str = include_str!({:?});", entry.display().to_string()).unwrap();
    writeln!(table, "pub(crate) const LIBRARY_MAIN: &str = {:?};", ui.join("main.slint").display().to_string()).unwrap();
    table.push_str("pub(crate) static SOURCES: &[(&str, &str)] = &[\n");
    let mut files: Vec<_> = std::fs::read_dir(ui)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "slint"))
        .collect();
    files.sort();
    for file in &files {
        let file = file.display().to_string();
        writeln!(table, "    ({file:?}, include_str!({file:?})),").unwrap();
    }
    table.push_str("];\n");
    // each window's own file, so a thread compiles only what it opens
    table.push_str("pub(crate) static WINDOW_FILES: &[(&str, &str)] = &[\n");
    for file in &files {
        for line in std::fs::read_to_string(file).unwrap().lines() {
            if let Some(rest) = line.strip_prefix("export component ") {
                if let Some((name, _)) = rest.split_once(" inherits Window") {
                    writeln!(table, "    ({name:?}, {:?}),", file.display().to_string()).unwrap();
                }
            }
        }
    }
    table.push_str("];\n");
    std::fs::write(out.join("interp_sources.rs"), table).unwrap();

    let generated = out.join("all.rs");
    let code = std::fs::read_to_string(&generated).unwrap();
    let code = rewrite(&code);
    std::fs::write(&generated, code).unwrap();
}

fn rewrite(code: &str) -> String {
    // Each `new()` builds a live_preview::Compiler and then calls
    // `LiveReloadingComponent::new(compiler, <path>, Some("<Name>".into()))?`:
    // replace from the compiler's creation up to that call with a cached
    // instantiation of <Name>.
    let mut result = String::with_capacity(code.len());
    let mut rest = code;
    let start_pat = "let mut compiler = sp :: live_preview :: Compiler :: default () ;";
    let call_pat = "sp :: live_preview :: LiveReloadingComponent :: new (";
    while let Some(start) = rest.find(start_pat) {
        result.push_str(&rest[..start]);
        let after = &rest[start..];
        let call = after.find(call_pat).expect("each compiler is used in a new()");
        let args = &after[call + call_pat.len()..];
        let name_at = args.find("Some (\"").unwrap() + "Some (\"".len();
        let name_end = name_at + args[name_at..].find('"').unwrap();
        let name = &args[name_at..name_end];
        let close = args.find(") ? ;").unwrap() + ") ? ;".len();
        write!(result, "let instance = crate::interp::Component::new({name:?})?;").unwrap();
        rest = &args[close..];
    }
    result.push_str(rest);
    result.replace("sp :: live_preview :: LiveReloadingComponent", "crate::interp::Component")
}
