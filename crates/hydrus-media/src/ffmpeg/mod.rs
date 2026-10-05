//! Running ffmpeg, the reference's tool for everything audio/video.
//!
//! We shell out exactly like the reference does (same arguments, same parsing
//! of `ffmpeg -i` stderr) rather than linking libav, so results match the
//! reference given the same ffmpeg build.

pub(crate) mod parse;
pub(crate) mod video;

use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crate::error::{MediaError, Result};
use crate::text::python_splitlines;

/// How to run ffmpeg.
#[derive(Clone)]
pub struct Ffmpeg {
    exe: PathBuf,
    timeout: Duration,
    fixed_timeout: bool,
    timeout_reader: Option<std::sync::Arc<dyn Fn() -> Duration + Send + Sync>>,
}

impl std::fmt::Debug for Ffmpeg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ffmpeg")
            .field("exe", &self.exe)
            .field("timeout", &self.timeout)
            .field("fixed_timeout", &self.fixed_timeout)
            .finish_non_exhaustive()
    }
}

impl Default for Ffmpeg {
    fn default() -> Self {
        Self {
            exe: PathBuf::from(if cfg!(windows) {
                "ffmpeg.exe"
            } else {
                "ffmpeg"
            }),
            timeout: Duration::from_secs(15),
            fixed_timeout: false,
            timeout_reader: None,
        }
    }
}

/// Captured output of a finished process.
pub(crate) struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

const NULL_OUTPUT: &str = if cfg!(windows) { "NUL" } else { "/dev/null" };

impl Ffmpeg {
    /// Use a specific ffmpeg executable.
    pub fn with_executable(exe: impl Into<PathBuf>) -> Self {
        Self {
            exe: exe.into(),
            ..Self::default()
        }
    }

