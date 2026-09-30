//! Parsing `ffmpeg -i` stderr, ported line-for-line from the reference
//! (`HydrusFFMPEGParsing`, `HydrusVideoHandling.ParseFFMPEG*`,
//! `HydrusAudioHandling.ParseFFMPEGAudio`).
//!
//! These heuristics are odd in places; they are kept as-is because the
//! values they produce (duration, frame count, resolution) are stored in the
//! database and must match files imported by the reference.

use std::sync::LazyLock;

use regex::Regex;

use crate::error::{MediaError, Result};

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex is valid")
}

static STREAM_MAPPING: LazyLock<Regex> = LazyLock::new(|| re(r"^Stream #(\d+:\d+)"));
static TBR: LazyLock<Regex> = LazyLock::new(|| re(r"( [0-9]*.| )[0-9]* tbr"));
static FPS: LazyLock<Regex> = LazyLock::new(|| re(r"( [0-9]*.| )[0-9]* fps"));
static RESOLUTION: LazyLock<Regex> = LazyLock::new(|| re(r" [0-9]*x[0-9]*[, ]"));
static SAR: LazyLock<Regex> = LazyLock::new(|| re(r"[\[\s]SAR [0-9]*:[0-9]*[,\s]"));
static ROTATION: LazyLock<Regex> =
    LazyLock::new(|| re(r"(displaymatrix:|Display Matrix:) rotation of -?90.00 degrees"));
static DURATION_LINE: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*Duration:"));
static START: LazyLock<Regex> = LazyLock::new(|| re(r"(start: )-?[0-9]+\.[0-9]*"));
static HMS: LazyLock<Regex> = LazyLock::new(|| re(r"[0-9]+:[0-9][0-9]:[0-9][0-9].[0-9][0-9]"));
static METADATA: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*Metadata:\s*"));
static MAJOR_BRAND: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*major_brand\s*:.+"));

/// `Stream ...: Video: ...` lines, including still-image streams such as cover art.
pub(crate) fn video_stream_lines(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .filter(|l| l.starts_with("Stream ") && l.contains("Video: "))
        .map(String::as_str)
        .collect()
}

pub(crate) fn audio_stream_lines(lines: &[String]) -> Vec<&str> {
    lines
        .iter()
        .filter(|l| l.starts_with("Stream ") && l.contains("Audio: "))
        .map(String::as_str)
        .collect()
}

/// Video streams that are really embedded images (cover art).
pub(crate) fn image_stream_lines(lines: &[String]) -> Vec<&str> {
    video_stream_lines(lines)
        .into_iter()
        .filter(|l| {
            l.contains("Video: png")
                || l.contains("Video: jpg")
                || l.contains("attached pic")
                || l.contains("attached_pic")
        })
        .collect()
}

/// `0:1` from `Stream #0:1[0x1](eng): Video: ...`.
pub(crate) fn stream_mapping(line: &str) -> Option<String> {
    STREAM_MAPPING
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_owned())
}

/// Python's `(?<=LABEL\s).+?(?=,)`: the text after `LABEL` + one whitespace
/// char, up to the next comma (at least one char).
fn codec_after(line: &str, label: &str) -> Option<String> {
    let mut search_from = 0;
    while let Some(found) = line[search_from..].find(label) {
        let after_label = search_from + found + label.len();
        let mut rest = line[after_label..].chars();
        if let Some(ws) = rest.next().filter(|c| c.is_whitespace()) {
            let start = after_label + ws.len_utf8();
            let mut chars = line[start..].char_indices();
            // `.+?` needs at least one character before the comma
            chars.next()?;
            return chars
                .find(|&(_, c)| c == ',')
                .map(|(i, _)| line[start..start + i].to_owned());
        }
        search_from = after_label;
    }
    None
}

/// What `ParseFFMPEGAudio` says about a file's audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AudioStreams {
    pub found: bool,
    pub format: String,
}

pub(crate) fn audio(lines: &[String]) -> AudioStreams {
    let audio_lines = audio_stream_lines(lines);
    let Some(first) = audio_lines.first() else {
        return AudioStreams {
            found: false,
            format: "unknown".into(),
        };
    };
    AudioStreams {
        found: true,
        format: codec_after(first, "Audio:").unwrap_or_else(|| "unknown".into()),
    }
}

fn parse_python_float(s: &str) -> Result<f64> {
    s.trim()
        .parse::<f64>()
        .map_err(|_| MediaError::damaged(format!("could not parse number {s:?}")))
}

fn fps_string_is_likely_stupid(fps: &str) -> Result<bool> {
    if fps.ends_with('k') {
        return Ok(true);
    }
    let f = parse_python_float(fps)?;
    #[allow(clippy::float_cmp)]
    Ok(f <= 1.0 || f == 100.0 || f > 144.0)
}

