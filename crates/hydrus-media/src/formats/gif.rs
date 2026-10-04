//! GIF structure as Pillow's `GifImagePlugin` walks it: whether a file is
//! animated (a second image descriptor exists), per-frame durations, and the
//! first frame's pixels.

use crate::error::{MediaError, Result};

/// One frame's header information.
#[derive(Debug, Clone)]
pub(crate) struct Frame {
    pub extent: (u32, u32, u32, u32),
    /// `info['duration']` for this frame (GCE delay * 10), if it has a GCE.
    pub duration_ms: Option<u32>,
    pub transparency: Option<u8>,
    pub local_palette: LocalPalette,
    pub interlaced: bool,
    /// GCE disposal bits (0 = unspecified).
    pub disposal: u8,
    pub lzw_min_code_size: u8,
    /// Offset of the first image data sub-block.
    pub data_offset: usize,
}

/// A frame's local colour table, as Pillow sees it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocalPalette {
    /// No local table: the global one applies.
    Absent,
    /// A greyscale-identity table, which Pillow treats as "no palette".
    Identity,
    /// A real table (RGB triples).
    Colours(Vec<u8>),
}

/// A parsed GIF: logical screen, global palette and the frames Pillow finds.
#[derive(Debug, Clone)]
pub(crate) struct Gif {
    pub screen: (u32, u32),
    pub global_palette: Option<Vec<u8>>,
    pub frames: Vec<Frame>,
    pub times_to_play: u32,
}

struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Cursor<'_> {
    fn read(&mut self, n: usize) -> &[u8] {
        let end = (self.pos + n).min(self.data.len());
        let s = &self.data[self.pos.min(end)..end];
        self.pos = end;
        s
    }

    fn byte(&mut self) -> Option<u8> {
        self.read(1).first().copied()
    }

    /// `GifImageFile.data()`: the next sub-block, or `None` at a terminator/EOF.
    fn sub_block(&mut self) -> Option<Vec<u8>> {
        let n = self.byte()?;
        if n == 0 {
            return None;
        }
        Some(self.read(usize::from(n)).to_vec())
    }
}

/// `_is_palette_needed`: false for a palette that is just the grey ramp.
fn palette_needed(p: &[u8]) -> bool {
    p.chunks(3)
        .enumerate()
        .any(|(i, c)| c.len() < 3 || !(i == usize::from(c[0]) && c[0] == c[1] && c[1] == c[2]))
}

fn le16(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?])))
}

enum Step {
    Frame(Frame),
    End,
    Broken,
}

/// One `_seek` iteration: read extensions until an image descriptor.
fn next_frame(
    c: &mut Cursor<'_>,
    size: &mut (u32, u32),
    first: bool,
    times_to_play: &mut u32,
) -> Step {
    let mut s = c.byte();
    match s {
        None | Some(b';') => return Step::End,
        _ => {}
    }
    let mut duration = None;
    let mut transparency = None;
    let mut disposal = 0;
    loop {
        if s.is_none() {
            s = c.byte();
        }
        match s {
            None | Some(b';') => return Step::End,
            Some(b'!') => {
                let Some(label) = c.byte() else {
                    return Step::Broken;
                };
                let block = c.sub_block();
                if label == 249 {
                    if let Some(b) = &block {
                        let (Some(&flags), Some(delay)) = (b.first(), le16(b, 1)) else {
                            return Step::Broken;
                        };
                        if flags & 1 != 0 {
                            let Some(&t) = b.get(3) else {
                                return Step::Broken;
                            };
                            transparency = Some(t);
                        }
                        duration = Some(delay * 10);
                        disposal = (flags & 0b0001_1100) >> 2;
                    }
                } else if label == 254 {
                    let mut block = block;
                    while block.is_some() {
                        block = c.sub_block();
                    }
                    s = None;
                    continue;
                } else if label == 255
                    && first
                    && block
                        .as_deref()
                        .is_some_and(|b| b.starts_with(b"NETSCAPE2.0"))
                {
                    // Pillow exposes the stored count directly (0 infinite).
                    if let Some(block) = c.sub_block()
                        && block.first() == Some(&1)
                        && let Some(count) = le16(&block, 1)
                    {
                        *times_to_play = count;
                    }
                }
                while c.sub_block().is_some() {}
            }
            Some(b',') => {
                let d = c.read(9).to_vec();
                if d.len() < 9 {
                    return Step::Broken;
                }
                let (x0, y0) = (le16(&d, 0).unwrap_or(0), le16(&d, 2).unwrap_or(0));
                let (x1, y1) = (x0 + le16(&d, 4).unwrap_or(0), y0 + le16(&d, 6).unwrap_or(0));
                if x1 > size.0 || y1 > size.1 {
                    *size = (x1.max(size.0), y1.max(size.1));
                }
                let flags = d[8];
                let local_palette = if flags & 128 != 0 {
                    let bits = (flags & 7) + 1;
                    let p = c.read(3 << bits).to_vec();
                    if palette_needed(&p) {
                        LocalPalette::Colours(p)
                    } else {
                        LocalPalette::Identity
                    }
                } else {
                    LocalPalette::Absent
                };
                let Some(lzw) = c.byte() else {
                    return Step::Broken;
                };
                return Step::Frame(Frame {
                    extent: (x0, y0, x1, y1),
                    duration_ms: duration,
                    transparency,
                    local_palette,
                    interlaced: flags & 64 != 0,
                    disposal,
                    lzw_min_code_size: lzw,
                    data_offset: c.pos,
                });
            }
            Some(_) => {}
        }
        s = None;
    }
}

