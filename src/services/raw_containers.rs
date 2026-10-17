//! Embedded-JPEG previews and basic metadata for the two common RAW
//! formats that are *not* TIFF-based, so `raw_preview`'s IFD walker can't
//! read them:
//!
//! - **CR3** (Canon, 2018+): an ISO base-media ("MP4") container. Track 1
//!   holds a full-size JPEG; a smaller `PRVW` JPEG sits in a Canon `uuid`
//!   box as a fallback. Capture date, make, model and orientation live in
//!   the `CMT1` / `CMT2` TIFF blocks of the Canon metadata box.
//! - **RAF** (Fujifilm): a fixed header whose bytes 84..92 give the offset
//!   and length of an embedded JPEG, which also carries normal EXIF.
//!
//! Like the TIFF path, this shows the camera's own rendering rather than
//! demosaicing sensor data.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Anything larger is more likely a corrupt length than a real preview.
const MAX_PREVIEW_BYTES: u64 = 128 * 1024 * 1024;
/// Metadata blocks are a few KB; cap what we read for them.
const MAX_META_BOX_BYTES: u64 = 4 * 1024 * 1024;
/// Bounds for walking boxes, so malformed files can't loop or recurse.
const MAX_BOXES: usize = 512;
const MAX_DEPTH: usize = 8;

const CANON_META_UUID: [u8; 16] = [
    0x85, 0xc0, 0xb6, 0x87, 0x82, 0x0f, 0x11, 0xe0, 0x81, 0x11, 0xf4, 0xce, 0x46, 0x2b, 0x6a, 0x48,
];
const CANON_PREVIEW_UUID: [u8; 16] = [
    0xea, 0xf4, 0x2b, 0x5e, 0x1c, 0x98, 0x4b, 0x88, 0xb9, 0xfb, 0xb7, 0xdc, 0x40, 0x6e, 0x4d, 0x16,
];
const RAF_MAGIC: &[u8] = b"FUJIFILMCCD-RAW";

/// Capture metadata recovered from a CR3's Canon metadata box.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Cr3Metadata {
    /// EXIF `DateTimeOriginal`, as written (`YYYY:MM:DD HH:MM:SS`).
    pub date_time_original: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub orientation: Option<u16>,
}

/// Largest embedded JPEG in a CR3, or `None` if none was found.
pub fn cr3_preview(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let mut file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let len = file.metadata().map_err(|e| format!("stat: {e}"))?.len();
    let layout = scan_cr3(&mut file, len)?;
    for (offset, size) in [layout.track1_jpeg, layout.prvw_jpeg].into_iter().flatten() {
        if let Some(bytes) = read_jpeg(&mut file, len, offset, size)? {
            return Ok(Some(bytes));
        }
    }
    Ok(None)
}

/// Capture date, make, model and orientation from a CR3.
pub fn cr3_metadata(path: &Path) -> Result<Cr3Metadata, String> {
    let mut file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let len = file.metadata().map_err(|e| format!("stat: {e}"))?.len();
    let layout = scan_cr3(&mut file, len)?;
    let mut meta = Cr3Metadata::default();
    if let Some((offset, size)) = layout.cmt1 {
        let tiff = read_bounded(&mut file, len, offset, size, MAX_META_BOX_BYTES)?;
        meta.make = tiff_ascii(&tiff, 0x010F);
        meta.model = tiff_ascii(&tiff, 0x0110);
        meta.orientation = tiff_short(&tiff, 0x0112);
    }
    if let Some((offset, size)) = layout.cmt2 {
        let tiff = read_bounded(&mut file, len, offset, size, MAX_META_BOX_BYTES)?;
        meta.date_time_original = tiff_ascii(&tiff, 0x9003);
    }
    Ok(meta)
}