/// `ParseFFMPEGFPSPossibleResults`: candidate frame rates and whether they agree.
pub(crate) fn fps_possible_results(video_line: &str) -> Result<(Vec<f64>, bool)> {
    let mut results: Vec<f64> = Vec::new();
    for regex in [&*TBR, &*FPS] {
        if let Some(m) = regex.find(video_line) {
            let token = m.as_str().split(' ').nth(1).unwrap_or("");
            if !fps_string_is_likely_stupid(token)? {
                let value = parse_python_float(token)?;
                if !results.contains(&value) {
                    results.push(value);
                }
            }
        }
    }
    results.retain(|v| *v != 0.0);
    let confident = if results.is_empty() {
        false
    } else {
        let max = results.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if results.iter().all(|&p| p >= max * 0.95) {
            true
        } else {
            results.len() <= 1
        }
    };
    Ok((results, confident))
}

/// `ParseFFMPEGVideoLine`: the stream line describing the real video.
///
/// `Ok(None)` is the reference's "Could not find video information!".
pub(crate) fn video_line(lines: &[String], png_ok: bool) -> Result<Option<&str>> {
    let bad: &[&str] = if png_ok {
        &["Video: jpg"]
    } else {
        &["Video: png", "Video: jpg"]
    };
    let candidates: Vec<&str> = video_stream_lines(lines)
        .into_iter()
        .filter(|l| !bad.iter().any(|b| l.contains(b)))
        .collect();
    if candidates.is_empty() {
        return Ok(None);
    }
    // multi-track avifs/heifs list a still image before the real sequence
    for line in &candidates {
        let (mut results, confident) = fps_possible_results(line)?;
        results.retain(|v| v.total_cmp(&1.0).is_ne());
        if !results.is_empty() && confident {
            return Ok(Some(line));
        }
    }
    Ok(Some(candidates[0]))
}

/// `ParseFFMPEGHasVideo`.
pub(crate) fn has_video(lines: &[String]) -> Result<bool> {
    Ok(video_line(lines, false)?.is_some())
}

/// `ParseFFMPEGVideoFormat`: (has video, codec name, stream mapping).
pub(crate) fn video_format(lines: &[String]) -> Result<(bool, String, String)> {
    let default_mapping = "0:0".to_owned();
    let Some(line) = video_line(lines, false)? else {
        return Ok((false, "unknown".into(), default_mapping));
    };
    let mut format = "unknown".to_owned();
    if let Some(f) = codec_after(line, "Video:") {
        if f.starts_with("none") {
            return Ok((false, "none".into(), default_mapping));
        }
        format = f;
    }
    let mapping = stream_mapping(line).unwrap_or(default_mapping);
    Ok((true, format, mapping))
}

/// `ParseFFMPEGFPS`: (fps, confident).
pub(crate) fn fps(lines: &[String]) -> Result<(f64, bool)> {
    let err = || MediaError::damaged("Error estimating framerate!");
    let line = video_line(lines, false)
        .map_err(|_| err())?
        .ok_or_else(err)?;
    let (results, confident) = fps_possible_results(line).map_err(|_| err())?;
    if results.is_empty() {
        Ok((1.0, false))
    } else {
        Ok((
            results.iter().copied().fold(f64::INFINITY, f64::min),
            confident,
        ))
    }
}

fn python_int(s: &str) -> Option<i64> {
    s.trim().parse().ok()
}

/// `ParseFFMPEGVideoResolution`: display resolution (SAR and rotation applied).
pub(crate) fn video_resolution(lines: &[String], png_ok: bool) -> Result<(u32, u32)> {
    let err = || MediaError::damaged("Error parsing resolution!");
    let line = video_line(lines, png_ok)
        .map_err(|_| err())?
        .ok_or_else(err)?;
    let m = RESOLUTION.find(line).ok_or_else(err)?;
    let text = &line[m.start()..m.end() - 1];
    let (w, h) = text.split_once('x').ok_or_else(err)?;
    let mut width = python_int(w).ok_or_else(err)?;
    let height = python_int(h).ok_or_else(err)?;
    if let Some(sar) = SAR.find(line) {
        let s = &sar.as_str()[5..sar.as_str().len() - 1];
        let (sw, sh) = s.split_once(':').ok_or_else(err)?;
        let sw = python_int(sw).ok_or_else(err)?;
        let sh = python_int(sh).ok_or_else(err)?;
        if sh == 0 {
            return Err(err());
        }
        width = (width * sw).div_euclid(sh);
    }
    let (mut width, mut height) = (
        u32::try_from(width).map_err(|_| err())?,
        u32::try_from(height).map_err(|_| err())?,
    );
    if lines.iter().any(|l| ROTATION.is_match(l)) {
        std::mem::swap(&mut width, &mut height);
    }
    Ok((width, height))
}

