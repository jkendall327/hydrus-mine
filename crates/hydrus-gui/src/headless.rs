//! Drawing windows without a display, with Slint's software renderer: for
//! tests, and for screenshots.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use slint::PhysicalSize;
use slint::platform::software_renderer::{
    MinimalSoftwareWindow, PremultipliedRgbaColor, RepaintBufferType,
};
use slint::platform::{Clipboard, Platform, PlatformError, WindowAdapter};

// The platform must never retain an adapter strongly: each adapter's Window
// retains the Slint context, which owns this platform. Visible windows also
// retain their component until hidden. Both edges need owner cleanup.
#[derive(Default)]
struct Registry(RefCell<Vec<Weak<MinimalSoftwareWindow>>>);
impl Registry {
    fn hide_all(&self) {
        let windows: Vec<_> = self.0.borrow().iter().filter_map(Weak::upgrade).collect();
        for window in windows {
            let _ = window.window().hide();
        }
    }
}
#[derive(Default)]
struct Collected {
    registry: Rc<Registry>,
    windows: RefCell<Vec<Rc<MinimalSoftwareWindow>>>,
}
impl Drop for Collected {
    fn drop(&mut self) {
        self.registry.hide_all();
        self.windows.get_mut().clear();
    }
}
/// Every window made so far, in order. The last collector hides and releases
/// its adapters, including visible components retained by Slint. Keep a collector
/// alive for the entire UI scope and release it before returning from that thread.
/// A helper returning windows must pass the collector to its caller too.
///
/// Cleanup must not run from a thread-local destructor: releasing callbacks can
/// drop a Store and join its writer, which deadlocks under Windows' loader lock.
#[must_use = "retain the collector for the entire UI scope, then drop it before thread return"]
#[derive(Clone, Default)]
pub struct Windows(Rc<Collected>);

impl std::fmt::Debug for Windows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Windows({})", self.count())
    }
}

impl Windows {
    /// The `n`th window made (0 is the first).
    pub fn get(&self, n: usize) -> Option<Rc<MinimalSoftwareWindow>> {
        self.0.windows.borrow().get(n).cloned()
    }

    pub fn count(&self) -> usize {
        self.0.windows.borrow().len()
    }
}

thread_local! {
    static CLIPBOARD_TEXT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Seed the native text editor's clipboard for headless key/paste tests.
pub fn set_clipboard_text(text: &str) {
    CLIPBOARD_TEXT.with(|clipboard| *clipboard.borrow_mut() = Some(text.to_owned()));
}

/// Read the native text editor clipboard in headless keyboard regressions.
pub fn clipboard_text() -> Option<String> {
    CLIPBOARD_TEXT.with(|clipboard| clipboard.borrow().clone())
}

struct Headless {
    registry: Rc<Registry>,
    collector: Weak<Collected>,
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
        self.registry.0.borrow_mut().push(Rc::downgrade(&window));
        if let Some(collector) = self.collector.upgrade() {
            collector.windows.borrow_mut().push(window.clone());
        }
        Ok(window)
    }
}

/// Make this process draw its windows headless (call once, before creating
/// any window, on the thread that will use them). Retain the returned collector
/// until every window in that UI scope has finished; drop it before thread exit.
pub fn init() -> Windows {
    let windows = Windows::default();
    slint::platform::set_platform(Box::new(Headless {
        registry: windows.0.registry.clone(),
        collector: Rc::downgrade(&windows.0),
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
