//! Detached ordered routing queues and MIME rows, with registered-key choosers.
use crate::list_selection::ListSelection;
use hydrus_core::{
    Mime,
    external_calls::{Manager, Pipeline},
    open_externally::{CallRef, Routing},
};

pub const EMPTY: &str = "empty -- will fall back to default OS launch";
pub const MIME_EXHAUSTED: &str = "You have managed to add an entry for every possible mime and general mimetype! What are you doing!!!";
pub const CALL_TITLE: &str = "select call to add";
pub fn exhausted(pipeline: Pipeline) -> String {
    format!(
        "You have added all the \"{}\" calls that are currently registered with the executable manager! Try going to the \"external programs\" panel to add more.",
        pipeline.label()
    )
}
pub fn choices(manager: &Manager, pipeline: Pipeline, existing: &[CallRef]) -> Vec<CallRef> {
    let mut result = manager
        .calls
        .iter()
        .filter(|call| {
            call.pipeline == pipeline && !existing.iter().any(|value| value.key == call.key)
        })
        .map(CallRef::from)
        .collect::<Vec<_>>();
    result.sort_by(|a, b| a.name.cmp(&b.name));
    result
}
pub fn mime_choices() -> Vec<Mime> {
    let mut general = Mime::general_classes();
    general.sort_by_key(|mime| mime.code());
    general
        .into_iter()
        .chain(hydrus_core::mime::SEARCHABLE_MIMES.iter().copied())
        .collect()
}
/// Row identities survive edits, order changes and captured delete questions.
#[derive(Debug, Clone)]
pub struct Queue {
    pub rows: Vec<(u64, CallRef)>,
    next: u64,
    pub selection: ListSelection<u64>,
}
impl Queue {
    pub fn new(values: &[CallRef]) -> Self {
        Self {
            rows: values
                .iter()
                .cloned()
                .enumerate()
                .map(|(i, value)| (i as u64, value))
                .collect(),
            next: values.len() as u64,
            selection: ListSelection::default(),
        }
    }
    pub fn values(&self) -> Vec<CallRef> {
        self.rows.iter().map(|(_, value)| value.clone()).collect()
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(
            &self.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            index,
            ctrl,
            shift,
        );
    }
    pub fn editing(&self) -> Option<u64> {
        self.rows
            .iter()
            .find(|(id, _)| self.selection.is_selected(*id))
            .map(|(id, _)| *id)
    }
    pub fn put(&mut self, id: Option<u64>, value: CallRef) {
        if let Some(id) = id {
            if let Some(row) = self.rows.iter_mut().find(|(key, _)| *key == id) {
                row.1 = value;
            }
        } else {
            self.rows.push((self.next, value));
            self.next += 1;
        }
    }
    pub fn selected(&self) -> Vec<u64> {
        self.selection
            .in_order(&self.rows.iter().map(|(id, _)| *id).collect::<Vec<_>>())
    }
    pub fn delete(&mut self, ids: &[u64]) {
        self.rows.retain(|(id, _)| !ids.contains(id));
        for id in ids {
            self.selection.forget(*id);
        }
    }
    pub fn move_selected(&mut self, down: bool) {
        let mut indices = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| self.selection.is_selected(row.0))
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if down {
            indices.reverse();
        }
        for index in indices {
            let other = if down {
                index.checked_add(1)
            } else {
                index.checked_sub(1)
            };
            if let Some(other) = other.filter(|other| *other < self.rows.len()) {
                self.rows.swap(index, other);
            }
        }
    }
}
#[derive(Debug, Clone)]
pub struct Editor {
    pub urls: Queue,
    pub routing: Routing,
    pub selection: ListSelection<Mime>,
}
impl Editor {
    pub fn new(routing: Routing) -> Self {
        Self {
            urls: Queue::new(&routing.urls),
            routing,
            selection: ListSelection::default(),
        }
    }
    pub fn value(&self) -> Routing {
        let mut value = self.routing.clone();
        value.urls = self.urls.values();
        value
    }
    pub fn rows(&self) -> Vec<(Mime, Vec<CallRef>)> {
        let mut rows = self
            .routing
            .files
            .iter()
            .map(|(mime, values)| (*mime, values.clone()))
            .collect::<Vec<_>>();
        rows.sort_by_key(|(mime, values)| {
            (
                if *mime == Mime::GeneralFile {
                    -2
                } else if mime.is_general_class() {
                    -1
                } else {
                    0
                },
                mime.human_name(),
                summary(values),
            )
        });
        rows
    }
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        self.selection.click(
            &self
                .rows()
                .iter()
                .map(|(mime, _)| *mime)
                .collect::<Vec<_>>(),
            index,
            ctrl,
            shift,
        );
    }
    pub fn selected(&self) -> Vec<Mime> {
        self.selection.in_order(
            &self
                .rows()
                .iter()
                .map(|(mime, _)| *mime)
                .collect::<Vec<_>>(),
        )
    }
    pub fn delete(&mut self, values: &[Mime]) {
        if values.contains(&Mime::GeneralFile) {
            return;
        }
        for mime in values {
            self.routing.files.remove(mime);
            self.selection.forget(*mime);
        }
    }
    pub fn put(&mut self, mime: Mime, values: Vec<CallRef>, new: bool) {
        self.routing.files.insert(mime, values);
        if new {
            self.selection.select_only(Some(mime));
        }
    }
}
pub fn summary(values: &[CallRef]) -> String {
    if values.is_empty() {
        EMPTY.into()
    } else {
        values
            .iter()
            .map(|value| value.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The selected registered operation and its complete typed parameter set.
#[derive(Debug, Clone, PartialEq)]
pub struct Launch {
    pub call: hydrus_core::external_calls::ActualCall,
    pub inputs: hydrus_core::external_calls::Inputs,
}
fn resolve(
    manager: &Manager,
    reference: Option<&CallRef>,
    pipeline: Pipeline,
    subject: &str,
    inputs: hydrus_core::external_calls::Inputs,
) -> Result<Launch, String> {
    let fallback = match pipeline {
        Pipeline::File => hydrus_core::external_calls::ActualCall::DefaultFile,
        Pipeline::Url => hydrus_core::external_calls::ActualCall::DefaultUrl,
    };
    let Some(reference) = reference else {
        return Ok(Launch {
            call: fallback,
            inputs,
        });
    };
    let identity = format!("\"{}\" (id {})", reference.name, hex_key(&reference.key));
    let Some(call) = manager.calls.iter().find(|call| call.key == reference.key) else {
        return Err(format!(
            "When trying to open {subject} externally, the executable we wanted to call ({identity}) did not exist!"
        ));
    };
    if call.pipeline != pipeline {
        return Err(format!(
            "When trying to open {subject} externally, the executable we wanted to call ({identity}) was the wrong type ({})!",
            call.pipeline.label()
        ));
    }
    Ok(Launch {
        call: call.call.clone(),
        inputs,
    })
}
fn hex_key(key: &[u8; 32]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub fn url(manager: &Manager, routing: &Routing, url: &str) -> Result<Launch, String> {
    resolve(
        manager,
        routing.urls.first(),
        Pipeline::Url,
        &format!("URL \"{url}\""),
        std::collections::BTreeMap::from([(
            hydrus_core::external_calls::Parameter::Url,
            vec![url.to_owned()],
        )]),
    )
}
pub fn file(
    manager: &Manager,
    routing: &Routing,
    mime: Mime,
    path: &str,
    uri: &str,
    hash: &str,
    id: i64,
) -> Result<Launch, String> {
    use hydrus_core::external_calls::Parameter;
    resolve(
        manager,
        routing.file_calls(mime).first(),
        Pipeline::File,
        &format!("file \"{hash}\""),
        std::collections::BTreeMap::from([
            (Parameter::Path, vec![path.to_owned()]),
            (Parameter::Uri, vec![uri.to_owned()]),
            (Parameter::Hash, vec![hash.to_owned()]),
            (Parameter::FileId, vec![id.to_string()]),
        ]),
    )
}
/// Ordinary long-lived launches return after spawning; a reaper owns their child.
/// Bounded calls use the existing direct-argument executor and its saved timeout.
pub fn run_process(
    process: &hydrus_core::external_calls::Process,
    inputs: &hydrus_core::external_calls::Inputs,
) -> Result<(), String> {
    if !process.long_lived {
        return crate::external_calls::test_call(
            &hydrus_core::external_calls::ActualCall::Process(process.clone()),
            inputs,
        );
    }
    let arguments = process.command(inputs)?;
    let executable = arguments
        .first()
        .ok_or_else(|| "No executable path is set!".to_owned())?;
    let mut command = std::process::Command::new(executable);
    command
        .args(&arguments[1..])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    if process.hide_terminal {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let mut child=command.spawn().map_err(|error|format!("Problem running external local process! Final call list was \"{arguments:?}\", error was: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
