fn main() {
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the manifest directory"),
    );
    // A library, so that the other UI crates can import its components and
    // types (`@hydrus_ui_core`) and share them with the Rust side.
    let config = slint_build::CompilerConfiguration::new().as_library("hydrus_ui_core");
    slint_build::compile_with_config(
        manifest.join("../../crates/hydrus-gui/ui/main.slint"),
        config,
    )
    .expect("the UI compiles");
}
