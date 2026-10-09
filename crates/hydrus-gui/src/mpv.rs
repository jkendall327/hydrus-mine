//! Video, animation and audio through libmpv, as the reference plays them.
//!
//! libmpv is loaded when first needed, so building needs nothing, and
//! without it the viewer shows a file's thumbnail. Frames come from mpv's
//! software renderer on a thread of their own, which (as mpv's render API
//! requires) calls nothing but the render functions; the UI thread sends
//! mpv its commands and shows the frames as they arrive.
//!
//! As the reference, a file loops, and the store's `mpv.conf` (else
//! hydrus's default one) is loaded.
#![allow(unsafe_code)] // calls into libmpv, each with the contract noted

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::thread::JoinHandle;

use crossbeam_channel::{Receiver, Sender};
use libloading::Library;
use slint::{Rgba8Pixel, SharedPixelBuffer};

/// `mpv_render_param`.
#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

// `mpv_render_param_type` and `mpv_render_update_flag` (libmpv/render.h)
const PARAM_INVALID: c_int = 0;
const PARAM_API_TYPE: c_int = 1;
const PARAM_SW_SIZE: c_int = 17;
const PARAM_SW_FORMAT: c_int = 18;
const PARAM_SW_STRIDE: c_int = 19;
const PARAM_SW_POINTER: c_int = 20;
const UPDATE_FRAME: u64 = 1;

// `mpv_format` (libmpv/client.h)
const FORMAT_FLAG: c_int = 3;
const FORMAT_DOUBLE: c_int = 5;

type Handle = *mut c_void;
type RenderContext = *mut c_void;
type UpdateFn = unsafe extern "C" fn(*mut c_void);

/// hydrus's `static/mpv-conf/default_mpv.conf`, used when the store has
/// no `mpv.conf` of its own.
const DEFAULT_OPTIONS: &[(&str, &str)] = &[
    ("autoload-files", "no"),
    ("access-references", "no"),
    ("rescan-external-files", "keep-selection"),
    ("audio-display", "embedded-first"),
    ("cursor-autohide", "no"),
];

