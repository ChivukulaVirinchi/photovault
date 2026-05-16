//! Extract the largest embedded JPEG preview from a TIFF-based RAW.
//!
//! TIFF-based RAW formats (NEF, CR2, CR3, ARW, DNG, ORF, RW2, PEF,
//! RWL, SRW, …) all pack one or more full-decoded JPEGs at known
//! offsets inside the file. Cameras embed these for the rear-screen
//! preview — they're exactly what every consumer photo-viewer shows
//! when it claims to "open a RAW". The image is the manufacturer's
//! intended rendering: correct white balance, correct colours,
//! correct sharpening. Better than anything an open-source debayer
//! pipeline produces without the camera's proprietary DCP profiles.
//!
//! ## Why a hand-rolled walker
//!
//! `rawloader` and `rawler` give you Bayer sensor data, not embedded
//! JPEGs. `libraw` would work but is a native C dep we don't want to
//! ship per-platform. `kamadak-exif` (already a dep) walks IFD0,
//! IFD1, plus the EXIF / GPS / Interop SubIFDs — but the preview
//! JPEGs in NEF / CR2 / ARW / DNG / ORF live in a TIFF "SubIFD"
//! chain referenced by tag 0x014A, which kamadak doesn't traverse.
//!
//! So we walk the TIFF directly. ~150 lines of well-defined byte
//! pushing. No deps. Covers every TIFF-based RAW in one shot.
//!
//! ## Algorithm
//!
//! 1. Read the 8-byte TIFF header. Confirm `II*\0` (little-endian)
//!    or `MM\0*` (big-endian). Note the offset of IFD0.
//! 2. Recursively walk every IFD reachable from IFD0:
//!    - IFD chain via the NextIFD offset (last 4 bytes of each IFD)
//!    - SubIFD chain via tag 0x014A
//!    - EXIF / GPS / Interop SubIFDs via tags 0x8769 / 0x8825 / 0xA005
//!      Each visit short-circuits if we've already seen that offset,
//!      so a malformed file with a cycle can't hang us.
//! 3. For each IFD, look for two embedded-JPEG idioms:
//!    a. `JPEGInterchangeFormat` (0x0201) + length (0x0202)
//!    b. `StripOffsets` (0x0111) + byte counts (0x0117) where
//!    `Compression` (0x0103) is 6 (old-style JPEG) or 7 (new).
//! 4. Sanity-bound each candidate against the file size, then return
//!    the bytes of the largest one (or None if no candidate found).
//!
//! ## What this doesn't do
//!
//! - RAF (Fujifilm) — custom container, not TIFF. Falls through to
//!   the `image::open` path which fails; the user gets a clear error.
//!   ~100 LOC of standalone parser would add it; do that when a Fuji
//!   user actually asks.
//! - Full Bayer demosaic — out of scope; that's a separate path that
//!   would need DCP colour profiles and per-camera tuning.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;

/// Bytes of the largest embedded JPEG in this TIFF-based RAW, or
/// `None` if the file doesn't contain a recognisable embedded preview.
///
/// Returns `Err` only for I/O failures and malformed TIFF headers —
/// "no preview found" is the `Ok(None)` case and is normal for some
/// older / minimal RAWs. Callers should surface that as "couldn't
/// decode this photo" rather than a hard error.
pub fn extract_largest_preview(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let file_len = file.metadata().map_err(|e| format!("stat: {e}"))?.len();
    let mut reader = TiffReader::new(BufReader::new(file), file_len)?;
    let candidates = reader.find_candidates()?;
    let Some(best) = candidates.into_iter().max_by_key(|c| c.length) else {
        return Ok(None);
    };
    let bytes = reader
        .read_at(best.offset, best.length)
        .map_err(|e| format!("read preview bytes: {e}"))?;
    Ok(Some(bytes))
}

// ----- internals -----------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct PreviewCandidate {
    offset: u64,
    length: u64,
}

struct TiffReader {
    file: BufReader<File>,
    file_len: u64,
    little_endian: bool,
}

/// Tags we care about. TIFF 6.0 + EXIF SubIFD chain entries.
const TAG_COMPRESSION: u16 = 0x0103;
const TAG_STRIP_OFFSETS: u16 = 0x0111;
const TAG_STRIP_BYTE_COUNTS: u16 = 0x0117;
const TAG_JPEG_INTERCHANGE_FORMAT: u16 = 0x0201;
const TAG_JPEG_INTERCHANGE_FORMAT_LENGTH: u16 = 0x0202;
const TAG_SUB_IFDS: u16 = 0x014A;
const TAG_EXIF_IFD: u16 = 0x8769;
const TAG_GPS_IFD: u16 = 0x8825;
const TAG_INTEROP_IFD: u16 = 0xA005;

