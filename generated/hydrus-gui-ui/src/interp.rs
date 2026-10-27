//! (spike) The runtime behind the `interpreter-cached` feature: the UI's
//! sources are embedded, compiled by Slint's interpreter once per thread
//! (its definitions are not `Send`), and every `Window::new()` instantiates
//! the cached definition. The generated API (slint-build's live-preview
//! generator) is rewritten by build.rs to use [`Component`] in place of
//! `LiveReloadingComponent`: same methods, no compiling per `new()`, no file
//! watching.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::PlatformError;
use slint_interpreter::{CompilationResult, Compiler, ComponentDefinition, ComponentInstance, Value};

include!(concat!(env!("OUT_DIR"), "/interp_sources.rs"));

/// (spike) compile results not yet dropped, in every thread
static LIVE_COMPILES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct Compiled(CompilationResult);
impl Drop for Compiled {
    fn drop(&mut self) {
        LIVE_COMPILES.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}
impl std::ops::Deref for Compiled {
    type Target = CompilationResult;
    fn deref(&self) -> &CompilationResult {
        &self.0
    }
}

/// (spike, HYDRUS_INTERP_LEAKS) instances made on this thread, reported at
/// thread exit if they are still alive
struct Made(RefCell<Vec<(String, slint::Weak<ComponentInstance>)>>);
impl Drop for Made {
    fn drop(&mut self) {
        let alive: Vec<String> = self
            .0
            .borrow()
            .iter()
            .filter(|(_, weak)| weak.upgrade().is_some())
            .map(|(name, _)| name.clone())
            .collect();
        if !alive.is_empty() {
            let thread = std::thread::current();
            eprintln!("hydrus-gui-ui leak: {} alive at exit of {:?}: {alive:?}", alive.len(), thread.name());
        }
    }
}

thread_local! {
    static MADE: Made = Made(RefCell::default());
    /// By entry file: a window's own file, or the whole UI.
    static UI: RefCell<HashMap<&'static str, Rc<Compiled>>> = RefCell::default();
}

fn sources() -> &'static HashMap<&'static Path, &'static str> {
    static SOURCE_MAP: std::sync::OnceLock<HashMap<&'static Path, &'static str>> = std::sync::OnceLock::new();
    SOURCE_MAP.get_or_init(|| SOURCES.iter().map(|(path, text)| (Path::new(*path), *text)).collect())
}

/// The file to compile for `name`: its own, unless HYDRUS_INTERP_WHOLE asks
/// for the whole UI at once.
fn entry_for(name: &str) -> &'static str {
    static WHOLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *WHOLE.get_or_init(|| std::env::var_os("HYDRUS_INTERP_WHOLE").is_some()) {
        return ENTRY_PATH;
    }
    WINDOW_FILES.iter().find(|(window, _)| *window == name).map_or(ENTRY_PATH, |(_, file)| file)
}