/// Walk the file the way Pillow's `seek`/`n_frames` do.
pub(crate) fn parse(data: &[u8]) -> Result<Gif> {
    if data.len() < 13 || !(data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a")) {
        return Err(MediaError::damaged("not a GIF file"));
    }
    let mut size = (le16(data, 6).unwrap_or(0), le16(data, 8).unwrap_or(0));
    let flags = data[10];
    let mut c = Cursor { data, pos: 13 };
    let mut global_palette = None;
    if flags & 128 != 0 {
        let bits = (flags & 7) + 1;
        let p = c.read(3 << bits).to_vec();
        if palette_needed(&p) {
            global_palette = Some(p);
        }
    }
    let mut frames: Vec<Frame> = Vec::new();
    let mut times_to_play = 1;
    loop {
        if let Some(prev) = frames.last() {
            // skip the previous frame's image data
            c.pos = prev.data_offset;
            while c.sub_block().is_some() {}
        }
        match next_frame(&mut c, &mut size, frames.is_empty(), &mut times_to_play) {
            Step::Frame(f) => frames.push(f),
            // Pillow stops at the first frame it cannot read
            Step::End | Step::Broken => break,
        }
    }
    if frames.is_empty() {
        return Err(MediaError::damaged("image not found in GIF frame"));
    }
    Ok(Gif {
        screen: (le16(data, 6).unwrap_or(0), le16(data, 8).unwrap_or(0)),
        global_palette,
        frames,
        times_to_play,
    })
}

/// Pillow's `is_animated` for GIFs.
pub(crate) fn is_animated(data: &[u8]) -> bool {
    parse(data).is_ok_and(|g| g.frames.len() > 1)
}

/// The size Pillow reports after opening (frame 0 may enlarge the screen).
pub(crate) fn open_size(gif: &Gif) -> (u32, u32) {
    let (x1, y1) = (gif.frames[0].extent.2, gif.frames[0].extent.3);
    (gif.screen.0.max(x1), gif.screen.1.max(y1))
}

/// `GetFrameDurationsMSPILAnimation` for a GIF.
pub(crate) fn frame_durations_ms(gif: &Gif) -> Vec<u64> {
    const FALLBACK: u64 = 83;
    let initial = open_size(gif);
    let mut size = initial;
    let mut durations: Vec<u64> = Vec::new();
    for (i, frame) in gif.frames.iter().enumerate() {
        let too_big = size.0 > 16384 || size.1 > 16384;
        if too_big || size != initial {
            let fill = if durations.is_empty() {
                FALLBACK
            } else {
                durations.iter().sum::<u64>() / durations.len() as u64
            };
            durations.resize(gif.frames.len(), fill);
            return durations;
        }
        if i > 0 {
            let (x1, y1) = (frame.extent.2, frame.extent.3);
            size = (size.0.max(x1), size.1.max(y1));
        }
        durations.push(match frame.duration_ms {
            None | Some(0 | 10) => FALLBACK,
            Some(d) => u64::from(d),
        });
    }
    durations
}

/// Frame 0 as Pillow holds it: palette indices (P) or grey levels (L).
#[derive(Debug, Clone)]
pub(crate) struct FirstFrame {
    pub width: u32,
    pub height: u32,
    pub indices: Vec<u8>,
    /// RGB palette bytes, or `None` for greyscale (L) mode.
    pub palette: Option<Vec<u8>>,
    pub transparency: Option<u8>,
}

fn lzw_decode(data: &[u8], frame: &Frame) -> Result<Vec<u8>> {
    let mut c = Cursor {
        data,
        pos: frame.data_offset,
    };
    let mut stream = Vec::new();
    while let Some(b) = c.sub_block() {
        stream.extend_from_slice(&b);
    }
    let size = frame.lzw_min_code_size;
    if !(1..=12).contains(&size) {
        return Err(MediaError::damaged("bad GIF LZW code size"));
    }
    let mut decoder = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, size.max(2));
    let mut out = Vec::new();
    let result = decoder.into_vec(&mut out).decode(&stream);
    if result.status.is_err() && out.is_empty() {
        return Err(MediaError::damaged("bad GIF image data"));
    }
    Ok(out)
}