/// Compression codes that indicate the strip data IS a JPEG bitstream.
///   6  = old-style JPEG (TIFF 6.0)
///   7  = new-style JPEG (Technical Note 2)
fn is_jpeg_compression(code: u32) -> bool {
    code == 6 || code == 7
}

/// Hard cap on IFDs walked per file. Protects against pathological
/// inputs (cycles already prevented by the visited set, but a chain
/// of valid distinct IFDs could still spiral).
const MAX_IFDS: usize = 64;

impl TiffReader {
    fn new(mut file: BufReader<File>, file_len: u64) -> Result<Self, String> {
        let mut header = [0u8; 8];
        file.seek(SeekFrom::Start(0))
            .map_err(|e| format!("seek header: {e}"))?;
        file.read_exact(&mut header)
            .map_err(|e| format!("read header: {e}"))?;
        let little_endian = match &header[0..2] {
            b"II" => true,
            b"MM" => false,
            _ => return Err("not a TIFF (bad byte-order mark)".into()),
        };
        // Magic 42 (0x002A). Some makers use 0x0055 (Olympus ORF) but
        // their IFD layout is the same — accept anything non-zero so
        // we don't lock out valid containers we haven't seen yet.
        let magic = read_u16(&header[2..4], little_endian);
        if magic == 0 {
            return Err("not a TIFF (zero magic)".into());
        }
        Ok(Self {
            file,
            file_len,
            little_endian,
        })
    }

    fn find_candidates(&mut self) -> Result<Vec<PreviewCandidate>, String> {
        // IFD0 offset is bytes 4..8 of the header.
        let mut header = [0u8; 8];
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| format!("seek: {e}"))?;
        self.file
            .read_exact(&mut header)
            .map_err(|e| format!("read: {e}"))?;
        let ifd0 = read_u32(&header[4..8], self.little_endian) as u64;