fn compile(entry: &'static str) -> CompilationResult {
    let started = std::time::Instant::now();
    let mut compiler = Compiler::default();
    compiler.set_library_paths(HashMap::from([(
        "hydrus_ui_core".to_owned(),
        PathBuf::from(LIBRARY_MAIN),
    )]));
    compiler.set_file_loader(move |path| {
        let found = sources().get(path).map(|text| Ok((*text).to_owned()));
        Box::pin(std::future::ready(found))
    });
    let (source, path) = if entry == ENTRY_PATH {
        (ENTRY_SOURCE.to_owned(), PathBuf::from(ENTRY_PATH))
    } else {
        // the file's windows, and every global (see build.rs)
        let windows: Vec<&str> =
            WINDOW_FILES.iter().filter(|(_, file)| *file == entry).map(|(window, _)| *window).collect();
        let source = format!("export {{ {} }} from {entry:?};\n{GLOBAL_EXPORTS}", windows.join(", "));
        (source, PathBuf::from(format!("{entry}.entry.slint")))
    };
    let future = compiler.build_from_source(source, path);
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let std::task::Poll::Ready(result) = std::future::Future::poll(future.as_mut(), &mut cx) else {
        unreachable!("the file loader is synchronous")
    };
    result.print_diagnostics();
    assert!(!result.has_errors(), "the embedded UI compiles");
    if std::env::var_os("HYDRUS_INTERP_TIMING").is_some() {
        let rss = std::fs::read_to_string("/proc/self/status")
            .ok()
            .and_then(|s| s.lines().find(|l| l.starts_with("VmRSS")).map(str::to_owned))
            .unwrap_or_default();
        eprintln!(
            "hydrus-gui-ui: compiled {entry} in {:?} ({rss}; live before this {})",
            started.elapsed(),
            LIVE_COMPILES.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
    result
}

fn definition(name: &str) -> Result<ComponentDefinition, PlatformError> {
    let entry = entry_for(name);
    let compiled = UI.with(|ui| ui.borrow().get(entry).cloned());
    let compiled = compiled.unwrap_or_else(|| {
        let compiled = Rc::new(Compiled(compile(entry)));
        LIVE_COMPILES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        UI.with(|ui| ui.borrow_mut().insert(entry, compiled.clone()));
        compiled
    });
    compiled
        .component(name)
        .ok_or_else(|| PlatformError::from(format!("no component {name} in {entry}")))
}

/// An instance of one of the UI's exported components: a strong handle to
/// the interpreter's instance, so that (as with compiled code) a weak
/// handle upgrades for as long as Slint keeps the instance, a shown window
/// included, not only while Rust holds a strong one.
pub struct Component {
    instance: ComponentInstance,
}

impl Clone for Component {
    fn clone(&self) -> Self {
        Self { instance: slint::ComponentHandle::clone_strong(&self.instance) }
    }
}

/// A weak [`Component`].
#[derive(Clone, Default)]
pub struct WeakComponent(slint::Weak<ComponentInstance>);

impl WeakComponent {
    pub fn upgrade(&self) -> Option<Component> {
        self.0.upgrade().map(|instance| Component { instance })
    }
}

impl Component {
    pub fn new(name: &str) -> Result<Self, PlatformError> {
        let instance = definition(name)?.create()?;
        static TRACK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *TRACK.get_or_init(|| std::env::var_os("HYDRUS_INTERP_LEAKS").is_some()) {
            MADE.with(|made| made.0.borrow_mut().push((name.to_owned(), slint::ComponentHandle::as_weak(&instance))));
        }
        Ok(Self { instance })
    }

    pub fn downgrade(this: &Self) -> WeakComponent {
        WeakComponent(slint::ComponentHandle::as_weak(&this.instance))
    }

    /// (the generated code borrows a `RefCell`; this is the same call)
    pub fn borrow(&self) -> &Self {
        self
    }

    pub fn instance(&self) -> &ComponentInstance {
        &self.instance
    }

    pub fn set_property(&self, name: &str, value: Value) {
        self.instance.set_property(name, value).unwrap_or_else(|e| panic!("Cannot set property {name}: {e}"));
    }

    pub fn get_property(&self, name: &str) -> Value {
        self.instance.get_property(name).unwrap_or_else(|e| panic!("Cannot get property {name}: {e}"))
    }

    pub fn invoke(&self, name: &str, args: &[Value]) -> Value {
        self.instance.invoke(name, args).unwrap_or_else(|e| panic!("Cannot invoke {name}: {e}"))
    }

    pub fn set_callback(&self, name: &str, callback: Rc<dyn Fn(&[Value]) -> Value + 'static>) {
        self.instance
            .set_callback(name, move |args| callback(args))
            .unwrap_or_else(|e| panic!("Cannot set callback {name}: {e}"));
    }

    pub fn set_global_property(&self, global: &str, name: &str, value: Value) {
        self.instance
            .set_global_property(global, name, value)
            .unwrap_or_else(|e| panic!("Cannot set property {global}::{name}: {e}"));
    }

    pub fn get_global_property(&self, global: &str, name: &str) -> Value {
        self.instance
            .get_global_property(global, name)
            .unwrap_or_else(|e| panic!("Cannot get property {global}::{name}: {e}"))
    }

    pub fn invoke_global(&self, global: &str, name: &str, args: &[Value]) -> Value {
        self.instance
            .invoke_global(global, name, args)
            .unwrap_or_else(|e| panic!("Cannot invoke {global}::{name}: {e}"))
    }

    pub fn set_global_callback(&self, global: &str, name: &str, callback: Rc<dyn Fn(&[Value]) -> Value + 'static>) {
        self.instance
            .set_global_callback(global, name, move |args| callback(args))
            .unwrap_or_else(|e| panic!("Cannot set callback {global}::{name}: {e}"));
    }
}

/// (spike) Compile `name`'s entry and drop the result, without caching: for
/// measuring what a compile leaves behind.
pub fn compile_and_drop(name: &str) {
    drop(compile(entry_for(name)));
}

/// (spike) Forget this thread's compiled definitions.
pub fn clear_cache() {
    UI.with(|ui| ui.borrow_mut().clear());
}
