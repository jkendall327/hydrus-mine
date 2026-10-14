//! Embeds `git describe` in the build, for the bench report's version line.

use std::process::Command;

fn main() {
    // HYDRUS_GIT_DESCRIBE in the environment wins (a worktree or a source
    // archive has no .git to ask, and the rerun paths below do nothing there)
    let described = std::env::var("HYDRUS_GIT_DESCRIBE")
        .ok()
        .filter(|text| !text.is_empty())
        .or_else(|| {
            Command::new("git")
                .args(["describe", "--always", "--dirty", "--tags"])
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| String::from_utf8(out.stdout).ok())
                .map(|text| text.trim().to_owned())
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=HYDRUS_GIT_DESCRIBE={described}");
    // (the commit changes by moving HEAD or the branch it names)
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
    println!("cargo:rerun-if-env-changed=HYDRUS_GIT_DESCRIBE");
    println!("cargo:rerun-if-changed=build.rs");
}
