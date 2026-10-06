fn main() {
    // Slint 1.18 derives gettext's domain from CARGO_PKG_NAME. Preserve the
    // original domain without mutating this process's environment (unsafe in
    // edition 2024): compile in a child with the original package name.
    if std::env::var("CARGO_PKG_NAME").as_deref() != Ok("hydrus-gui") {
        let status = std::process::Command::new(
            std::env::current_exe().expect("the build script has an executable path"),
        )
        .env("CARGO_PKG_NAME", "hydrus-gui")
        .status()
        .expect("the UI compiler child starts");
        assert!(status.success(), "the UI compiler child succeeds");
        return;
    }
    let manifest = std::path::PathBuf::from(
        std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo sets the manifest directory"),
    );
    slint_build::compile(manifest.join("../hydrus-gui/ui/main.slint")).expect("the UI compiles");
}
