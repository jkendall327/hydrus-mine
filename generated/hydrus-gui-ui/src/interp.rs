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

thread_local! {
    /// By entry file: a window's own file, or the whole UI.
    static UI: RefCell<HashMap<&'static str, Rc<CompilationResult>>> = RefCell::default();
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
    let source = if entry == ENTRY_PATH { ENTRY_SOURCE } else { sources()[Path::new(entry)] };
    let future = compiler.build_from_source(source.to_owned(), PathBuf::from(entry));
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let std::task::Poll::Ready(result) = std::future::Future::poll(future.as_mut(), &mut cx) else {
        unreachable!("the file loader is synchronous")
    };
    result.print_diagnostics();
    assert!(!result.has_errors(), "the embedded UI compiles");
    if std::env::var_os("HYDRUS_INTERP_TIMING").is_some() {
        eprintln!("hydrus-gui-ui: compiled {entry} in {:?}", started.elapsed());
    }
    result
}

fn definition(name: &str) -> Result<ComponentDefinition, PlatformError> {
    let entry = entry_for(name);
    let compiled = UI.with(|ui| ui.borrow().get(entry).cloned());
    let compiled = compiled.unwrap_or_else(|| {
        let compiled = Rc::new(compile(entry));
        UI.with(|ui| ui.borrow_mut().insert(entry, compiled.clone()));
        compiled
    });
    compiled
        .component(name)
        .ok_or_else(|| PlatformError::from(format!("no component {name} in {entry}")))
}

/// An instance of one of the UI's exported components.
pub struct Component {
    instance: ComponentInstance,
}

impl Component {
    pub fn new(name: &str) -> Result<Rc<RefCell<Self>>, PlatformError> {
        let instance = definition(name)?.create()?;
        Ok(Rc::new(RefCell::new(Self { instance })))
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
