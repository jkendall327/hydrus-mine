use slint::ComponentHandle;
#[path="/workspace/hydrus-mine/crates/hydrus-gui/src/fonts.rs"]
mod fonts;
slint::slint! {
    import { LineEdit } from "std-widgets.slint";
    export component Probe inherits Window {
        width: 400px; height: 100px;
        VerticalLayout {
            Text { text: "Latin 123 é 日本語 中文 한글"; }
            LineEdit { text: " 🦊 "; }
        }
    }
}
fn main() {
    let windows = hydrus_gui::headless::init();
    fonts::install_emoji_fallback().unwrap();
    let ui = Probe::new().unwrap(); ui.show().unwrap();
    let native=windows.get(0).unwrap();
    let pixels=hydrus_gui::headless::render(&native,400,100);
    hydrus_gui::headless::save_png(std::path::Path::new("/workspace/validation-reviews/or-emoji/probe.png"),&pixels,400,100).unwrap();
}
