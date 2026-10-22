use crate::{Error, Result};
use std::io::Read;

const HEADER_LEN: usize = 12;
/// Upper bound on the declared uncompressed size, to avoid giant allocations.
const MAX_ULEN: usize = 512 * 1024 * 1024;

/// Decompress a Palworld `.sav` container (`PlM` Oodle or `PlZ` zlib) into raw GVAS bytes.
pub fn decompress(raw: &[u8]) -> Result<Vec<u8>> {
    let Some(header) = raw.get(..HEADER_LEN) else {
        return Err(Error::InvalidHeader(format!(
            "file too short: {} bytes",
            raw.len()
        )));
    };
    let ulen = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    let magic = &header[8..11];
    let save_type = header[11];
    let body = &raw[HEADER_LEN..];

    if ulen > MAX_ULEN {
        return Err(Error::InvalidHeader(format!(
            "uncompressed length {ulen} exceeds limit {MAX_ULEN}"
        )));
    }

    match (magic, save_type) {
        (b"PlM", b'1') => {
            let mut out = vec![0u8; ulen];
            let n = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                oozextract::Extractor::new().read_from_slice(body, &mut out)
            }))
            .map_err(|_| Error::Decompress("oodle decoder panicked on corrupt input".into()))?
            .map_err(|e| Error::Decompress(format!("oodle: {e:?}")))?;
            if n != ulen {
                return Err(Error::Decompress(format!(
                    "oodle produced {n} bytes, expected {ulen}"
                )));
            }
            Ok(out)
        }
        (b"PlZ", b'1') => inflate(body, ulen),
        (b"PlZ", b'2') => inflate(&inflate(body, MAX_ULEN)?, ulen),
        _ => Err(Error::InvalidHeader(format!(
            "unknown magic/type: {:?}/{}",
            String::from_utf8_lossy(magic),
            save_type as char
        ))),
    }
}

/// Inflate zlib data; the result must be exactly `expected` bytes (or at most it, for bounds).
fn inflate(data: &[u8], expected: usize) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(data)
        .take(expected as u64 + 1)
        .read_to_end(&mut out)
        .map_err(|e| Error::Decompress(format!("zlib: {e}")))?;
    if expected != MAX_ULEN && out.len() != expected {
        return Err(Error::Decompress(format!(
            "zlib produced {} bytes, expected {expected}",
            out.len()
        )));
    }
    if out.len() > MAX_ULEN {
        return Err(Error::Decompress("zlib output exceeds limit".into()));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{Compression, write::ZlibEncoder};
    use std::io::Write;

    fn zlib(data: &[u8]) -> Vec<u8> {
        let mut e = ZlibEncoder::new(Vec::new(), Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn build(ulen: u32, body: &[u8], magic: &[u8; 3], ty: u8) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&ulen.to_le_bytes());
        v.extend_from_slice(&(body.len() as u32).to_le_bytes());
        v.extend_from_slice(magic);
        v.push(ty);
        v.extend_from_slice(body);
        v
    }

    #[test]
    fn too_short_is_error() {
        assert!(matches!(decompress(&[0; 5]), Err(Error::InvalidHeader(_))));
    }

    #[test]
    fn unknown_magic_is_error() {
        let mut v = vec![4, 0, 0, 0, 4, 0, 0, 0];
        v.extend_from_slice(b"XYZ1");
        v.extend_from_slice(&[0; 4]);
        assert!(matches!(decompress(&v), Err(Error::InvalidHeader(_))));
    }

    #[test]
    fn absurd_length_is_error() {
        let mut v = vec![0xff, 0xff, 0xff, 0xff, 4, 0, 0, 0];
        v.extend_from_slice(b"PlM1");
        v.extend_from_slice(&[0; 4]);
        assert!(matches!(decompress(&v), Err(Error::InvalidHeader(_))));
    }

    #[test]
    fn plz_single_and_double_zlib() {
        let data = b"GVAS-test";
        let once = zlib(data);
        let raw1 = build(9, &once, b"PlZ", b'1');
        assert_eq!(decompress(&raw1).unwrap(), data);
        let twice = zlib(&once);
        let raw2 = build(9, &twice, b"PlZ", b'2');
        assert_eq!(decompress(&raw2).unwrap(), data);
    }

    #[test]
    fn plz_length_mismatch_is_error() {
        let raw = build(10, &zlib(b"GVAS-test"), b"PlZ", b'1');
        assert!(matches!(decompress(&raw), Err(Error::Decompress(_))));
    }

    #[test]
    fn plz_bad_type_is_error() {
        let raw = build(9, &zlib(b"GVAS-test"), b"PlZ", b'9');
        assert!(matches!(decompress(&raw), Err(Error::InvalidHeader(_))));
    }
}
