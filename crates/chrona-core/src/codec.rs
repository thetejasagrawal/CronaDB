//! Low-level byte encoding helpers.
//!
//! All multi-byte integers are big-endian so that lexicographic byte order
//! matches numeric order in composite keys. See FORMAT.md §5.

use crate::error::{Error, Result};

/// A tiny cursor-based byte reader that turns format errors into
/// [`Error::Format`] rather than panicking.
pub struct ByteReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    fn read(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(Error::Format(format!(
                "unexpected end of record at offset {} (wanted {} more bytes)",
                self.pos, n
            )));
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read(1)?[0])
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        let b = self.read(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_u64(&mut self) -> Result<u64> {
        let b = self.read(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_be_bytes(a))
    }

    pub fn read_i64_sortable(&mut self) -> Result<i64> {
        let raw = self.read_u64()?;
        Ok((raw ^ 0x8000_0000_0000_0000) as i64)
    }

    pub fn read_f32(&mut self) -> Result<f32> {
        let b = self.read(4)?;
        Ok(f32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.read(n)
    }

    pub fn read_len_prefixed(&mut self) -> Result<&'a [u8]> {
        let n = self.read_u32()? as usize;
        self.read(n)
    }
}

/// Lightweight `Vec<u8>` builder.
#[derive(Default, Debug, Clone)]
pub struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn with_capacity(n: usize) -> Self {
        Self {
            buf: Vec::with_capacity(n),
        }
    }

    pub fn write_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn write_u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn write_u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn write_i64_sortable(&mut self, v: i64) {
        let flipped = (v as u64) ^ 0x8000_0000_0000_0000;
        self.buf.extend_from_slice(&flipped.to_be_bytes());
    }

    pub fn write_f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_be_bytes());
    }

    pub fn write_bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    pub fn write_len_prefixed(&mut self, b: &[u8]) {
        self.write_u32(b.len() as u32);
        self.buf.extend_from_slice(b);
    }

    pub fn finish(self) -> Vec<u8> {
        self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_u64() {
        let mut w = ByteWriter::new();
        w.write_u64(0xDEADBEEF_CAFEBABE);
        let bytes = w.finish();
        let mut r = ByteReader::new(&bytes);
        assert_eq!(r.read_u64().unwrap(), 0xDEADBEEF_CAFEBABE);
        assert!(r.is_empty());
    }

    #[test]
    fn roundtrip_sortable_i64() {
        for v in [i64::MIN, -1_000_000, -1, 0, 1, 1_000_000, i64::MAX] {
            let mut w = ByteWriter::new();
            w.write_i64_sortable(v);
            let bytes = w.finish();
            let mut r = ByteReader::new(&bytes);
            assert_eq!(r.read_i64_sortable().unwrap(), v);
        }
    }

    #[test]
    fn len_prefixed() {
        let mut w = ByteWriter::new();
        w.write_len_prefixed(b"hello");
        w.write_len_prefixed(b"world!");
        let bytes = w.finish();
        let mut r = ByteReader::new(&bytes);
        assert_eq!(r.read_len_prefixed().unwrap(), b"hello");
        assert_eq!(r.read_len_prefixed().unwrap(), b"world!");
    }

    #[test]
    fn short_read_errors_cleanly() {
        let mut r = ByteReader::new(&[1, 2, 3]);
        assert!(r.read_u64().is_err());
    }
}
