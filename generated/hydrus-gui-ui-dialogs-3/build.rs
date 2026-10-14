fn main() {
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the manifest directory"),
    );
    slint_build::compile(manifest.join("../../crates/hydrus-gui/ui/dialogs_3_group.slint"))
        .expect("the UI compiles");
}