/// `ParseFFMPEGDuration`: (file duration, stream duration) in seconds.
pub(crate) fn duration(lines: &[String]) -> Result<(Option<f64>, Option<f64>)> {
    let err = || MediaError::damaged("Error reading duration!");
    let line = lines
        .iter()
        .find(|l| DURATION_LINE.is_match(l))
        .ok_or_else(err)?;
    if line.contains("Duration: N/A") {
        return Ok((None, None));
    }
    let mut start_offset = if line.contains("start:") {
        let m = START.find(line).ok_or_else(err)?;
        parse_python_float(&line[m.start() + 7..m.end()]).map_err(|_| err())?
    } else {
        0.0
    };
    let m = HMS.find(line).ok_or_else(err)?;
    let hms: Vec<f64> = m
        .as_str()
        .split(':')
        .map(parse_python_float)
        .collect::<Result<_>>()
        .map_err(|_| err())?;
    let duration_s = match hms.as_slice() {
        [s] => *s,
        [m, s] => 60.0 * m + s,
        [h, m, s] => 3600.0 * h + 60.0 * m + s,
        _ => 0.0,
    };
    if duration_s == 0.0 {
        return Ok((None, None));
    }
    if start_offset > 0.85 * duration_s {
        return Ok((None, None));
    }
    if start_offset > 1.0 {
        start_offset = 0.0;
    }
    Ok((Some(duration_s + start_offset), Some(duration_s)))
}

/// `ParseFFMPEGMimeText`: the demuxer names, e.g. `matroska,webm`.
pub(crate) fn mime_text(lines: &[String]) -> Result<String> {
    let inputs: Vec<&String> = lines.iter().filter(|l| l.starts_with("Input #0")).collect();
    let [input] = inputs.as_slice() else {
        return Err(MediaError::damaged("Error reading file type!"));
    };
    let text: String = input.chars().skip(10).collect();
    Ok(text.split(", from").next().unwrap_or("").to_owned())
}

/// `ParseFFMPEGMetadataContainer`: the mp4 `major_brand`, or empty.
pub(crate) fn metadata_container(lines: &[String]) -> String {
    let Some(start) = lines.iter().position(|l| METADATA.is_match(l)) else {
        return String::new();
    };
    lines[start..]
        .iter()
        .find(|l| MAJOR_BRAND.is_match(l))
        .and_then(|l| l.split_once(':'))
        .map(|(_, v)| v.trim().to_owned())
        .unwrap_or_default()
}

/// `ParseFFMPEGNumFramesManually`: the final `frame=` progress count.
pub(crate) fn num_frames_manually(lines: &[String]) -> Result<u64> {
    let Some(last) = lines.iter().rev().find(|l| l.starts_with("frame=")) else {
        return Err(MediaError::damaged(
            "Video appears to be broken and non-renderable--perhaps a damaged single-frame video?",
        ));
    };
    let line = last.replace("frame=", "");
    let line = line.trim_start_matches(' ');
    line.split(' ')
        .next()
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| {
            MediaError::damaged(format!(
                "Video was unable to render correctly--could not parse ffmpeg output line: \"{last}\""
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(text: &str) -> Vec<String> {
        text.lines().map(|l| l.trim().to_owned()).collect()
    }

    const MP4: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'x.mp4':
  Metadata:
    major_brand     : isom
    minor_version   : 512
  Duration: 00:00:02.46, start: 0.033000, bitrate: 1069 kb/s
  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(progressive), 1280x720 [SAR 69:80 DAR 23:15], 1000 kb/s, 29.97 fps, 29.97 tbr, 30k tbn (default)
  Stream #0:1[0x2](und): Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, stereo, fltp, 128 kb/s (default)
    Side data:
      displaymatrix: rotation of -90.00 degrees
At least one output file must be specified";

    #[test]
    fn parses_a_typical_mp4() {
        let l = lines(MP4);
        assert_eq!(mime_text(&l).unwrap(), "mov,mp4,m4a,3gp,3g2,mj2");
        assert_eq!(metadata_container(&l), "isom");
        let (file, stream) = duration(&l).unwrap();
        assert_eq!(stream, Some(2.46));
        assert_eq!(file, Some(2.46 + 0.033));
        assert_eq!(fps(&l).unwrap(), (29.97, true));
        // 1280 * 69 / 80 = 1104, then rotated
        assert_eq!(video_resolution(&l, false).unwrap(), (720, 1104));
        assert_eq!(
            video_format(&l).unwrap(),
            (
                true,
                "h264 (High) (avc1 / 0x31637661)".to_owned(),
                "0:0".to_owned()
            )
        );
        assert_eq!(
            audio(&l),
            AudioStreams {
                found: true,
                format: "aac (LC) (mp4a / 0x6134706D)".into()
            }
        );
    }

    #[test]
    fn frame_counts_and_mappings() {
        let l = lines("frame=    1 fps=0.0 q=-0.0 size=N/A\nframe=  123 fps=0.0 q=-0.0 Lsize=N/A");
        assert_eq!(num_frames_manually(&l).unwrap(), 123);
        assert_eq!(
            stream_mapping("Stream #0:1[0x1](eng): Video: hevc").as_deref(),
            Some("0:1")
        );
    }

    #[test]
    fn stupid_fps_values_are_dropped() {
        let (r, c) =
            fps_possible_results("Video: gif, bgra, 10x10, 100 fps, 100 tbr, 100 tbn").unwrap();
        assert!(r.is_empty());
        assert!(!c);
        let (r, c) = fps_possible_results("Video: x, 10x10, 25 fps, 50 tbr, 1k tbn").unwrap();
        assert_eq!(r, vec![50.0, 25.0]);
        assert!(!c);
    }
}