/// The embedded JPEG of a Fujifilm RAF (it carries the photo's EXIF too).
pub fn raf_preview(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let mut file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let len = file.metadata().map_err(|e| format!("stat: {e}"))?.len();
    let mut header = [0u8; 92];
    file.read_exact(&mut header)
        .map_err(|e| format!("read RAF header: {e}"))?;
    if !header.starts_with(RAF_MAGIC) {
        return Err("not a Fujifilm RAF (bad magic)".into());
    }
    let offset = u64::from(u32::from_be_bytes([
        header[84], header[85], header[86], header[87],
    ]));
    let size = u64::from(u32::from_be_bytes([
        header[88], header[89], header[90], header[91],
    ]));
    read_jpeg(&mut file, len, offset, size)
}

// ----- CR3 box walking ------------------------------------------------

#[derive(Debug, Default)]
struct Cr3Layout {
    /// (offset, size) of the full-size JPEG sample in track 1.
    track1_jpeg: Option<(u64, u64)>,
    /// (offset, size) of the PRVW preview JPEG.
    prvw_jpeg: Option<(u64, u64)>,
    /// (offset, size) of the CMT1 / CMT2 TIFF payloads.
    cmt1: Option<(u64, u64)>,
    cmt2: Option<(u64, u64)>,
}

struct BoxHeader {
    kind: [u8; 4],
    /// Offset of the payload (after size/type/largesize).
    payload: u64,
    /// Offset just past the end of the box.
    end: u64,
}

struct Walker<'a> {
    file: &'a mut File,
    len: u64,
    boxes: usize,
    layout: Cr3Layout,
    tracks_seen: usize,
    /// Sample table of the track currently being walked.
    stsz_first: Option<u64>,
    chunk_first: Option<u64>,
}

fn scan_cr3(file: &mut File, len: u64) -> Result<Cr3Layout, String> {
    let mut ftyp = [0u8; 12];
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    file.read_exact(&mut ftyp)
        .map_err(|e| format!("read CR3 header: {e}"))?;
    if &ftyp[4..8] != b"ftyp" || &ftyp[8..12] != b"crx " {
        return Err("not a Canon CR3 (bad ftyp)".into());
    }
    let mut walker = Walker {
        file,
        len,
        boxes: 0,
        layout: Cr3Layout::default(),
        tracks_seen: 0,
        stsz_first: None,
        chunk_first: None,
    };
    walker.walk(0, len, 0)?;
    Ok(walker.layout)
}

