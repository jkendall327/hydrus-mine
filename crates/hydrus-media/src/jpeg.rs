//! A jpeg's encoding quality, for duplicates auto-resolution ("A has
//! clearly better jpeg quality", "A is a progressive jpeg"): the
//! reference's `populate_jpeg_quality_storage`, which reads the file's
//! header with Pillow.

/// `SUBSAMPLING_*`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subsampling {
    S444,
    S422,
    S420,
    Unknown,
    Greyscale,
}

impl Subsampling {
    /// `subsampling_quality_lookup`: broad relative quality.
    pub fn relative_quality(self) -> f64 {
        match self {
            Subsampling::S444 => 1.00,
            Subsampling::S422 => 0.93,
            Subsampling::S420 => 0.83,
            Subsampling::Unknown => 0.75,
            Subsampling::Greyscale => 0.967,
        }
    }
}

/// What the reference knows about a jpeg's encoding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JpegQuality {
    pub subsampling: Subsampling,
    /// The quantisation estimate (higher is worse), if it could be read.
    pub quality: Option<f64>,
    pub progressive: bool,
}

impl JpegQuality {
    const UNREADABLE: JpegQuality = JpegQuality {
        subsampling: Subsampling::Unknown,
        quality: None,
        progressive: false,
    };
}

/// The header fields Pillow reads before the first scan.
struct Header {
    /// Quantisation tables by id (a later definition replaces an earlier).
    tables: Vec<(u8, Vec<u16>)>,
    /// Components as (h, v) sampling factors, from the frame header.
    components: Vec<(u8, u8)>,
    progressive: bool,
}

fn read_header(data: &[u8]) -> Option<Header> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut header = Header {
        tables: Vec::new(),
        components: Vec::new(),
        progressive: false,
    };
    let mut i = 2;
    let mut frame_seen = false;
    loop {
        // skip fill bytes
        while i < data.len() && data[i] == 0xFF && data.get(i + 1) == Some(&0xFF) {
            i += 1;
        }
        if i + 1 >= data.len() || data[i] != 0xFF {
            break;
        }
        let marker = data[i + 1];
        i += 2;
        if matches!(marker, 0xD8 | 0x01 | 0xD0..=0xD7) {
            continue;
        }
        if marker == 0xD9 || i + 2 > data.len() {
            break;
        }
        let len = usize::from(u16::from_be_bytes([data[i], data[i + 1]]));
        if len < 2 || i + len > data.len() {
            break;
        }
        let body = &data[i + 2..i + len];
        match marker {
            0xDB => {
                let mut p = 0;
                while p < body.len() {
                    let (precision, id) = (body[p] >> 4, body[p] & 15);
                    p += 1;
                    let n = if precision == 0 { 64 } else { 128 };
                    if p + n > body.len() {
                        return None;
                    }
                    let values: Vec<u16> = if precision == 0 {
                        body[p..p + 64].iter().map(|&v| u16::from(v)).collect()
                    } else {
                        body[p..p + 128]
                            .chunks_exact(2)
                            .map(|c| u16::from_be_bytes([c[0], c[1]]))
                            .collect()
                    };
                    p += n;
                    header.tables.retain(|(t, _)| *t != id);
                    header.tables.push((id, values));
                }
            }
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                if frame_seen {
                    // Pillow keeps the first frame's layout
                } else if body.len() >= 6 {
                    frame_seen = true;
                    let count = usize::from(body[5]);
                    for c in 0..count {
                        let Some(&factors) = body.get(6 + c * 3 + 1) else {
                            break;
                        };
                        header.components.push((factors >> 4, factors & 15));
                    }
                }
                if matches!(marker, 0xC2 | 0xC6 | 0xCA | 0xCE) {
                    header.progressive = true;
                }
            }
            0xDA => break,
            _ => {}
        }
        i += len;
    }
    frame_seen.then_some(header)
}

/// The jpeg's quality, as the reference estimates it; an unreadable file
/// gives no quality, unknown subsampling and not progressive.
pub fn jpeg_quality(data: &[u8]) -> JpegQuality {
    let Some(header) = read_header(data) else {
        return JpegQuality::UNREADABLE;
    };
    // `GetJpegSubsamplingRaw`: greyscale for one component; Pillow's
    // `get_sampling` for three
    let subsampling = match header.components.as_slice() {
        [_] => Subsampling::Greyscale,
        [y, cb, cr] => match (y, cb, cr) {
            ((1, 1), (1, 1), (1, 1)) => Subsampling::S444,
            ((2, 1), (1, 1), (1, 1)) => Subsampling::S422,
            ((2, 2), (1, 1), (1, 1)) => Subsampling::S420,
            _ => Subsampling::Unknown,
        },
        _ => Subsampling::Unknown,
    };
    let quality = (!header.tables.is_empty()).then(|| {
        let total: f64 = header
            .tables
            .iter()
            .map(|(_, t)| t.iter().map(|&v| f64::from(v)).sum::<f64>())
            .sum();
        let mean = total / header.tables.len() as f64;
        mean.powf(1.0 / subsampling.relative_quality())
    });
    JpegQuality {
        subsampling,
        quality,
        progressive: header.progressive,
    }
}