        let mut out: Vec<PreviewCandidate> = Vec::new();
        let mut visited: HashSet<u64> = HashSet::new();
        let mut count = 0usize;
        self.walk(ifd0, &mut visited, &mut out, &mut count)?;
        Ok(out)
    }

    fn walk(
        &mut self,
        ifd_offset: u64,
        visited: &mut HashSet<u64>,
        out: &mut Vec<PreviewCandidate>,
        count: &mut usize,
    ) -> Result<(), String> {
        if ifd_offset == 0 || ifd_offset >= self.file_len {
            return Ok(());
        }
        if !visited.insert(ifd_offset) {
            return Ok(());
        }
        *count += 1;
        if *count > MAX_IFDS {
            return Ok(());
        }

        // IFD layout: u16 entry-count, N × 12-byte entries, u32 next-IFD.
        let entry_count = self.read_u16(ifd_offset)? as u64;
        // Defensive: don't trust insane entry counts.
        if entry_count > 4096 {
            return Ok(());
        }
        let entries_start = ifd_offset + 2;
        let next_ifd_pos = entries_start + entry_count * 12;
        if next_ifd_pos + 4 > self.file_len {
            return Ok(());
        }

        // First pass: read every entry into memory. We need random
        // access to multiple tags within one IFD, and re-seeking per
        // tag is slow + bug-prone.
        struct Entry {
            tag: u16,
            type_id: u16,
            count: u32,
            value: u32,
        }
        let mut entries: Vec<Entry> = Vec::with_capacity(entry_count as usize);
        let mut buf = [0u8; 12];
        for i in 0..entry_count {
            self.file
                .seek(SeekFrom::Start(entries_start + i * 12))
                .map_err(|e| format!("seek entry: {e}"))?;
            self.file
                .read_exact(&mut buf)
                .map_err(|e| format!("read entry: {e}"))?;
            entries.push(Entry {
                tag: read_u16(&buf[0..2], self.little_endian),
                type_id: read_u16(&buf[2..4], self.little_endian),
                count: read_u32(&buf[4..8], self.little_endian),
                value: read_u32(&buf[8..12], self.little_endian),
            });
        }

        // Pull the values we'll need for this IFD's preview-candidate
        // decision. All these are LONG or SHORT — we can read either
        // inline (count = 1, fits in the 4-byte slot) or from the
        // pointed-to offset.
        let mut compression: Option<u32> = None;
        let mut strip_offset: Option<u64> = None;
        let mut strip_byte_count: Option<u64> = None;
        let mut jpeg_offset: Option<u64> = None;
        let mut jpeg_length: Option<u64> = None;
        let mut sub_ifd_offsets: Vec<u64> = Vec::new();
        let mut exif_sub_ifd: Option<u64> = None;

        for e in &entries {
            match e.tag {
                TAG_COMPRESSION => {
                    compression = self.read_long_value(e.type_id, e.count, e.value).ok();
                }
                // Multiple strips are normal for tiled TIFFs, but for
                // an embedded JPEG it's always a single strip — count =
                // 1. The `if e.count == 1` guard on each arm skips
                // multi-strip variants; they're not JPEG bitstreams.
                TAG_STRIP_OFFSETS if e.count == 1 => {
                    strip_offset = self
                        .read_long_value(e.type_id, e.count, e.value)
                        .ok()
                        .map(|v| v as u64);
                }
                TAG_STRIP_BYTE_COUNTS if e.count == 1 => {
                    strip_byte_count = self
                        .read_long_value(e.type_id, e.count, e.value)
                        .ok()
                        .map(|v| v as u64);
                }
                TAG_JPEG_INTERCHANGE_FORMAT => {
                    jpeg_offset = self
                        .read_long_value(e.type_id, e.count, e.value)
                        .ok()
                        .map(|v| v as u64);
                }
                TAG_JPEG_INTERCHANGE_FORMAT_LENGTH => {
                    jpeg_length = self
                        .read_long_value(e.type_id, e.count, e.value)
                        .ok()
                        .map(|v| v as u64);
                }
                TAG_SUB_IFDS => {
                    // Value is an array of u32 offsets. If count == 1
                    // it sits inline; otherwise the value is an offset
                    // to count×4 bytes of u32s.
                    sub_ifd_offsets = self.read_long_array(e.count, e.value)?;
                }
                TAG_EXIF_IFD => {
                    exif_sub_ifd = Some(e.value as u64);
                }
                TAG_GPS_IFD | TAG_INTEROP_IFD => {
                    // We walk these for completeness — they don't
                    // normally hold preview JPEGs but it's cheap to
                    // visit and protects against future formats that
                    // tuck data there.
                    self.walk(e.value as u64, visited, out, count)?;
                }
                _ => {}
            }
        }

        // Idiom (a): explicit JPEGInterchangeFormat tag pair.
        if let (Some(off), Some(len)) = (jpeg_offset, jpeg_length) {
            if let Some(c) = self.bounded(off, len) {
                out.push(c);
            }
        }
        // Idiom (b): single-strip + JPEG compression.
        if let (Some(off), Some(len), Some(comp)) = (strip_offset, strip_byte_count, compression) {
            if is_jpeg_compression(comp) {
                if let Some(c) = self.bounded(off, len) {
                    out.push(c);
                }
            }
        }

        // Recurse into SubIFDs (where Nikon/Sony/DNG put their
        // full-res previews) and the EXIF SubIFD (sometimes carries
        // a smaller PreviewIFD on phones).
        for sub in sub_ifd_offsets {
            self.walk(sub, visited, out, count)?;
        }
        if let Some(off) = exif_sub_ifd {
            self.walk(off, visited, out, count)?;
        }

        // Walk the next-IFD chain too — IFD1 (the standard thumbnail
        // IFD) is reached this way, and on some bodies it carries a
        // JPEG that's bigger than the SubIFD preview.
        let next_ifd = self.read_u32(next_ifd_pos)? as u64;
        self.walk(next_ifd, visited, out, count)?;

        Ok(())
    }

    /// Confirm `offset + length` is inside the file. Returns `None`
    /// for any out-of-range or zero-length candidate.
    fn bounded(&self, offset: u64, length: u64) -> Option<PreviewCandidate> {
        if length == 0 || offset == 0 {
            return None;
        }
        let end = offset.checked_add(length)?;
        if end > self.file_len {
            return None;
        }
        Some(PreviewCandidate { offset, length })
    }

    fn read_u16(&mut self, offset: u64) -> Result<u16, String> {
        let mut buf = [0u8; 2];
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("seek u16: {e}"))?;
        self.file
            .read_exact(&mut buf)
            .map_err(|e| format!("read u16: {e}"))?;
        Ok(read_u16(&buf, self.little_endian))
    }

    fn read_u32(&mut self, offset: u64) -> Result<u32, String> {
        let mut buf = [0u8; 4];
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| format!("seek u32: {e}"))?;
        self.file
            .read_exact(&mut buf)
            .map_err(|e| format!("read u32: {e}"))?;
        Ok(read_u32(&buf, self.little_endian))
    }

    /// Read a tag value that's a single LONG or SHORT. Handles the
    /// inline-vs-pointed-to distinction: if the on-disk byte size of
    /// the value fits in the 4-byte slot (≤4 bytes), it's stored
    /// inline; otherwise the slot holds a u32 offset.
    fn read_long_value(&mut self, type_id: u16, count: u32, value: u32) -> Result<u32, String> {
        // Types: 3 = SHORT (u16), 4 = LONG (u32). Count must be 1.
        if count != 1 {
            return Err(format!("expected count=1, got {count}"));
        }
        match type_id {
            3 => Ok(value & 0xFFFF), // inline SHORT
            4 => Ok(value),          // inline LONG
            _ => Err(format!("unsupported type {type_id}")),
        }
    }

    /// Read a tag value that's an array of LONGs (SubIFDs[]).
    fn read_long_array(&mut self, count: u32, value: u32) -> Result<Vec<u64>, String> {
        if count == 0 {
            return Ok(Vec::new());
        }
        if count == 1 {
            return Ok(vec![value as u64]);
        }
        // count >= 2 → value is an offset to count×4 bytes
        let bytes_needed = (count as u64) * 4;
        if (value as u64) + bytes_needed > self.file_len {
            return Ok(Vec::new()); // truncated / corrupt; skip
        }
        let mut buf = vec![0u8; bytes_needed as usize];
        self.file
            .seek(SeekFrom::Start(value as u64))
            .map_err(|e| format!("seek long-array: {e}"))?;
        self.file
            .read_exact(&mut buf)
            .map_err(|e| format!("read long-array: {e}"))?;
        let mut out = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let off = read_u32(&buf[i * 4..i * 4 + 4], self.little_endian);
            out.push(off as u64);
        }
        Ok(out)
    }

    fn read_at(&mut self, offset: u64, length: u64) -> std::io::Result<Vec<u8>> {
        self.file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; length as usize];
        self.file.read_exact(&mut buf)?;
        Ok(buf)
    }
}