impl Walker<'_> {
    fn header_at(&mut self, at: u64, parent_end: u64) -> Result<Option<BoxHeader>, String> {
        if at.saturating_add(8) > parent_end {
            return Ok(None);
        }
        let mut head = [0u8; 8];
        self.file
            .seek(SeekFrom::Start(at))
            .map_err(|e| e.to_string())?;
        self.file.read_exact(&mut head).map_err(|e| e.to_string())?;
        let size32 = u64::from(u32::from_be_bytes([head[0], head[1], head[2], head[3]]));
        let kind = [head[4], head[5], head[6], head[7]];
        let (payload, size) = match size32 {
            0 => (at + 8, parent_end - at),
            1 => {
                let mut large = [0u8; 8];
                self.file
                    .read_exact(&mut large)
                    .map_err(|e| e.to_string())?;
                (at + 16, u64::from_be_bytes(large))
            }
            n => (at + 8, n),
        };
        let end = at.saturating_add(size);
        if size < 8 || end > parent_end || payload > end {
            return Ok(None); // malformed: stop walking this level
        }
        Ok(Some(BoxHeader { kind, payload, end }))
    }

    fn walk(&mut self, start: u64, end: u64, depth: usize) -> Result<(), String> {
        if depth > MAX_DEPTH {
            return Ok(());
        }
        let mut at = start;
        while let Some(header) = self.header_at(at, end)? {
            self.boxes += 1;
            if self.boxes > MAX_BOXES {
                return Ok(());
            }
            self.visit(&header, depth)?;
            at = header.end;
        }
        Ok(())
    }

    fn visit(&mut self, header: &BoxHeader, depth: usize) -> Result<(), String> {
        match &header.kind {
            b"moov" | b"mdia" | b"minf" | b"stbl" => {
                self.walk(header.payload, header.end, depth + 1)?;
            }
            b"trak" => {
                self.tracks_seen += 1;
                self.stsz_first = None;
                self.chunk_first = None;
                self.walk(header.payload, header.end, depth + 1)?;
                if self.tracks_seen == 1 {
                    if let (Some(offset), Some(size)) = (self.chunk_first, self.stsz_first) {
                        self.layout.track1_jpeg = Some((offset, size));
                    }
                }
            }
            b"stsz" => self.stsz_first = self.read_stsz(header)?,
            b"co64" => self.chunk_first = self.read_chunk_offset(header, true)?,
            b"stco" => self.chunk_first = self.read_chunk_offset(header, false)?,
            b"uuid" => {
                let mut uuid = [0u8; 16];
                self.file
                    .seek(SeekFrom::Start(header.payload))
                    .map_err(|e| e.to_string())?;
                if self.file.read_exact(&mut uuid).is_err() {
                    return Ok(());
                }
                if uuid == CANON_META_UUID {
                    self.walk(header.payload + 16, header.end, depth + 1)?;
                } else if uuid == CANON_PREVIEW_UUID {
                    // 16-byte uuid, then 8 bytes Canon puts before the PRVW box.
                    self.walk(header.payload + 24, header.end, depth + 1)?;
                }
            }
            b"CMT1" => self.layout.cmt1 = Some((header.payload, header.end - header.payload)),
            b"CMT2" => self.layout.cmt2 = Some((header.payload, header.end - header.payload)),
            b"PRVW" => {
                // The JPEG starts at the first SOI marker inside the box.
                let size = (header.end - header.payload).min(64);
                let head = read_bounded(self.file, self.len, header.payload, size, 64)?;
                if let Some(pos) = head.windows(2).position(|w| w == [0xFF, 0xD8]) {
                    let offset = header.payload + pos as u64;
                    self.layout.prvw_jpeg = Some((offset, header.end - offset));
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// First sample size from an `stsz` box.
    fn read_stsz(&mut self, header: &BoxHeader) -> Result<Option<u64>, String> {
        let body = read_bounded(self.file, self.len, header.payload, 16, 16)?;
        if body.len() < 12 {
            return Ok(None);
        }
        let uniform = u64::from(u32::from_be_bytes([body[4], body[5], body[6], body[7]]));
        let count = u32::from_be_bytes([body[8], body[9], body[10], body[11]]);
        if uniform > 0 {
            return Ok(Some(uniform));
        }
        if count == 0 || body.len() < 16 {
            return Ok(None);
        }
        Ok(Some(u64::from(u32::from_be_bytes([
            body[12], body[13], body[14], body[15],
        ]))))
    }

    /// First chunk offset from a `co64` / `stco` box.
    fn read_chunk_offset(&mut self, header: &BoxHeader, wide: bool) -> Result<Option<u64>, String> {
        let body = read_bounded(self.file, self.len, header.payload, 16, 16)?;
        if body.len() < 8 || u32::from_be_bytes([body[4], body[5], body[6], body[7]]) == 0 {
            return Ok(None);
        }
        Ok(if wide {
            body.get(8..16)
                .map(|b| u64::from_be_bytes(b.try_into().unwrap_or_default()))
        } else {
            body.get(8..12)
                .map(|b| u64::from(u32::from_be_bytes(b.try_into().unwrap_or_default())))
        })
    }
}

// ----- shared helpers --------------------------------------------------

/// Read `size` bytes at `offset` if they start with a JPEG SOI marker.
fn read_jpeg(file: &mut File, len: u64, offset: u64, size: u64) -> Result<Option<Vec<u8>>, String> {
    if !(4..=MAX_PREVIEW_BYTES).contains(&size) || offset.saturating_add(size) > len {
        return Ok(None);
    }
    let bytes = read_bounded(file, len, offset, size, MAX_PREVIEW_BYTES)?;
    Ok(bytes.starts_with(&[0xFF, 0xD8]).then_some(bytes))
}

fn read_bounded(
    file: &mut File,
    len: u64,
    offset: u64,
    size: u64,
    cap: u64,
) -> Result<Vec<u8>, String> {
    let size = size.min(cap).min(len.saturating_sub(offset));
    let mut buf = vec![0u8; size as usize];
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut buf)
        .map_err(|e| format!("read: {e}"))?;
    Ok(buf)
}

/// Locate `tag` in IFD0 of a TIFF block: returns (type, count, value bytes).
fn tiff_entry(tiff: &[u8], tag: u16) -> Option<(u16, u32, Vec<u8>)> {
    let little = match tiff.get(0..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    let count = u16_at(ifd)? as usize;
    for i in 0..count.min(512) {
        let entry = ifd + 2 + i * 12;
        if u16_at(entry)? != tag {
            continue;
        }
        let kind = u16_at(entry + 2)?;
        let n = u32_at(entry + 4)?;
        let unit = match kind {
            2 | 7 | 1 => 1,
            3 => 2,
            4 => 4,
            _ => return None,
        };
        let total = (n as usize).checked_mul(unit)?;
        let data = if total <= 4 {
            tiff.get(entry + 8..entry + 8 + total)?.to_vec()
        } else {
            let at = u32_at(entry + 8)? as usize;
            tiff.get(at..at.checked_add(total)?)?.to_vec()
        };
        return Some((kind, n, data_with_order(data, kind, little)));
    }
    None
}

/// Normalise SHORT values to big-endian so callers read them one way.
fn data_with_order(mut data: Vec<u8>, kind: u16, little: bool) -> Vec<u8> {
    if kind == 3 && little {
        for pair in data.as_chunks_mut::<2>().0 {
            pair.swap(0, 1);
        }
    }
    data
}

fn tiff_ascii(tiff: &[u8], tag: u16) -> Option<String> {
    let (kind, _, data) = tiff_entry(tiff, tag)?;
    if kind != 2 {
        return None;
    }
    let text = String::from_utf8_lossy(&data);
    let text = text.trim_end_matches('\0').trim();
    (!text.is_empty()).then(|| text.to_string())
}

fn tiff_short(tiff: &[u8], tag: u16) -> Option<u16> {
    let (kind, _, data) = tiff_entry(tiff, tag)?;
    if kind != 3 {
        return None;
    }
    Some(u16::from_be_bytes(data.get(0..2)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn tiny_jpeg() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(8, 6, image::Rgb([200, 100, 50]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    }

    fn bmff_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    /// Little-endian TIFF with ASCII and SHORT entries in IFD0.
    fn tiny_tiff(ascii: &[(u16, &str)], shorts: &[(u16, u16)]) -> Vec<u8> {
        let count = ascii.len() + shorts.len();
        let ifd_len = 2 + count * 12 + 4;
        let mut data_at = 8 + ifd_len;
        let mut out = b"II*\0".to_vec();
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&(count as u16).to_le_bytes());
        let mut blobs = Vec::new();
        for (tag, text) in ascii {
            let mut value = text.as_bytes().to_vec();
            value.push(0);
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&2u16.to_le_bytes());
            out.extend_from_slice(&(value.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data_at as u32).to_le_bytes());
            data_at += value.len();
            blobs.extend_from_slice(&value);
        }
        for (tag, value) in shorts {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&3u16.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
            out.extend_from_slice(&[0, 0]);
        }
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&blobs);
        out
    }

    fn write_temp(bytes: &[u8], name: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(name);
        std::fs::File::create(&path)
            .unwrap()
            .write_all(bytes)
            .unwrap();
        (dir, path)
    }

    #[test]
    fn reads_raf_embedded_jpeg() {
        let jpeg = tiny_jpeg();
        let mut raf = vec![0u8; 100];
        raf[..RAF_MAGIC.len()].copy_from_slice(RAF_MAGIC);
        raf[84..88].copy_from_slice(&100u32.to_be_bytes());
        raf[88..92].copy_from_slice(&(jpeg.len() as u32).to_be_bytes());
        raf.extend_from_slice(&jpeg);
        let (_dir, path) = write_temp(&raf, "photo.raf");
        assert_eq!(raf_preview(&path).unwrap(), Some(jpeg));
    }

    #[test]
    fn rejects_non_raf() {
        let (_dir, path) = write_temp(&[0u8; 120], "bogus.raf");
        assert!(raf_preview(&path).is_err());
    }

    /// ftyp, moov{Canon uuid{CMT1,CMT2}, trak{mdia{minf{stbl{stsz,co64}}}}}, mdat{jpeg}.
    fn build_cr3(jpeg: &[u8]) -> Vec<u8> {
        let ftyp = bmff_box(b"ftyp", b"crx \0\0\0\x01crx isom");
        let cmt1 = bmff_box(
            b"CMT1",
            &tiny_tiff(
                &[(0x010F, "Canon"), (0x0110, "Canon EOS R6")],
                &[(0x0112, 6)],
            ),
        );
        let cmt2 = bmff_box(b"CMT2", &tiny_tiff(&[(0x9003, "2024:03:15 10:20:30")], &[]));
        let mut canon = CANON_META_UUID.to_vec();
        canon.extend_from_slice(&cmt1);
        canon.extend_from_slice(&cmt2);
        let canon_box = bmff_box(b"uuid", &canon);

        let mut stsz = vec![0u8; 4];
        stsz.extend_from_slice(&(jpeg.len() as u32).to_be_bytes());
        stsz.extend_from_slice(&1u32.to_be_bytes());
        // co64 offset is patched once the layout is known.
        let build = |offset: u64| {
            let mut co64 = vec![0u8; 4];
            co64.extend_from_slice(&1u32.to_be_bytes());
            co64.extend_from_slice(&offset.to_be_bytes());
            let stbl = bmff_box(
                b"stbl",
                &[bmff_box(b"stsz", &stsz), bmff_box(b"co64", &co64)].concat(),
            );
            let trak = bmff_box(b"trak", &bmff_box(b"mdia", &bmff_box(b"minf", &stbl)));
            bmff_box(b"moov", &[canon_box.clone(), trak].concat())
        };
        let moov_len = build(0).len();
        let jpeg_offset = (ftyp.len() + moov_len + 8) as u64;
        [ftyp.clone(), build(jpeg_offset), bmff_box(b"mdat", jpeg)].concat()
    }

    #[test]
    fn reads_cr3_track1_jpeg_and_metadata() {
        let jpeg = tiny_jpeg();
        let (_dir, path) = write_temp(&build_cr3(&jpeg), "photo.cr3");
        assert_eq!(cr3_preview(&path).unwrap(), Some(jpeg));
        let meta = cr3_metadata(&path).unwrap();
        assert_eq!(
            meta.date_time_original.as_deref(),
            Some("2024:03:15 10:20:30")
        );
        assert_eq!(meta.make.as_deref(), Some("Canon"));
        assert_eq!(meta.model.as_deref(), Some("Canon EOS R6"));
        assert_eq!(meta.orientation, Some(6));
    }

    #[test]
    fn rejects_non_cr3_and_survives_truncation() {
        let (_dir, path) = write_temp(&bmff_box(b"ftyp", b"isom\0\0\0\x01"), "clip.cr3");
        assert!(cr3_preview(&path).is_err());
        let full = build_cr3(&tiny_jpeg());
        let (_dir2, cut) = write_temp(&full[..full.len() / 2], "cut.cr3");
        assert_eq!(cr3_preview(&cut).unwrap(), None);
    }
}
