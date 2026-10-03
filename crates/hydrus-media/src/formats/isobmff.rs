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

/// The `meta` box's children, the primary item's id, and each item's
/// properties (`ipco` boxes associated with it by `ipma`).
struct Meta<'a> {
    boxes: Vec<([u8; 4], &'a [u8])>,
    primary: u32,
    properties: Vec<(u32, [u8; 4], &'a [u8])>,
}

fn meta(data: &[u8]) -> Option<Meta<'_>> {
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
    let mut properties = Vec::new();
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
            if index > 0
                && let Some(&(kind, body)) = ipco.get(index as usize - 1)
            {
                properties.push((item_id, kind, body));
            }
        }
    }
    Some(Meta {
        boxes: meta_boxes,
        primary,
        properties,
    })
}

/// Properties of the primary item of a HEIF/AVIF file.
pub(crate) fn primary_item(data: &[u8]) -> Option<ItemInfo> {
    let meta = meta(data)?;
    let mut info = ItemInfo::default();
    for &(item, kind, body) in &meta.properties {
        if item != meta.primary {
            continue;
        }
        match &kind {
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

/// The primary item's id and that of its alpha plane: an auxiliary item
/// (`auxl` reference) whose `auxC` type is one of the alpha URNs.
pub(crate) fn alpha_item(data: &[u8]) -> Option<(u32, u32)> {
    const ALPHA: [&[u8]; 2] = [
        b"urn:mpeg:mpegB:cicp:systems:auxiliary:alpha",
        b"urn:mpeg:hevc:2015:auxid:1",
    ];
    let meta = meta(data)?;
    let iref = find(&meta.boxes, *b"iref")?;
    let wide = iref.first()? != &0;
    let id_len = if wide { 4 } else { 2 };
    for (kind, body) in boxes(iref.get(4..)?) {
        if &kind != b"auxl" {
            continue;
        }
        let from = be(body, 0, id_len)?;
        let count = be(body, id_len, 2)? as usize;
        let refers_to_primary = (0..count)
            .filter_map(|i| be(body, id_len + 2 + i * id_len, id_len))
            .any(|to| to == meta.primary);
        let is_alpha = meta.properties.iter().any(|&(item, kind, aux)| {
            // auxC is a full box: version/flags, then a null-terminated URN
            item == from
                && &kind == b"auxC"
                && aux
                    .get(4..)
                    .and_then(|urn| urn.split(|&b| b == 0).next())
                    .is_some_and(|urn| ALPHA.contains(&urn))
        });
        if refers_to_primary && is_alpha {
            return Some((meta.primary, from));
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../oracle/fixtures/media/");
        std::fs::read(format!("{dir}{name}")).unwrap()
    }

    #[test]
    fn finds_the_alpha_plane_of_heif_and_avif_images() {
        assert_eq!(alpha_item(&fixture("heic_alpha.heic")), Some((1, 2)));
        assert_eq!(alpha_item(&fixture("avif_alpha.avif")), Some((1, 2)));
        assert_eq!(alpha_item(&fixture("heic_still.heic")), None);
        assert_eq!(alpha_item(&fixture("avif_still.avif")), None);
    }
}