    /// Change how long one ffmpeg call may run (the reference uses 15s).
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self.fixed_timeout = true;
        self.timeout_reader = None;
        self
    }

    /// Supply an owned live policy for default-timeout calls. Explicit
    /// [`Self::timeout`] configurations keep their fixed deadline.
    #[must_use]
    pub fn with_timeout_reader(
        mut self,
        reader: std::sync::Arc<dyn Fn() -> Duration + Send + Sync>,
    ) -> Self {
        if !self.fixed_timeout {
            self.timeout_reader = Some(reader);
        }
        self
    }

    fn call_timeout(&self) -> Duration {
        self.timeout_reader
            .as_ref()
            .map_or(self.timeout, |reader| reader())
    }

    /// Read the version through the same bounded process path as metadata.
    /// Unrecognised version output has no version, as the About window expects.
    pub fn version(&self) -> Result<Option<String>> {
        let output = self.run(&["-version".into()])?;
        if output.timed_out {
            return Err(MediaError::damaged("ffmpeg took too long to respond"));
        }
        let text = String::from_utf8_lossy(&output.stdout);
        Ok(text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("ffmpeg version "))
            .map(|rest| rest.split(' ').next().unwrap_or(rest).to_owned()))
    }

    /// The executable this runs.
    pub fn executable(&self) -> &Path {
        &self.exe
    }

    fn command(&self, args: &[OsString]) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.args(args).stdin(Stdio::null());
        cmd
    }

    fn spawn(&self, mut cmd: Command) -> Result<Child> {
        cmd.spawn().map_err(|source| MediaError::FfmpegMissing {
            path: self.exe.clone(),
            source,
        })
    }

    /// Run to completion (or timeout), capturing stdout and stderr.
    pub(crate) fn run(&self, args: &[OsString]) -> Result<Output> {
        // Capture before spawning; edits cannot shorten or extend this process.
        let timeout = self.call_timeout();
        let mut cmd = self.command(args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = self.spawn(cmd)?;
        let (tx, rx) = mpsc::channel();
        let mut pipes: Vec<Box<dyn Read + Send>> = Vec::new();
        if let Some(out) = child.stdout.take() {
            pipes.push(Box::new(out));
        }
        if let Some(err) = child.stderr.take() {
            pipes.push(Box::new(err));
        }
        for (i, mut pipe) in pipes.into_iter().enumerate() {
            let tx = tx.clone();
            thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = pipe.read_to_end(&mut buf);
                let _ = tx.send((i, buf));
            });
        }
        drop(tx);
        let deadline = Instant::now() + timeout;
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut timed_out = false;
        for _ in 0..2 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining) {
                Ok((0, buf)) => stdout = buf,
                Ok((_, buf)) => stderr = buf,
                Err(_) => {
                    timed_out = true;
                    break;
                }
            }
        }
        if timed_out {
            let _ = child.kill();
        }
        let _ = child.wait();
        Ok(Output {
            stdout,
            stderr,
            timed_out,
        })
    }

    /// `HydrusFFMPEG.GetFFMPEGInfoLines`: the stripped stderr lines of `ffmpeg -i`.
    ///
    /// With `count_frames_mapping`, the stream is also decoded to a null
    /// output so the final `frame=` progress line gives an exact frame count.
    pub(crate) fn info_lines(
        &self,
        path: &Path,
        count_frames_mapping: Option<&str>,
    ) -> Result<Vec<String>> {
        let mut args: Vec<OsString> = vec!["-xerror".into(), "-i".into(), path.into()];
        if let Some(mapping) = count_frames_mapping {
            for a in [
                "-map",
                mapping,
                "-vf",
                "scale=-2:120",
                "-an",
                "-f",
                "null",
                NULL_OUTPUT,
            ] {
                args.push(a.into());
            }
        }
        let out = self.run(&args)?;
        if out.timed_out {
            return Err(MediaError::damaged(
                "ffmpeg could not read file info quick enough!",
            ));
        }
        let text = String::from_utf8_lossy(&out.stderr);
        if text.is_empty() {
            return Err(MediaError::FfmpegNoOutput);
        }
        let lines: Vec<String> = python_splitlines(&text)
            .into_iter()
            .map(|l| l.trim().to_owned())
            .collect();
        check_ffmpeg_error(&lines)?;
        Ok(lines)
    }

    /// Start a process whose raw stdout we consume incrementally.
    pub(crate) fn stream(&self, args: &[OsString], chunk_size: usize) -> Result<Stream> {
        let mut cmd = self.command(args);
        cmd.stdout(Stdio::piped()).stderr(Stdio::null());
        let mut child = self.spawn(cmd)?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| MediaError::damaged("ffmpeg stdout was not captured"))?;
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || read_chunks(stdout, chunk_size, &tx));
        Ok(Stream {
            child,
            chunks: rx,
            timeout: self.timeout,
        })
    }

    /// Run ffmpeg and return stdout (for renders to a pipe).
    pub(crate) fn render_to_stdout(&self, args: &[&OsStr]) -> Result<Vec<u8>> {
        let args: Vec<OsString> = args.iter().map(|a| (*a).to_owned()).collect();
        let out = self.run(&args)?;
        if out.timed_out {
            return Err(MediaError::damaged(
                "ffmpeg could not render it quick enough!",
            ));
        }
        Ok(out.stdout)
    }
}

fn read_chunks(mut stdout: impl Read, chunk_size: usize, tx: &mpsc::SyncSender<Vec<u8>>) {
    loop {
        let mut chunk = Vec::with_capacity(chunk_size);
        let got = (&mut stdout)
            .take(chunk_size as u64)
            .read_to_end(&mut chunk)
            .unwrap_or(0);
        let done = got < chunk_size;
        if got > 0 && tx.send(chunk).is_err() {
            return;
        }
        if done {
            return;
        }
    }
}

/// A running ffmpeg whose stdout arrives in fixed-size chunks (one video
/// frame, or a block of PCM audio). Killed on drop.
pub(crate) struct Stream {
    child: Child,
    chunks: mpsc::Receiver<Vec<u8>>,
    timeout: Duration,
}

