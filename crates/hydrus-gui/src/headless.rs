//! Drawing windows without a display, with Slint's software renderer: for
//! tests, and for screenshots.

use std::cell::RefCell;
use std::rc::Rc;

use slint::PhysicalSize;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Clipboard, Platform, PlatformError, WindowAdapter};

/// Every window made so far, in order.
#[derive(Clone, Default)]
pub struct Windows(Rc<RefCell<Vec<Rc<MinimalSoftwareWindow>>>>);

impl std::fmt::Debug for Windows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Windows({})", self.count())
    }
}

impl Windows {
    /// The `n`th window made (0 is the first).
    pub fn get(&self, n: usize) -> Option<Rc<MinimalSoftwareWindow>> {
        self.0.borrow().get(n).cloned()
    }

    pub fn count(&self) -> usize {
        self.0.borrow().len()
    }
}

thread_local! {
    static CLIPBOARD_TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Seed the native text editor's clipboard for headless key/paste tests.
pub fn set_clipboard_text(text: &str) {
    CLIPBOARD_TEXT.with(|clipboard| *clipboard.borrow_mut() = Some(text.to_owned()));
}

struct Headless {
    windows: Windows,
}

impl Platform for Headless {
    fn set_clipboard_text(&self, text: &str, clipboard: Clipboard) {
        if clipboard == Clipboard::DefaultClipboard {
            set_clipboard_text(text);
        }
    }
    fn clipboard_text(&self, clipboard: Clipboard) -> Option<String> {
        if clipboard == Clipboard::DefaultClipboard {
            CLIPBOARD_TEXT.with(|text| text.borrow().clone())
        } else {
            None
        }
    }
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        self.windows.0.borrow_mut().push(window.clone());
        Ok(window)
    }
}

/// Make this process draw its windows headless (call once, before creating
/// any window, on the thread that will use them).
pub fn init() -> Windows {
    let windows = Windows::default();
    slint::platform::set_platform(Box::new(Headless {
        windows: windows.clone(),
    }))
    .expect("no platform was set yet");
    windows
}

/// Draw `window` at `width` × `height`, as RGBA pixels.
pub fn render(window: &MinimalSoftwareWindow, width: u32, height: u32) -> Vec<u8> {
    window.set_size(PhysicalSize::new(width, height));
    slint::platform::update_timers_and_animations();
    render_snapshot(window, width, height)
}

/// Draw the current state without advancing playback, decoding or other timers.
/// Settle layout with `render` before isolating a controlled paint snapshot.
pub fn render_snapshot(window: &MinimalSoftwareWindow, width: u32, height: u32) -> Vec<u8> {
    window.set_size(PhysicalSize::new(width, height));
    window.request_redraw();
    let mut buffer = vec![PremultipliedRgbaColor::default(); (width * height) as usize];
    window.draw_if_needed(|renderer| {
        renderer.render(&mut buffer, width as usize);
    });
    buffer
        .iter()
        .flat_map(|p| [p.red, p.green, p.blue, p.alpha])
        .collect()
}

/// Save RGBA pixels as a PNG.
pub fn save_png(
    path: &std::path::Path,
    pixels: &[u8],
    width: u32,
    height: u32,
) -> std::io::Result<()> {
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    let mut encoder = png::Encoder::new(file, width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(std::io::Error::other)?;
    writer
        .write_image_data(pixels)
        .map_err(std::io::Error::other)
}