/// The libmpv functions used.
struct Api {
    create: unsafe extern "C" fn() -> Handle,
    initialize: unsafe extern "C" fn(Handle) -> c_int,
    set_option_string: unsafe extern "C" fn(Handle, *const c_char, *const c_char) -> c_int,
    load_config_file: unsafe extern "C" fn(Handle, *const c_char) -> c_int,
    command: unsafe extern "C" fn(Handle, *mut *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(Handle, *const c_char, c_int, *mut c_void) -> c_int,
    get_property_string: unsafe extern "C" fn(Handle, *const c_char) -> *mut c_char,
    free: unsafe extern "C" fn(*mut c_void),
    terminate_destroy: unsafe extern "C" fn(Handle),
    render_create: unsafe extern "C" fn(*mut RenderContext, Handle, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(RenderContext, Option<UpdateFn>, *mut c_void),
    render_update: unsafe extern "C" fn(RenderContext) -> u64,
    render_render: unsafe extern "C" fn(RenderContext, *mut RenderParam) -> c_int,
    render_free: unsafe extern "C" fn(RenderContext),
    /// Kept loaded for as long as the functions are used (for good).
    _library: Library,
}

/// Where each platform's libmpv is usually found (hydrus ships `mpv-2.dll`
/// on Windows).
const LIBRARY_NAMES: &[&str] = if cfg!(windows) {
    &["libmpv-2.dll", "mpv-2.dll", "mpv-1.dll"]
} else if cfg!(target_os = "macos") {
    &["libmpv.2.dylib", "libmpv.dylib"]
} else {
    &["libmpv.so.2", "libmpv.so.1", "libmpv.so"]
};

impl Api {
    fn load() -> Option<Api> {
        LIBRARY_NAMES.iter().find_map(|name| {
            // SAFETY: loading libmpv runs no initialisers with preconditions
            let library = unsafe { Library::new(name) }.ok()?;
            Self::symbols(library).ok()
        })
    }

    fn symbols(library: Library) -> Result<Api, libloading::Error> {
        // SAFETY: each symbol is declared with its libmpv C signature
        // (libmpv/client.h and render.h, API 2.x)
        unsafe {
            Ok(Api {
                create: *library.get(b"mpv_create\0")?,
                initialize: *library.get(b"mpv_initialize\0")?,
                set_option_string: *library.get(b"mpv_set_option_string\0")?,
                load_config_file: *library.get(b"mpv_load_config_file\0")?,
                command: *library.get(b"mpv_command\0")?,
                get_property: *library.get(b"mpv_get_property\0")?,
                get_property_string: *library.get(b"mpv_get_property_string\0")?,
                free: *library.get(b"mpv_free\0")?,
                terminate_destroy: *library.get(b"mpv_terminate_destroy\0")?,
                render_create: *library.get(b"mpv_render_context_create\0")?,
                render_set_update_callback: *library
                    .get(b"mpv_render_context_set_update_callback\0")?,
                render_update: *library.get(b"mpv_render_context_update\0")?,
                render_render: *library.get(b"mpv_render_context_render\0")?,
                render_free: *library.get(b"mpv_render_context_free\0")?,
                _library: library,
            })
        }
    }
}

/// libmpv, if it could be loaded.
fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(Api::load).as_ref()
}

/// For tests that need libmpv: whether to skip (it isn't installed). Where
/// `HYDRUS_REQUIRE_MPV` is set (CI) a missing libmpv fails the test instead,
/// so that a test is never skipped there unseen.
///
/// # Panics
/// If libmpv is missing and `HYDRUS_REQUIRE_MPV` is set.
pub fn skip_without_libmpv() -> bool {
    if available() {
        return false;
    }
    assert!(
        std::env::var_os("HYDRUS_REQUIRE_MPV").is_none(),
        "libmpv is required here (HYDRUS_REQUIRE_MPV) but could not be loaded"
    );
    eprintln!("libmpv is not installed here; skipped");
    true
}

/// Whether libmpv is there to play video.
pub fn available() -> bool {
    api().is_some()
}

/// The audio devices a new mpv can see (its `audio-device-list`, as the
/// reference's `GetAudioDeviceTuples` asks); none without libmpv.
pub fn audio_devices() -> Option<Vec<hydrus_gui_model::mpv_audio_devices::Device>> {
    let api = api()?;
    // SAFETY: creating a handle has no preconditions
    let handle = unsafe { (api.create)() };
    if handle.is_null() {
        return None;
    }
    // SAFETY: a fresh handle, initialised once; the property name is
    // NUL-terminated; the string mpv returns is read once and given back to
    // `mpv_free`; the handle is destroyed last and not used after
    let list = unsafe {
        let list = if (api.initialize)(handle) >= 0 {
            let text = (api.get_property_string)(handle, c"audio-device-list".as_ptr());
            if text.is_null() {
                String::new()
            } else {
                let list = std::ffi::CStr::from_ptr(text)
                    .to_string_lossy()
                    .into_owned();
                (api.free)(text.cast());
                list
            }
        } else {
            String::new()
        };
        (api.terminate_destroy)(handle);
        list
    };
    Some(hydrus_gui_model::mpv_audio_devices::parse(&list))
}

enum Wake {
    /// mpv has something new to render.
    Update,
    /// The frame size changed: render again.
    Resize,
    Stop,
}

/// The render context, moved to the render thread (only it uses it).
struct SendContext(RenderContext);
// SAFETY: after creation only the render thread touches the context, as
// libmpv's render API allows
unsafe impl Send for SendContext {}

/// A player: one mpv core with its render thread.
pub struct Player {
    api: &'static Api,
    handle: Handle,
    size: Arc<AtomicU64>,
    wake: Sender<Wake>,
    frames: Receiver<SharedPixelBuffer<Rgba8Pixel>>,
    render: Option<JoinHandle<()>>,
    /// The update callback's data (a `Sender<Wake>`), freed after the
    /// render context is.
    callback_data: *mut Sender<Wake>,
}

impl std::fmt::Debug for Player {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Player")
            .field("size", &unpack(self.size.load(Ordering::Relaxed)))
            .finish_non_exhaustive()
    }
}

fn pack(width: u32, height: u32) -> u64 {
    (u64::from(width) << 32) | u64::from(height)
}

