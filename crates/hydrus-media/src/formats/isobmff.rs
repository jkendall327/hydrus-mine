//! HEIF/AVIF still images (ISO base media file format): the primary item's
//! size (`ispe`), rotation (`irot`) and colour box, which is what Pillow's
//! AVIF plugin and pillow-heif report as the image size after orientation.

/// What we read from the item properties of the primary image.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ItemInfo {
    pub width: u32,
    pub height: u32,
    /// Quarter turns counter-clockwise.
    pub rotation: u8,
    /// A `colr` box of type `nclx` (pillow-heif exposes it as `info['nclx_profile']`).
    pub has_nclx: bool,
    pub has_icc: bool,
}

fn boxes(data: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 8 <= data.len() {
        let size = u64::from(u32::from_be_bytes([
            data[i],
            data[i + 1],
            data[i + 2],
            data[i + 3],
        ]));
        let kind = [data[i + 4], data[i + 5], data[i + 6], data[i + 7]];
        let (header, size) = match size {
            1 => {
                let Some(b) = data.get(i + 8..i + 16) else {
                    break;
                };
                (16, u64::from_be_bytes(b.try_into().unwrap_or([0; 8])))
            }
            0 => (8, (data.len() - i) as u64),
            s => (8, s),
        };
        let Ok(size) = usize::try_from(size) else {
            break;
        };
        if size < header || i + size > data.len() {
            break;
        }
        out.push((kind, &data[i + header..i + size]));
        i += size;
    }
    out
}

fn find<'a>(list: &[([u8; 4], &'a [u8])], kind: [u8; 4]) -> Option<&'a [u8]> {
    list.iter().find(|(k, _)| *k == kind).map(|(_, d)| *d)
}

fn be(data: &[u8], at: usize, n: usize) -> Option<u32> {
    Some(
        data.get(at..at + n)?
            .iter()
            .fold(0u32, |a, &b| (a << 8) | u32::from(b)),
    )
}

/// Properties of the primary item of a HEIF/AVIF file.
pub(crate) fn primary_item(data: &[u8]) -> Option<ItemInfo> {
    let top = boxes(data);
    let meta = find(&top, *b"meta")?;
    // meta is a full box: skip version/flags
    let meta_boxes = boxes(meta.get(4..)?);
    let pitm = find(&meta_boxes, *b"pitm")?;
    let primary = if pitm.first()? == &0 {
        be(pitm, 4, 2)?
    } else {
        be(pitm, 4, 4)?
    };
    let iprp = boxes(find(&meta_boxes, *b"iprp")?);
    let ipco = boxes(find(&iprp, *b"ipco")?);
    let ipma = find(&iprp, *b"ipma")?;
    let (version, flags) = (ipma[0], be(ipma, 1, 3)?);
    let entry_count = be(ipma, 4, 4)? as usize;
    let mut pos = 8;
    let mut associated: Vec<usize> = Vec::new();
    for _ in 0..entry_count {
        let (item_id, id_len) = if version < 1 {
            (be(ipma, pos, 2)?, 2)
        } else {
            (be(ipma, pos, 4)?, 4)
        };
        pos += id_len;
        let count = usize::from(*ipma.get(pos)?);
        pos += 1;
        for _ in 0..count {
            let index = if flags & 1 != 0 {
                let v = be(ipma, pos, 2)?;
                pos += 2;
                v & 0x7FFF
            } else {
                let v = be(ipma, pos, 1)?;
                pos += 1;
                v & 0x7F
            };
            if item_id == primary && index > 0 {
                associated.push(index as usize - 1);
            }
        }
    }
    let mut info = ItemInfo::default();
    for i in associated {
        let Some((kind, body)) = ipco.get(i) else {
            continue;
        };
        match kind {
            b"ispe" => {
                info.width = be(body, 4, 4)?;
                info.height = be(body, 8, 4)?;
            }
            b"irot" => info.rotation = body.first()? & 3,
            b"colr" => match body.get(..4)? {
                b"nclx" => info.has_nclx = true,
                b"prof" | b"rICC" => info.has_icc = true,
                _ => {}
            },
            _ => {}
        }
    }
    Some(info)
}

/// Display size (rotation applied).
pub(crate) fn display_size(data: &[u8]) -> Option<(u32, u32)> {
    let info = primary_item(data)?;
    if info.width == 0 || info.height == 0 {
        return None;
    }
    Some(if info.rotation % 2 == 1 {
        (info.height, info.width)
    } else {
        (info.width, info.height)
    })
}
