use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use flate2::Compression;
use flate2::write::ZlibEncoder;
use std::io::Write;

use crate::pdf::kittyv2::kgfx::CHUNK_LIMIT;

pub struct PixelEncoder;

impl PixelEncoder {
    pub fn compress_and_encode(pixels: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(pixels)?;
        let compressed = encoder.finish()?;
        Ok(STANDARD.encode(compressed).into_bytes())
    }

    pub fn chunk_iterator(data: &[u8]) -> ChunkIter<'_> {
        ChunkIter {
            data,
            offset: 0,
            chunk_size: CHUNK_LIMIT - (CHUNK_LIMIT % 4),
        }
    }
}

pub struct ChunkIter<'a> {
    data: &'a [u8],
    offset: usize,
    chunk_size: usize,
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.data.len() {
            return None;
        }
        let end = (self.offset + self.chunk_size).min(self.data.len());
        let chunk = &self.data[self.offset..end];
        self.offset = end;
        Some(chunk)
    }
}

#[cfg(test)]
mod tests {
    use super::PixelEncoder;

    #[test]
    fn chunking_respects_limit() {
        let data = vec![b'a'; 300_000];
        let chunks: Vec<&[u8]> = PixelEncoder::chunk_iterator(&data).collect();
        assert!(chunks.len() > 1);
        for chunk in chunks {
            assert!(chunk.len() <= 131072);
            assert_eq!(chunk.len() % 4, 0);
        }
    }
}