/// Decode a frame's indices onto a canvas of `w` columns (already filled).
fn draw_frame(data: &[u8], frame: &Frame, canvas: &mut [u8], w: u32) -> Result<()> {
    let decoded = lzw_decode(data, frame)?;
    let (x0, y0, x1, y1) = frame.extent;
    let fw = (x1 - x0) as usize;
    let fh = (y1 - y0) as usize;
    let rows: Vec<usize> = if frame.interlaced {
        let mut r = Vec::with_capacity(fh);
        for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
            r.extend((start..fh).step_by(step));
        }
        r
    } else {
        (0..fh).collect()
    };
    if fw > 0 {
        for (row_index, chunk) in rows.iter().zip(decoded.chunks(fw)) {
            let y = y0 as usize + row_index;
            let start = y * w as usize + x0 as usize;
            canvas[start..start + chunk.len()].copy_from_slice(chunk);
        }
    }
    Ok(())
}

pub(crate) fn first_frame(data: &[u8]) -> Result<FirstFrame> {
    let gif = parse(data)?;
    let frame = &gif.frames[0];
    let (w, h) = open_size(&gif);
    let palette = match &frame.local_palette {
        LocalPalette::Absent => gif.global_palette.clone(),
        LocalPalette::Identity => None,
        LocalPalette::Colours(p) => Some(p.clone()),
    };
    let fill = frame.transparency.unwrap_or(0);
    let mut indices = vec![fill; w as usize * h as usize];
    draw_frame(data, frame, &mut indices, w)?;
    Ok(FirstFrame {
        width: w,
        height: h,
        indices,
        palette,
        transparency: frame.transparency,
    })
}

/// Whether any frame, composited the way Pillow's `GifImageFile` does
/// (disposal methods, transparent pixels keeping the canvas), has visible
/// transparency. Only the alpha plane is tracked. Used for
/// `ClientFiles.HasTransparency` on animated gifs.
pub(crate) fn any_frame_has_useful_alpha(data: &[u8], num_frames: u64) -> Result<bool> {
    enum Dispose {
        Fill(u8),
        Restore(Vec<u8>),
    }
    let gif = parse(data)?;
    let f0 = &gif.frames[0];
    // frame 0 without transparency (or in L mode) converts to RGB: no alpha at all
    let palette0 = match &f0.local_palette {
        LocalPalette::Absent => gif.global_palette.is_some(),
        LocalPalette::Identity => false,
        LocalPalette::Colours(_) => true,
    };
    let Some(t0) = f0.transparency.filter(|_| palette0) else {
        return Ok(false);
    };
    let (w, h) = open_size(&gif);
    let useful = |alpha: &[u8]| crate::imaging::alpha_is_useful(alpha, w, h);
    let first = first_frame(data)?;
    let mut alpha: Vec<u8> = first
        .indices
        .iter()
        .map(|&i| if i == t0 { 0 } else { 255 })
        .collect();
    if useful(&alpha) {
        return Ok(true);
    }
    let mut method = f0.disposal;
    let mut pending = matches!(method, 2 | 3).then(|| (f0.extent, Dispose::Fill(0)));
    let region = |e: (u32, u32, u32, u32)| {
        (e.1 as usize..e.3 as usize)
            .flat_map(move |y| (e.0 as usize..e.2 as usize).map(move |x| y * w as usize + x))
    };
    for frame in gif.frames.iter().take(num_frames as usize).skip(1) {
        if frame.extent.2 > w || frame.extent.3 > h {
            return Ok(false);
        }
        if let Some((extent, d)) = pending.take() {
            match d {
                Dispose::Fill(a) => region(extent).for_each(|i| alpha[i] = a),
                Dispose::Restore(saved) => {
                    region(extent).zip(saved).for_each(|(i, a)| alpha[i] = a);
                }
            }
        }
        if frame.disposal != 0 {
            method = frame.disposal;
        }
        pending = match method {
            2 => Some((
                frame.extent,
                Dispose::Fill(if frame.transparency.is_some() { 0 } else { 255 }),
            )),
            3 => Some((
                frame.extent,
                Dispose::Restore(region(frame.extent).map(|i| alpha[i]).collect()),
            )),
            _ => None,
        };
        let fill = frame.transparency.unwrap_or(0);
        let mut indices = vec![fill; w as usize * h as usize];
        draw_frame(data, frame, &mut indices, w)?;
        for i in region(frame.extent) {
            if frame.transparency != Some(indices[i]) {
                alpha[i] = 255;
            }
        }
        if useful(&alpha) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grey_ramp_palette_is_not_needed() {
        assert!(!palette_needed(&[0, 0, 0, 1, 1, 1, 2, 2, 2]));
        assert!(palette_needed(&[0, 0, 0, 1, 1, 2]));
        assert!(palette_needed(&[5, 5, 5]));
    }
}
