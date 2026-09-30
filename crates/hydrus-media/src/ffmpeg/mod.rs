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
#[derive(Debug, Clone)]
pub struct Ffmpeg {
    exe: PathBuf,
    timeout: Duration,
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
        self
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
        let deadline = Instant::now() + self.timeout;
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
