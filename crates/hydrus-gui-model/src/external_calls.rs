//! Detached Options call list and local execution of explicitly tested calls.
//! Parent drafts own keys and selection; direct process calls use argument vectors.
use crate::list_selection::ListSelection;
use hydrus_core::external_calls::{ActualCall, Callable, Inputs, Manager, Process};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};

/// Staged external-call rows, preserving selection across sorting and edits.
#[derive(Debug, Clone)]
pub struct Table {
    pub manager: Manager,
    pub selection: ListSelection<[u8; 32]>,
    pub sort_column: usize,
    pub ascending: bool,
}
impl Table {
    /// Begin an independent Options transaction.
    pub fn new(manager: Manager) -> Self {
        let mut table = Self {
            manager,
            selection: ListSelection::default(),
            sort_column: 1,
            ascending: true,
        };
        table.sort(1, true);
        table
    }
    /// Sort by the clicked reference name/job/command column.
    pub fn sort(&mut self, column: usize, ascending: bool) {
        self.sort_column = column.min(2);
        self.ascending = ascending;
        self.manager.calls.sort_by(|a, b| {
            let key = |c: &Callable| {
                let tuple = [c.name.as_str(), c.pipeline.label(), &c.call.description()]
                    .map(hydrus_core::casefold::casefold);
                (tuple[self.sort_column].clone(), tuple)
            };
            let order = key(a).cmp(&key(b));
            if ascending { order } else { order.reverse() }
        });
    }
    /// Extended selection, retaining call identity rather than row positions.
    pub fn click(&mut self, index: usize, ctrl: bool, shift: bool) {
        let order = self.manager.calls.iter().map(|c| c.key).collect::<Vec<_>>();
        self.selection.click(&order, index, ctrl, shift);
    }
    /// Selected call snapshots in visible order.
    pub fn selected(&self) -> Vec<Callable> {
        self.manager
            .calls
            .iter()
            .filter(|c| self.selection.is_selected(c.key))
            .cloned()
            .collect()
    }
    /// Add/import/duplicate with a fresh key and a nonduplicate name.
    pub fn add(&mut self, call: Callable) -> [u8; 32] {
        let key = self.append(call);
        self.sort(self.sort_column, self.ascending);
        key
    }
    /// Append an import prefix without sorting until the complete batch succeeds.
    pub fn append(&mut self, mut call: Callable) -> [u8; 32] {
        self.nondupe_name(&mut call, None);
        call.regenerate_key();
        let key = call.key;
        self.manager.calls.push(call);
        key
    }
    fn nondupe_name(&self, call: &mut Callable, except: Option<[u8; 32]>) {
        let original = call.name.clone();
        let mut n = 1;
        while self
            .manager
            .calls
            .iter()
            .any(|c| Some(c.key) != except && c.name == call.name)
        {
            call.name = format!("{original} ({n})");
            n += 1;
        }
    }
    /// Accept a child edit while retaining the original call key.
    pub fn replace(&mut self, key: [u8; 32], mut call: Callable) -> bool {
        if !self.manager.calls.iter().any(|c| c.key == key) {
            return false;
        }
        call.key = key;
        self.nondupe_name(&mut call, Some(key));
        if let Some(row) = self.manager.calls.iter_mut().find(|c| c.key == key) {
            *row = call;
        }
        self.sort(self.sort_column, self.ascending);
        true
    }
    /// Duplicate each selected call through the same fresh-identity path.
    pub fn duplicate(&mut self) {
        self.add_selected(self.selected());
    }
    /// Import/default/duplicate rows become selected alongside the prior selection.
    pub fn add_selected(&mut self, calls: Vec<Callable>) {
        let mut selected = self.selected().iter().map(|c| c.key).collect::<Vec<_>>();
        for call in calls {
            selected.push(self.add(call));
        }
        self.selection.select_many(&selected);
    }
    /// Delete the selected snapshots only after their captured confirmation.
    pub fn delete(&mut self, keys: &[[u8; 32]]) {
        self.manager.calls.retain(|c| !keys.contains(&c.key));
        for key in keys {
            self.selection.forget(*key);
        }
    }
}
fn executable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        true
    }
}
/// Resolve an executable using the client's PATH, without running it.
pub fn executable_path(name: &str) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    let path = Path::new(name);
    if path.is_absolute() || path.components().count() > 1 {
        return executable(path).then(|| path.to_owned());
    }
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        let candidate = dir.join(name);
        if executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        if candidate.extension().is_none() {
            let extensions =
                std::env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
            for extension in extensions.split(';') {
                let file = dir.join(format!("{name}{extension}"));
                if executable(&file) {
                    return Some(file);
                }
            }
        }
    }
    None
}
/// Reference availability test, enabled only for a general process call.
pub fn available(call: &ActualCall) -> bool {
    match call {
        ActualCall::Process(p) => executable_path(&p.executable).is_some(),
        ActualCall::DefaultFile | ActualCall::DefaultUrl => true,
    }
}
fn process_command(process: &Process, inputs: &Inputs) -> Result<Command, String> {
    let args = process.command(inputs)?;
    let executable = args
        .first()
        .ok_or_else(|| "No executable path is set!".to_owned())?;
    let mut command = Command::new(executable);
    command.args(&args[1..]);
    #[cfg(windows)]
    if process.hide_terminal {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    Ok(command)
}
/// Run an explicitly requested test with a finite deadline. Detached calls use
/// the reference's forced 15-second test timeout. Current reference calls return
/// no response parameters, so stdout/stderr are discarded with bounded storage.
pub fn test_call(call: &ActualCall, inputs: &Inputs) -> Result<(), String> {
    test_call_cancellable(call, inputs, &Arc::new(AtomicBool::new(false)))
}
/// Owner-scoped worker: cancellation kills and reaps its direct child. Descendant
/// process groups are not owned; a launched program remains responsible for them.
pub fn test_call_cancellable(
    call: &ActualCall,
    inputs: &Inputs,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    let ActualCall::Process(process) = call else {
        return Err("The OS launcher test is not supported by this native test panel.".into());
    };
    let mut command = process_command(process, inputs)?;
    command.stdout(Stdio::null()).stderr(Stdio::null());
    if cancel.load(Ordering::Acquire) {
        return Err("External call cancelled.".into());
    }
    let mut child = command.spawn().map_err(|e| {
        format!(
            "Problem running external local process! Final call list was \"{:?}\", error was: {e}",
            process.command(inputs).unwrap_or_default()
        )
    })?;
    let timeout = Duration::from_secs(if process.long_lived {
        15
    } else {
        process.timeout_seconds
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(format!("External local process returned {status}."))
                };
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.to_string());
            }
        }
        if cancel.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("External call cancelled.".into());
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "External local process timed out after {} seconds.",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
