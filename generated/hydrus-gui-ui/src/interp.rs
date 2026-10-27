//! (spike) The runtime behind the `interpreter-cached` feature: the UI's
//! sources are embedded, compiled by Slint's interpreter once per thread
//! (its definitions are not `Send`), and every `Window::new()` instantiates
//! the cached definition. The generated API (slint-build's live-preview
//! generator) is rewritten by build.rs to use [`Component`] in place of
//! `LiveReloadingComponent`: same methods, no compiling per `new()`, no file
//! watching.

use std::cell::{OnceCell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use slint::PlatformError;
use slint_interpreter::{CompilationResult, Compiler, ComponentDefinition, ComponentInstance, Value};

include!(concat!(env!("OUT_DIR"), "/interp_sources.rs"));

thread_local! {
    static UI: OnceCell<CompilationResult> = const { OnceCell::new() };
}

fn compile() -> CompilationResult {
    let started = std::time::Instant::now();
    let mut compiler = Compiler::default();
    compiler.set_library_paths(HashMap::from([(
        "hydrus_ui_core".to_owned(),
        PathBuf::from(LIBRARY_MAIN),
    )]));
    let sources: HashMap<&'static Path, &'static str> =
        SOURCES.iter().map(|(path, text)| (Path::new(*path), *text)).collect();
    compiler.set_file_loader(move |path| {
        let found = sources.get(path).map(|text| Ok((*text).to_owned()));
        Box::pin(std::future::ready(found))
    });
    let future = compiler.build_from_source(ENTRY_SOURCE.to_owned(), PathBuf::from(ENTRY_PATH));
    let mut future = std::pin::pin!(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    let std::task::Poll::Ready(result) = std::future::Future::poll(future.as_mut(), &mut cx) else {
        unreachable!("the file loader is synchronous")
    };
    result.print_diagnostics();
    assert!(!result.has_errors(), "the embedded UI compiles");
    if std::env::var_os("HYDRUS_INTERP_TIMING").is_some() {
        eprintln!("hydrus-gui-ui: compiled the UI in {:?}", started.elapsed());
    }
    result
}

fn definition(name: &str) -> Result<ComponentDefinition, PlatformError> {
    UI.with(|ui| {
        ui.get_or_init(compile)
            .component(name)
            .ok_or_else(|| PlatformError::from(format!("no component {name} in the UI")))
    })
}

/// Compile the UI on this thread now, rather than at the first `new()`.
pub fn warm_up() {
    UI.with(|ui| {
        ui.get_or_init(compile);
    });
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
