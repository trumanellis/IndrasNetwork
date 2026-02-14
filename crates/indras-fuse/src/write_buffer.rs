/// Write buffer for accumulating writes before flushing to the artifact store.
///
/// On `flush()`/`release()`, the buffer contents are hashed (BLAKE3) and stored
/// as a new content-addressed blob. Multiple small writes produce one blob creation.
#[derive(Debug)]
pub struct WriteBuffer {
    data: Vec<u8>,
    dirty: bool,
}

impl WriteBuffer {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            dirty: false,
        }
    }

    pub fn from_existing(data: Vec<u8>) -> Self {
        Self { data, dirty: false }
    }

    /// Write `data` at the given byte offset, extending the buffer if needed.
    pub fn write_at(&mut self, offset: usize, data: &[u8]) -> usize {
        let end = offset + data.len();
        if end > self.data.len() {
            self.data.resize(end, 0);
        }
        self.data[offset..end].copy_from_slice(data);
        self.dirty = true;
        data.len()
    }

    /// Read up to `size` bytes starting at `offset`.
    pub fn read_at(&self, offset: usize, size: usize) -> &[u8] {
        if offset >= self.data.len() {
            return &[];
        }
        let end = (offset + size).min(self.data.len());
        &self.data[offset..end]
    }

    pub fn truncate(&mut self, size: usize) {
        self.data.truncate(size);
        self.dirty = true;
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn into_data(self) -> Vec<u8> {
        self.data
    }

    pub fn mark_clean(&mut self) {
        self.dirty = false;
    }
}

impl Default for WriteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_at_extends_buffer() {
        let mut buf = WriteBuffer::new();
        assert_eq!(buf.write_at(0, b"hello"), 5);
        assert_eq!(buf.data(), b"hello");
        assert!(buf.is_dirty());
    }

    #[test]
    fn test_write_at_offset() {
        let mut buf = WriteBuffer::new();
        buf.write_at(0, b"hello world");
        buf.write_at(6, b"WORLD");
        assert_eq!(buf.data(), b"hello WORLD");
    }

    #[test]
    fn test_write_at_gap() {
        let mut buf = WriteBuffer::new();
        buf.write_at(5, b"data");
        assert_eq!(buf.len(), 9);
        assert_eq!(&buf.data()[..5], &[0, 0, 0, 0, 0]);
        assert_eq!(&buf.data()[5..], b"data");
    }

    #[test]
    fn test_read_at() {
        let buf = WriteBuffer::from_existing(b"hello world".to_vec());
        assert_eq!(buf.read_at(0, 5), b"hello");
        assert_eq!(buf.read_at(6, 5), b"world");
        assert_eq!(buf.read_at(100, 5), b"");
    }

    #[test]
    fn test_truncate() {
        let mut buf = WriteBuffer::from_existing(b"hello world".to_vec());
        assert!(!buf.is_dirty());
        buf.truncate(5);
        assert_eq!(buf.data(), b"hello");
        assert!(buf.is_dirty());
    }

    #[test]
    fn test_from_existing_not_dirty() {
        let buf = WriteBuffer::from_existing(b"existing".to_vec());
        assert!(!buf.is_dirty());
        assert_eq!(buf.len(), 8);
    }
}