fn unpack(size: u64) -> (u32, u32) {
    ((size >> 32) as u32, size as u32)
}

/// Called by mpv (on its own thread) when there is a frame to render.
unsafe extern "C" fn on_update(data: *mut c_void) {
    // SAFETY: `data` is the player's boxed sender, alive until the render
    // context (whose callback this is) has been freed
    let wake = unsafe { &*data.cast::<Sender<Wake>>() };
    let _ = wake.try_send(Wake::Update);
}

/// The audio output every player is told to use whatever its conf says
/// (unset in the client).
static AUDIO_OUTPUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Make every player from now on use audio output `name` (`null` plays in
/// real time with no sound card). **For tests only**: it is `pub` so the
/// integration tests can reach it, but the client never calls it, and where
/// it has been called it overrides the user's `mpv.conf` `ao`. Left to
/// itself, mpv probes
/// PipeWire, ALSA and JACK for each file with sound, which on a machine
/// without a sound card (CI) is slow, noisy, and crashes when several
/// players probe at once.
pub fn use_audio_output(name: &str) {
    let _ = AUDIO_OUTPUT.set(name.to_owned());
}

impl Player {
    /// A player, with `conf` (the store's `mpv.conf`) if there is one.
    pub fn new(conf: Option<&Path>) -> Result<Player, String> {
        let api = api().ok_or_else(|| "libmpv could not be loaded".to_owned())?;
        // SAFETY: plain libmpv calls on a handle we just created
        let handle = unsafe { (api.create)() };
        if handle.is_null() {
            return Err("mpv could not be started".into());
        }
        let option = |name: &str, value: &str| {
            let (name, value) = (CString::new(name).unwrap(), CString::new(value).unwrap());
            // SAFETY: a valid handle and NUL-terminated strings
            unsafe { (api.set_option_string)(handle, name.as_ptr(), value.as_ptr()) }
        };
        // video comes to us, never to a window of mpv's own
        option("vo", "libmpv");
        option("terminal", "no");
        option("input-default-bindings", "no");
        option("idle", "yes");
        // as the reference: a file loops
        option("loop", "inf");
        option("loop-playlist", "no");
        let conf = conf.filter(|c| c.exists());
        match conf.and_then(|c| CString::new(c.to_string_lossy().as_bytes()).ok()) {
            // SAFETY: a valid handle and a NUL-terminated path
            Some(path) => unsafe {
                (api.load_config_file)(handle, path.as_ptr());
            },
            None => {
                for (name, value) in DEFAULT_OPTIONS {
                    option(name, value);
                }
            }
        }
        if let Some(output) = AUDIO_OUTPUT.get() {
            option("ao", output);
        }
        // SAFETY: a valid, configured handle
        if unsafe { (api.initialize)(handle) } < 0 {
            // SAFETY: the handle is ours and not used again
            unsafe { (api.terminate_destroy)(handle) };
            return Err("mpv could not be initialised".into());
        }
        let mut context: RenderContext = null_mut();
        let mut params = [
            RenderParam {
                kind: PARAM_API_TYPE,
                data: c"sw".as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: PARAM_INVALID,
                data: null_mut(),
            },
        ];
        // SAFETY: a valid handle and a terminated parameter list
        if unsafe { (api.render_create)(&raw mut context, handle, params.as_mut_ptr()) } < 0 {
            // SAFETY: as above
            unsafe { (api.terminate_destroy)(handle) };
            return Err("mpv's software renderer could not be started".into());
        }
        let (wake, woken) = crossbeam_channel::unbounded();
        let (frames_out, frames) = crossbeam_channel::bounded(2);
        let callback_data = Box::into_raw(Box::new(wake.clone()));
        // SAFETY: the callback's data lives until after the context is freed
        unsafe {
            (api.render_set_update_callback)(context, Some(on_update), callback_data.cast());
        }
        let size = Arc::new(AtomicU64::new(0));
        let render = {
            let context = SendContext(context);
            let size = size.clone();
            std::thread::Builder::new()
                .name("mpv render".into())
                .spawn(move || render_frames(api, &context, &size, &woken, &frames_out))
                .map_err(|e| e.to_string())?
        };
        Ok(Player {
            api,
            handle,
            size,
            wake,
            frames,
            render: Some(render),
            callback_data,
        })
    }

    fn command(&self, args: &[&str]) -> Result<(), String> {
        let args: Vec<CString> = args
            .iter()
            .map(|a| CString::new(*a).map_err(|e| e.to_string()))
            .collect::<Result<_, _>>()?;
        let mut pointers: Vec<*const c_char> = args.iter().map(|a| a.as_ptr()).collect();
        pointers.push(std::ptr::null());
        // SAFETY: a valid handle and a NULL-terminated array of C strings
        let error = unsafe { (self.api.command)(self.handle, pointers.as_mut_ptr()) };
        if error < 0 {
            Err(format!("mpv refused {:?} ({error})", args[0]))
        } else {
            Ok(())
        }
    }

    /// Play a file (it loops).
    pub fn load(&self, path: &Path) -> Result<(), String> {
        self.command(&["loadfile", &path.to_string_lossy()])
    }

    /// Stop playing (the window goes blank).
    pub fn stop(&self) -> Result<(), String> {
        self.command(&["stop"])
    }

    pub fn toggle_pause(&self) -> Result<(), String> {
        self.command(&["cycle", "pause"])
    }

    /// A number property (`time-pos`, `duration`), if mpv has it now.
    fn number(&self, name: &str) -> Option<f64> {
        let name = CString::new(name).ok()?;
        let mut value = 0.0_f64;
        // SAFETY: a valid handle, a NUL-terminated name, and a double for
        // mpv to write
        let error = unsafe {
            (self.api.get_property)(
                self.handle,
                name.as_ptr(),
                FORMAT_DOUBLE,
                (&raw mut value).cast(),
            )
        };
        (error >= 0 && value.is_finite()).then_some(value)
    }

    /// Where playing is, in milliseconds, once a file is loaded.
    pub fn position_ms(&self) -> Option<f64> {
        self.number("time-pos").map(|s| s * 1000.0)
    }

    /// How long the file plays, in milliseconds, once loaded.
    pub fn duration_ms(&self) -> Option<f64> {
        self.number("duration").map(|s| s * 1000.0)
    }

    /// A flag property (`pause`, `mute`): false if mpv hasn't it.
    fn flag(&self, name: &std::ffi::CStr) -> bool {
        let mut flag: c_int = 0;
        // SAFETY: a valid handle, a NUL-terminated name, and an int for mpv
        // to write
        let error = unsafe {
            (self.api.get_property)(
                self.handle,
                name.as_ptr(),
                FORMAT_FLAG,
                (&raw mut flag).cast(),
            )
        };
        error >= 0 && flag != 0
    }

    pub fn paused(&self) -> bool {
        self.flag(c"pause")
    }

    /// A frame on (`direction` 1) or back (-1), as the reference's
    /// `GotoPreviousOrNextFrame` asks mpv (which pauses on it).
    pub fn frame_step(&self, direction: i32) -> Result<(), String> {
        self.command(&[if direction < 0 {
            "frame-back-step"
        } else {
            "frame-step"
        }])
    }

    /// Make mpv loop, and play through the device, as `plan` says (the
    /// reference sets them on its players as a file loads).
    pub fn set_playback_options(
        &self,
        plan: &hydrus_gui_model::mpv_options::Plan,
    ) -> Result<(), String> {
        for command in plan.commands() {
            let command: Vec<&str> = command.iter().map(String::as_str).collect();
            self.command(&command)?;
        }
        Ok(())
    }

    /// Play at `volume` (0 to 100), muted or not.
    pub fn set_audio(&self, volume: u8, mute: bool) -> Result<(), String> {
        self.command(&["set", "volume", &volume.min(100).to_string()])?;
        self.command(&["set", "mute", if mute { "yes" } else { "no" }])
    }

    /// The volume mpv plays at, 0 to 100 (or more, if a config allows).
    pub fn volume(&self) -> Option<f64> {
        self.number("volume")
    }

    pub fn muted(&self) -> bool {
        self.flag(c"mute")
    }

    pub fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.command(&["set", "pause", if paused { "yes" } else { "no" }])
    }

    /// Go to `ms` into the file, exactly, as the reference seeks.
    pub fn seek_ms(&self, ms: f64) -> Result<(), String> {
        let seconds = format!("{:.3}", ms.max(0.0) / 1000.0);
        self.command(&["seek", &seconds, "absolute", "exact"])
    }

    /// Render at this size from now on (the video is fitted within it).
    pub fn set_size(&self, width: u32, height: u32) {
        let size = pack(width, height);
        if self.size.swap(size, Ordering::Relaxed) != size {
            let _ = self.wake.send(Wake::Resize);
        }
    }

    /// The newest frame rendered since last asked, if any.
    pub fn frame(&self) -> Option<slint::Image> {
        self.frames.try_iter().last().map(slint::Image::from_rgba8)
    }

    /// Wait up to `timeout` for a frame (for tests).
    pub fn wait_frame(&self, timeout: std::time::Duration) -> Option<slint::Image> {
        self.frames
            .recv_timeout(timeout)
            .ok()
            .map(slint::Image::from_rgba8)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.wake.send(Wake::Stop);
        if let Some(render) = self.render.take() {
            let _ = render.join();
        }
        // SAFETY: the render thread has freed the context, so nothing calls
        // back with this data, and the handle is not used again
        unsafe {
            drop(Box::from_raw(self.callback_data));
            (self.api.terminate_destroy)(self.handle);
        }
    }
}