fn read_u16(b: &[u8], little_endian: bool) -> u16 {
    if little_endian {
        u16::from_le_bytes([b[0], b[1]])
    } else {
        u16::from_be_bytes([b[0], b[1]])
    }
}

fn read_u32(b: &[u8], little_endian: bool) -> u32 {
    if little_endian {
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    } else {
        u32::from_be_bytes([b[0], b[1], b[2], b[3]])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Build a minimal little-endian TIFF in memory with N embedded
    /// "JPEGs" (just byte runs — they don't need to be valid JPEGs,
    /// since extract_largest_preview only reads the strip bytes; the
    /// extractor's job is to find them, not to validate them).
    /// Each embedded blob lives at a known offset after the IFD0,
    /// referenced via JPEGInterchangeFormat (0x0201) + length (0x0202).
    fn make_tiff(blob_lengths: &[u32]) -> Vec<u8> {
        // We'll build IFD0 with one entry per (offset, length) pair,
        // followed by IFD1 chained via NextIFD, each carrying ONE
        // embedded blob. That way each IFD has exactly the
        // JPEGInterchangeFormat + Length pair the walker recognises.
        //
        // Layout:
        //   [0..8]      header (II*\0, ifd0_offset = 8)
        //   [8..]       IFD0 (count, entries, next_ifd_offset)
        //   ...         IFD1, IFD2, ... if multiple blobs
        //   [end..]     blob bytes
        //
        // We compute offsets up front, then emit.
        let n = blob_lengths.len() as u64;
        let mut bytes: Vec<u8> = Vec::new();

        // Reserve header.
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8

        // Each IFD is: 2 bytes (count) + 2×12 bytes (entries) + 4 bytes (next).
        // = 30 bytes per IFD.
        let ifd_size: u64 = 30;
        let ifds_total: u64 = ifd_size * n;
        let blobs_start: u64 = 8 + ifds_total;

        // Compute per-blob offsets.
        let mut blob_offsets: Vec<u64> = Vec::with_capacity(n as usize);
        let mut cursor = blobs_start;
        for &len in blob_lengths {
            blob_offsets.push(cursor);
            cursor += len as u64;
        }

        // Emit IFDs.
        for (i, (&len, &off)) in blob_lengths.iter().zip(&blob_offsets).enumerate() {
            // entry count = 2 (JPEGInterchangeFormat + Length).
            bytes.extend_from_slice(&2u16.to_le_bytes());
            // Entry: tag 0x0201, type 4 (LONG), count 1, value = offset.
            bytes.extend_from_slice(&TAG_JPEG_INTERCHANGE_FORMAT.to_le_bytes());
            bytes.extend_from_slice(&4u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&(off as u32).to_le_bytes());
            // Entry: tag 0x0202, type 4 (LONG), count 1, value = length.
            bytes.extend_from_slice(&TAG_JPEG_INTERCHANGE_FORMAT_LENGTH.to_le_bytes());
            bytes.extend_from_slice(&4u16.to_le_bytes());
            bytes.extend_from_slice(&1u32.to_le_bytes());
            bytes.extend_from_slice(&len.to_le_bytes());
            // NextIFD pointer. Last IFD has 0.
            let next: u32 = if i + 1 < blob_lengths.len() {
                (8 + ifd_size * (i as u64 + 1)) as u32
            } else {
                0
            };
            bytes.extend_from_slice(&next.to_le_bytes());
        }

        // Emit the blob bytes themselves.
        for (i, &len) in blob_lengths.iter().enumerate() {
            // Fill with distinct bytes per blob so tests can verify
            // we pulled the right one.
            let filler = (i as u8).wrapping_add(0xA0);
            for _ in 0..len {
                bytes.push(filler);
            }
        }
        bytes
    }

    fn write_tiff(bytes: &[u8]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.as_file_mut().write_all(bytes).unwrap();
        f.as_file_mut().sync_all().unwrap();
        f
    }

    #[test]
    fn single_preview_is_returned() {
        let blob_len: u32 = 64;
        let tiff = make_tiff(&[blob_len]);
        let f = write_tiff(&tiff);
        let bytes = extract_largest_preview(f.path()).unwrap().unwrap();
        assert_eq!(bytes.len(), blob_len as usize);
        // Every byte should be 0xA0 (filler for blob index 0).
        assert!(bytes.iter().all(|&b| b == 0xA0));
    }

    #[test]
    fn largest_of_multiple_wins() {
        // Three embedded JPEGs: 64 bytes (idx 0), 256 (idx 1), 32 (idx 2).
        let tiff = make_tiff(&[64, 256, 32]);
        let f = write_tiff(&tiff);
        let bytes = extract_largest_preview(f.path()).unwrap().unwrap();
        assert_eq!(bytes.len(), 256);
        // Filler for blob index 1 is 0xA1.
        assert!(bytes.iter().all(|&b| b == 0xA1));
    }

    #[test]
    fn no_preview_returns_none() {
        // A TIFF whose IFD0 has no JPEG-pointer tags at all.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        // IFD0: 0 entries, next = 0.
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let f = write_tiff(&bytes);
        assert!(extract_largest_preview(f.path()).unwrap().is_none());
    }

    #[test]
    fn bad_header_errors() {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.as_file_mut().write_all(b"NOTATIFF").unwrap();
        f.as_file_mut().sync_all().unwrap();
        assert!(extract_largest_preview(f.path()).is_err());
    }

    #[test]
    fn out_of_bounds_offset_is_skipped() {
        // Build a TIFF that claims an embedded JPEG ten megabytes
        // past EOF. The walker should silently drop the candidate.
        let mut bytes: Vec<u8> = Vec::new();
        bytes.extend_from_slice(b"II");
        bytes.extend_from_slice(&42u16.to_le_bytes());
        bytes.extend_from_slice(&8u32.to_le_bytes());
        // IFD0: 2 entries, pointing 10 MB past the file end.
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&TAG_JPEG_INTERCHANGE_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&10_000_000u32.to_le_bytes());
        bytes.extend_from_slice(&TAG_JPEG_INTERCHANGE_FORMAT_LENGTH.to_le_bytes());
        bytes.extend_from_slice(&4u16.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&1024u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes()); // next IFD
        let f = write_tiff(&bytes);
        assert!(extract_largest_preview(f.path()).unwrap().is_none());
    }
}
