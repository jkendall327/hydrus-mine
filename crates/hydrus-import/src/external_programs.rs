//! The external programs an import runs on each file it imports
//! (`FileSeed.DoExternalProgramCalls` and `SendSingleFile`).

use std::io::Read as _;
use std::path::Path;
use std::time::{Duration, Instant};

use hydrus_core::Sha256;
use hydrus_core::external_calls::{ActualCall, Inputs, Manager, Parameter, Pipeline, Process};
use hydrus_core::import_options::ExternalProgramEntry;

use crate::{FileImporter, FileImportOptions, ImportResult, ImportStatus};

/// `Path.as_uri()`: the file's `file://` URI, escaped as Python escapes it.
fn file_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut uri = String::from(if text.starts_with('/') {
        "file://"
    } else {
        "file:///"
    });
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~/:".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// The reference's `IdAndName.__str__`.
fn id_and_name(entry: &ExternalProgramEntry) -> String {
    format!("\"{}\" (id {})", entry.name, entry.key)
}

impl FileImporter {
    /// Run the programs a successfully imported file's options name
    /// (`DoExternalProgramCalls`): each entry that fires for the file being
    /// new or already in the client, in the order they were added. A program
    /// that fails stops the rest and pauses every importer (subscriptions,
    /// import folders and file queues), as the reference does.
    pub(crate) fn run_external_programs(
        &self,
        result: &ImportResult,
        hash: &Sha256,
        options: &FileImportOptions,
    ) {
        let already_in = match result.status {
            ImportStatus::SuccessfulAndNew => false,
            ImportStatus::SuccessfulButRedundant => true,
            _ => return,
        };
        self.fire_external_programs(hash, already_in, options);
    }

    /// [`Self::run_external_programs`] for a file known to be in the client
    /// already (`already_in`), or just imported (not).
    pub fn fire_external_programs(
        &self,
        hash: &Sha256,
        already_in: bool,
        options: &FileImportOptions,
    ) {
        let entries: Vec<&ExternalProgramEntry> = options
            .external_programs
            .iter()
            .filter(|entry| entry.should_fire(already_in))
            .collect();
        if entries.is_empty() {
            return;
        }
        let manager: Manager = self.store.read(hydrus_store::settings::get).unwrap_or_default();
        let target = self.external_program_target(hash);
        for entry in entries {
            let outcome = target
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|(path, id)| send_single_file(&manager, entry, hash, path, *id));
            if let Err(error) = outcome {
                tracing::error!(
                    "Failed to run \"{}\" on file with hash \"{hash}\"! All importers--subscriptions, import folders, and paged file import queues--have been paused. Once the issue is clear, restart the client and resume your imports under the file and network menus!\n{error}",
                    entry.name
                );
                self.pause_importers();
                return;
            }
        }
    }

    /// Where the file is stored and its id.
    fn external_program_target(&self, hash: &Sha256) -> Result<(std::path::PathBuf, i64), String> {
        let id = self
            .store
            .read(|conn| hydrus_store::master::hash_id(conn, hash))
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "This file is not local--it cannot be opened!".to_owned())?;
        let mime = self
            .store
            .read(|conn| {
                Ok(hydrus_store::media::load_basic(conn, &[id])?
                    .into_iter()
                    .next()
                    .and_then(|m| m.info)
                    .map(|info| info.mime))
            })
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "This file is not local--it cannot be opened!".to_owned())?;
        let path = self
            .store
            .snapshot()
            .storage
            .file_path(hash, mime)
            .ok_or_else(|| "This file is not local--it cannot be opened!".to_owned())?;
        Ok((path, i64::from(id.0)))
    }
}

/// `SendSingleFile`.
fn send_single_file(
    manager: &Manager,
    entry: &ExternalProgramEntry,
    hash: &Sha256,
    path: &Path,
    id: i64,
) -> Result<(), String> {
    let missing = || {
        format!(
            "When trying to open file \"{hash}\" externally, the executable we wanted to call ({}) did not exist!",
            id_and_name(entry)
        )
    };
    let call = manager
        .calls
        .iter()
        .find(|call| {
            call.key.iter().map(|b| format!("{b:02x}")).collect::<String>()
                == entry.key.to_lowercase()
        })
        .ok_or_else(missing)?;
    if call.pipeline != Pipeline::File {
        return Err(format!(
            "When trying to open file \"{hash}\" externally, the executable we wanted to call ({}) was the wrong type ({})!",
            id_and_name(entry),
            call.pipeline.label()
        ));
    }
    let path_text = path.to_string_lossy().into_owned();
    let inputs: Inputs = [
        (Parameter::Path, vec![path_text.clone()]),
        (Parameter::Uri, vec![file_uri(path)]),
        (Parameter::Hash, vec![hash.to_hex()]),
        (Parameter::FileId, vec![id.to_string()]),
    ]
    .into_iter()
    .collect();
    match &call.call {
        ActualCall::Process(process) => run_process(process, &inputs),
        ActualCall::DefaultFile => open_with_the_os(&path_text),
        ActualCall::DefaultUrl => Err("This call opens URLs, not files.".into()),
    }
}

/// `ExecutableLocalProcessCall._DoCall`: run it, waiting for it (and failing
/// if it takes too long or returns non-zero) unless it may be long-lived.
fn run_process(process: &Process, inputs: &Inputs) -> Result<(), String> {
    let cmd = process.command(inputs)?;
    let (program, arguments) = cmd
        .split_first()
        .filter(|(program, _)| !program.is_empty())
        .ok_or_else(|| "No executable path is set!".to_owned())?;
    let problem = |error: &dyn std::fmt::Display| {
        format!(
            "Problem running external local process! Final call list was \"{cmd:?}\", error was: {error}"
        )
    };
    let mut command = std::process::Command::new(program);
    command.args(arguments);
    #[cfg(windows)]
    if process.hide_terminal {
        use std::os::windows::process::CommandExt as _;
        command.creation_flags(0x0800_0000);
    }
    if process.long_lived {
        command
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let mut child = command.spawn().map_err(|e| problem(&e))?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return Ok(());
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command.spawn().map_err(|e| problem(&e))?;
    let read = |pipe: Option<Box<dyn std::io::Read + Send>>| {
        std::thread::spawn(move || {
            let mut text = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.by_ref().take(1 << 16).read_to_end(&mut text);
                let _ = std::io::copy(&mut pipe, &mut std::io::sink());
            }
            String::from_utf8_lossy(&text).into_owned()
        })
    };
    let stdout = read(child.stdout.take().map(|p| Box::new(p) as _));
    let stderr = read(child.stderr.take().map(|p| Box::new(p) as _));
    let deadline = Instant::now() + Duration::from_secs(process.timeout_seconds);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    return Ok(());
                }
                let text = |reader: std::thread::JoinHandle<String>| {
                    reader.join().unwrap_or_default().chars().take(256).collect::<String>()
                };
                return Err(format!(
                    "A call to another executable gave a non-zero return code ({})! The call was: {cmd:?}\n\n========== stdout ==========\n{:?}\n========== stderr ==========\n{:?}\n============================\n",
                    status.code().map_or_else(|| "none".to_owned(), |c| c.to_string()),
                    text(stdout),
                    text(stderr),
                ));
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "A call to another executable timed out after {} seconds! The call was: {cmd:?}",
                    process.timeout_seconds
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(error) => return Err(problem(&error)),
        }
    }
}

/// The operating system's own way to open a file.
fn open_with_the_os(path: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut command = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", ""]).arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    #[cfg(not(any(windows, target_os = "macos")))]
    let mut command = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|e| format!("Problem running external local process! Error was: {e}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