/// The render thread: render a frame whenever mpv has one (or the size
/// changes), and send it to the UI thread.
fn render_frames(
    api: &'static Api,
    context: &SendContext,
    size: &AtomicU64,
    woken: &Receiver<Wake>,
    frames: &Sender<SharedPixelBuffer<Rgba8Pixel>>,
) {
    let context = context.0;
    let mut buffer: Vec<u8> = Vec::new();
    while let Ok(wake) = woken.recv() {
        let resized = match wake {
            Wake::Update => false,
            Wake::Resize => true,
            Wake::Stop => break,
        };
        // SAFETY: only this thread uses the context
        let flags = unsafe { (api.render_update)(context) };
        let (width, height) = unpack(size.load(Ordering::Relaxed));
        if width == 0 || height == 0 || (flags & UPDATE_FRAME == 0 && !resized) {
            continue;
        }
        // rows aligned to 64 bytes, as mpv prefers
        let stride = (width as usize * 4).next_multiple_of(64);
        buffer.resize(stride * height as usize + 64, 0);
        let offset = buffer.as_ptr().align_offset(64);
        let pixels = buffer[offset..].as_mut_ptr();
        let mut sw_size = [width as c_int, height as c_int];
        let mut sw_stride = stride;
        let mut params = [
            RenderParam {
                kind: PARAM_SW_SIZE,
                data: sw_size.as_mut_ptr().cast(),
            },
            RenderParam {
                kind: PARAM_SW_FORMAT,
                data: c"rgb0".as_ptr().cast_mut().cast(),
            },
            RenderParam {
                kind: PARAM_SW_STRIDE,
                data: (&raw mut sw_stride).cast(),
            },
            RenderParam {
                kind: PARAM_SW_POINTER,
                data: pixels.cast(),
            },
            RenderParam {
                kind: PARAM_INVALID,
                data: null_mut(),
            },
        ];
        // SAFETY: the target holds `stride * height` bytes from an aligned
        // pointer, and only this thread uses the context
        if unsafe { (api.render_render)(context, params.as_mut_ptr()) } < 0 {
            continue;
        }
        let mut frame = SharedPixelBuffer::<Rgba8Pixel>::new(width, height);
        let target = frame.make_mut_bytes();
        let row = width as usize * 4;
        for (y, out) in target.chunks_exact_mut(row).enumerate() {
            let start = offset + y * stride;
            out.copy_from_slice(&buffer[start..start + row]);
            // (the fourth byte of rgb0 is padding)
            for pixel in out.chunks_exact_mut(4) {
                pixel[3] = 255;
            }
        }
        let _ = frames.try_send(frame);
    }
    // SAFETY: only this thread used the context; it is not used again
    unsafe {
        (api.render_set_update_callback)(context, None, null_mut());
        (api.render_free)(context);
    }
}