impl Stream {
    /// The next chunk; empty at end of output. Only the last chunk may be short.
    pub(crate) fn next_chunk(&mut self) -> Result<Vec<u8>> {
        match self.chunks.recv_timeout(self.timeout) {
            Ok(chunk) => Ok(chunk),
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(Vec::new()),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                let _ = self.child.kill();
                Err(MediaError::damaged("ffmpeg timed out while rendering"))
            }
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn check_ffmpeg_error(lines: &[String]) -> Result<()> {
    let Some(last) = lines.last() else {
        return Err(MediaError::damaged(
            "Could not parse that file--no FFMPEG output given.",
        ));
    };
    if last.contains("No such file or directory") {
        return Err(MediaError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "File not found!",
        )));
    }
    if last.contains("Invalid data") {
        return Err(MediaError::damaged("FFMPEG could not parse."));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod deadline_tests {
    use super::*;
    use std::io::Write as _;
    use std::sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    };

    fn quoted(path: &Path) -> String {
        format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
    }
    fn transport(dir: &Path) -> (Ffmpeg, PathBuf, PathBuf) {
        use std::os::unix::fs::PermissionsExt as _;
        let fifo = dir.join("release");
        assert!(
            Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success()
        );
        let marker = dir.join("pid");
        let exe = dir.join("ffmpeg");
        std::fs::write(&exe, format!("#!/bin/sh\nprintf '%s' \"$$\" > {}\nread -r reply < {}\nif [ \"$1\" = '-version' ]; then printf 'ffmpeg version authored Copyright local\\n'; else printf 'ffmpeg version authored\\nInput #0, authored, from local:\\n' >&2; fi\n",quoted(&marker),quoted(&fifo))).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        (Ffmpeg::with_executable(exe), marker, fifo)
    }
    fn started(marker: &Path) -> String {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(pid) = std::fs::read_to_string(marker)
                && !pid.is_empty()
            {
                return pid;
            }
            assert!(
                Instant::now() < deadline,
                "real subprocess must publish its PID"
            );
            thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn actual_metadata_timeout_kills_and_reaps_the_admitted_child() {
        let dir = tempfile::tempdir().unwrap();
        let (ffmpeg, marker, _) = transport(dir.path());
        let error = ffmpeg
            .timeout(Duration::from_secs(1))
            .info_lines(Path::new("authored"), None)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "damaged or unusual file: ffmpeg could not read file info quick enough!"
        );
        let pid = started(&marker);
        let alive = Command::new("kill")
            .args(["-0", &pid])
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(
            !alive.success(),
            "return must follow kill and wait, leaving no live/zombie child"
        );
    }
    #[test]
    fn version_deadline_is_captured_once_and_explicit_fixed_configuration_wins() {
        let dir = tempfile::tempdir().unwrap();
        let (ffmpeg, marker, fifo) = transport(dir.path());
        let policy = Arc::new(AtomicU64::new(2_000));
        let reader = policy.clone();
        let ffmpeg = ffmpeg.with_timeout_reader(Arc::new(move || {
            Duration::from_millis(reader.load(Ordering::SeqCst))
        }));
        let current = ffmpeg.clone();
        let (done, result) = mpsc::channel();
        let worker = thread::spawn(move || done.send(current.version()).unwrap());
        started(&marker);
        policy.store(20, Ordering::SeqCst);
        assert!(
            matches!(
                result.recv_timeout(Duration::from_millis(60)),
                Err(mpsc::RecvTimeoutError::Timeout)
            ),
            "an already-running call must retain its original deadline"
        );
        writeln!(
            std::fs::OpenOptions::new().write(true).open(&fifo).unwrap(),
            "complete"
        )
        .unwrap();
        assert_eq!(
            result
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .unwrap(),
            Some("authored".into())
        );
        worker.join().unwrap();
        assert!(
            ffmpeg
                .version()
                .unwrap_err()
                .to_string()
                .contains("ffmpeg took too long to respond")
        );
        let fixed = ffmpeg
            .timeout(Duration::from_millis(90))
            .with_timeout_reader(Arc::new(|| Duration::ZERO));
        assert_eq!(fixed.call_timeout(), Duration::from_millis(90));
        let configured = Ffmpeg::default()
            .with_timeout_reader(Arc::new(|| Duration::from_secs(8)))
            .timeout(Duration::from_secs(7));
        assert_eq!(configured.call_timeout(), Duration::from_secs(7));
    }
}
