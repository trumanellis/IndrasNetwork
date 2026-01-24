//! # Compact Wire Format
//!
//! Bandwidth-efficient serialization for IoT communications.
//! Reduces message size for constrained networks (LoRa, BLE, etc.)
//!
//! ## Wire Format
//!
//! ```text
//! [type:1][flags:1][seq:varint][len:varint][payload:len][crc8:1]
//! ```
//!
//! - **type**: Message type (1 byte)
//! - **flags**: Bit flags (1 byte)
//!   - bit 0: ack_requested
//!   - bit 1: fragmented
//!   - bit 2: last_fragment
//! - **seq**: Sequence number (varint, 1-5 bytes)
//! - **len**: Payload length in bytes (varint, 1-5 bytes)
//! - **payload**: Message data
//! - **crc8**: CRC-8-CCITT checksum (polynomial 0x07)
//!
//! ## Security Note
//!
//! CRC8 provides error detection only, not cryptographic integrity.
//! For security-sensitive applications, add an HMAC or use authenticated encryption.

use std::io::{self, Read, Write};
use thiserror::Error;

/// Maximum payload size to prevent memory exhaustion attacks
pub const MAX_PAYLOAD_SIZE: usize = 65536;

/// Minimum valid message size: type(1) + flags(1) + seq_varint(1) + len_varint(1) + crc(1)
const MIN_MESSAGE_SIZE: usize = 5;

/// Compact format errors
#[derive(Debug, Error)]
pub enum CompactError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Invalid varint encoding")]
    InvalidVarint,
    #[error("Message too large: {size} bytes exceeds {limit}")]
    MessageTooLarge { size: usize, limit: usize },
    #[error("Invalid message type: {0}")]
    InvalidMessageType(u8),
    #[error("Checksum mismatch")]
    ChecksumMismatch,
    #[error("Sequence number overflow")]
    SequenceOverflow,
}

/// Compact message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CompactMessageType {
    /// Ping/keepalive
    Ping = 0,
    /// Pong response
    Pong = 1,
    /// Data payload
    Data = 2,
    /// Acknowledgment
    Ack = 3,
    /// Sync request (heads only)
    SyncRequest = 4,
    /// Sync response
    SyncResponse = 5,
    /// Presence announcement
    Presence = 6,
}

impl TryFrom<u8> for CompactMessageType {
    type Error = CompactError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Ping),
            1 => Ok(Self::Pong),
            2 => Ok(Self::Data),
            3 => Ok(Self::Ack),
            4 => Ok(Self::SyncRequest),
            5 => Ok(Self::SyncResponse),
            6 => Ok(Self::Presence),
            _ => Err(CompactError::InvalidMessageType(value)),
        }
    }
}

/// Compact message header
///
/// Format: [type:1][flags:1][seq:varint][len:varint][payload][crc8:1]
#[derive(Debug, Clone)]
pub struct CompactMessage {
    /// Message type
    pub msg_type: CompactMessageType,
    /// Flags (bit 0: ack_requested, bit 1: fragmented, bit 2: last_fragment)
    pub flags: u8,
    /// Sequence number (for ordering and ack)
    pub sequence: u32,
    /// Payload data
    pub payload: Vec<u8>,
}

impl CompactMessage {
    /// Create a new compact message
    pub fn new(msg_type: CompactMessageType, payload: Vec<u8>) -> Self {
        Self {
            msg_type,
            flags: 0,
            sequence: 0,
            payload,
        }
    }

    /// Create a ping message
    pub fn ping() -> Self {
        Self::new(CompactMessageType::Ping, vec![])
    }

    /// Create a pong message
    pub fn pong() -> Self {
        Self::new(CompactMessageType::Pong, vec![])
    }

    /// Create a data message
    pub fn data(payload: Vec<u8>) -> Self {
        Self::new(CompactMessageType::Data, payload)
    }

    /// Create an ack message
    pub fn ack(sequence: u32) -> Self {
        let mut msg = Self::new(CompactMessageType::Ack, vec![]);
        msg.sequence = sequence;
        msg
    }

    /// Set ack requested flag
    pub fn with_ack_requested(mut self) -> Self {
        self.flags |= 0x01;
        self
    }

    /// Check if ack is requested
    pub fn ack_requested(&self) -> bool {
        self.flags & 0x01 != 0
    }

    /// Set fragmented flag
    pub fn with_fragmented(mut self) -> Self {
        self.flags |= 0x02;
        self
    }

    /// Check if fragmented
    pub fn is_fragmented(&self) -> bool {
        self.flags & 0x02 != 0
    }

    /// Check if this is the last fragment
    pub fn is_last_fragment(&self) -> bool {
        self.flags & 0x04 != 0
    }

    /// Get fragment index (if fragmented)
    pub fn fragment_index(&self) -> u16 {
        (self.sequence >> 16) as u16
    }

    /// Get original sequence (if fragmented, lower 16 bits)
    pub fn original_sequence(&self) -> u32 {
        if self.is_fragmented() {
            self.sequence & 0xFFFF
        } else {
            self.sequence
        }
    }

    /// Set sequence number
    pub fn with_sequence(mut self, seq: u32) -> Self {
        self.sequence = seq;
        self
    }

    /// Encode to bytes
    pub fn encode(&self) -> Result<Vec<u8>, CompactError> {
        let mut buf = Vec::with_capacity(4 + self.payload.len());

        // Type and flags
        buf.push(self.msg_type as u8);
        buf.push(self.flags);

        // Sequence as varint
        encode_varint(&mut buf, self.sequence as u64)?;

        // Length as varint
        encode_varint(&mut buf, self.payload.len() as u64)?;

        // Payload
        buf.extend_from_slice(&self.payload);

        // CRC8 checksum
        let crc = crc8(&buf);
        buf.push(crc);

        Ok(buf)
    }

    /// Decode from bytes
    pub fn decode(data: &[u8]) -> Result<Self, CompactError> {
        if data.len() < MIN_MESSAGE_SIZE {
            return Err(CompactError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "message too short: {} bytes, minimum {}",
                    data.len(),
                    MIN_MESSAGE_SIZE
                ),
            )));
        }

        // Verify CRC first
        let crc_received = data[data.len() - 1];
        let crc_calculated = crc8(&data[..data.len() - 1]);
        if crc_received != crc_calculated {
            return Err(CompactError::ChecksumMismatch);
        }

        let mut cursor = io::Cursor::new(&data[..data.len() - 1]);

        // Type
        let mut type_byte = [0u8; 1];
        cursor.read_exact(&mut type_byte)?;
        let msg_type = CompactMessageType::try_from(type_byte[0])?;

        // Flags
        let mut flags_byte = [0u8; 1];
        cursor.read_exact(&mut flags_byte)?;
        let flags = flags_byte[0];

        // Sequence - validate fits in u32
        let seq_u64 = decode_varint(&mut cursor)?;
        if seq_u64 > u32::MAX as u64 {
            return Err(CompactError::SequenceOverflow);
        }
        let sequence = seq_u64 as u32;

        // Length - validate against max size
        let len_u64 = decode_varint(&mut cursor)?;
        if len_u64 > MAX_PAYLOAD_SIZE as u64 {
            return Err(CompactError::MessageTooLarge {
                size: len_u64 as usize,
                limit: MAX_PAYLOAD_SIZE,
            });
        }
        let len = len_u64 as usize;

        // Payload
        let pos = cursor.position() as usize;
        let remaining = &data[pos..data.len() - 1];

        if remaining.len() < len {
            return Err(CompactError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "payload truncated",
            )));
        }

        let payload = remaining[..len].to_vec();

        Ok(Self {
            msg_type,
            flags,
            sequence,
            payload,
        })
    }

    /// Get encoded size without actually encoding
    pub fn encoded_size(&self) -> usize {
        2 + // type + flags
        varint_size(self.sequence as u64) +
        varint_size(self.payload.len() as u64) +
        self.payload.len() +
        1 // crc
    }
}

/// Encode a varint
fn encode_varint<W: Write>(w: &mut W, mut value: u64) -> Result<(), CompactError> {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        w.write_all(&[byte])?;
        if value == 0 {
            break;
        }
    }
    Ok(())
}

/// Decode a varint with proper overflow checking
fn decode_varint<R: Read>(r: &mut R) -> Result<u64, CompactError> {
    let mut result = 0u64;
    let mut shift = 0u32;

    loop {
        // Check shift before reading to prevent overflow
        if shift >= 64 {
            return Err(CompactError::InvalidVarint);
        }

        let mut byte = [0u8; 1];
        r.read_exact(&mut byte)?;
        let b = byte[0];

        // For the last valid byte (shift=63), only bit 0 can be set
        let value = (b & 0x7F) as u64;
        if shift == 63 && value > 1 {
            return Err(CompactError::InvalidVarint);
        }

        result |= value << shift;

        if b & 0x80 == 0 {
            break;
        }
        shift += 7;
    }
    Ok(result)
}

/// Calculate varint size
fn varint_size(value: u64) -> usize {
    if value == 0 {
        1
    } else {
        (64 - value.leading_zeros() as usize).div_ceil(7)
    }
}

/// Simple CRC-8-CCITT (polynomial 0x07)
///
/// Note: This provides error detection only, not cryptographic integrity.
fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for byte in data {
        crc ^= byte;
        for _ in 0..8 {
            if crc & 0x80 != 0 {
                crc = (crc << 1) ^ 0x07;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

/// Fragment large messages for constrained MTU
pub struct Fragmenter {
    max_fragment_size: usize,
}

impl Fragmenter {
    /// Create a new fragmenter.
    ///
    /// # Panics
    ///
    /// Panics if `max_fragment_size` is zero.
    pub fn new(max_fragment_size: usize) -> Self {
        assert!(max_fragment_size > 0, "max_fragment_size must be positive");
        Self { max_fragment_size }
    }

    /// Fragment a message if needed.
    ///
    /// # Panics
    ///
    /// Panics if the message would require more than 65535 fragments.
    pub fn fragment(&self, msg: &CompactMessage) -> Vec<CompactMessage> {
        if msg.payload.len() <= self.max_fragment_size {
            return vec![msg.clone()];
        }

        let mut fragments = Vec::new();
        let chunks: Vec<_> = msg.payload.chunks(self.max_fragment_size).collect();

        assert!(
            chunks.len() <= 0xFFFF,
            "Too many fragments: {} exceeds maximum 65535",
            chunks.len()
        );

        for (i, chunk) in chunks.iter().enumerate() {
            let mut frag = CompactMessage {
                msg_type: msg.msg_type,
                flags: msg.flags | 0x02, // Set fragmented flag
                sequence: msg.sequence,
                payload: chunk.to_vec(),
            };

            // Encode fragment index in high bits of sequence
            // Low 16 bits: original sequence, High 16 bits: fragment index
            frag.sequence = (msg.sequence & 0xFFFF) | ((i as u32) << 16);

            // Mark last fragment
            if i == chunks.len() - 1 {
                frag.flags |= 0x04; // Last fragment
            }

            fragments.push(frag);
        }

        fragments
    }

    /// Get the max fragment size
    pub fn max_fragment_size(&self) -> usize {
        self.max_fragment_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_encode_decode() {
        let msg = CompactMessage::data(b"hello".to_vec())
            .with_sequence(42)
            .with_ack_requested();

        let encoded = msg.encode().unwrap();
        let decoded = CompactMessage::decode(&encoded).unwrap();

        assert_eq!(decoded.msg_type, CompactMessageType::Data);
        assert_eq!(decoded.sequence, 42);
        assert!(decoded.ack_requested());
        assert_eq!(decoded.payload, b"hello");
    }

    #[test]
    fn test_ping_pong() {
        let ping = CompactMessage::ping();
        let encoded = ping.encode().unwrap();
        assert!(encoded.len() < 10); // Very compact

        let decoded = CompactMessage::decode(&encoded).unwrap();
        assert_eq!(decoded.msg_type, CompactMessageType::Ping);
    }

    #[test]
    fn test_empty_payload() {
        let msg = CompactMessage::data(vec![]);
        let encoded = msg.encode().unwrap();
        let decoded = CompactMessage::decode(&encoded).unwrap();
        assert_eq!(decoded.payload.len(), 0);
    }

    #[test]
    fn test_max_sequence() {
        let msg = CompactMessage::data(vec![1, 2, 3]).with_sequence(u32::MAX);
        let encoded = msg.encode().unwrap();
        let decoded = CompactMessage::decode(&encoded).unwrap();
        assert_eq!(decoded.sequence, u32::MAX);
    }

    #[test]
    fn test_crc_validation() {
        let msg = CompactMessage::data(b"test".to_vec());
        let mut encoded = msg.encode().unwrap();

        // Corrupt the payload
        if encoded.len() > 4 {
            encoded[4] ^= 0xFF;
        }

        // Should fail CRC check
        assert!(matches!(
            CompactMessage::decode(&encoded),
            Err(CompactError::ChecksumMismatch)
        ));
    }

    #[test]
    fn test_varint() {
        let test_values = [0, 1, 127, 128, 16383, 16384, u32::MAX as u64];

        for &val in &test_values {
            let mut buf = Vec::new();
            encode_varint(&mut buf, val).unwrap();

            let decoded = decode_varint(&mut io::Cursor::new(&buf)).unwrap();
            assert_eq!(val, decoded);
        }
    }

    #[test]
    fn test_varint_overflow() {
        // Create a malformed varint with too many continuation bytes
        let bad_varint = vec![
            0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x01,
        ];
        let result = decode_varint(&mut io::Cursor::new(&bad_varint));
        assert!(matches!(result, Err(CompactError::InvalidVarint)));
    }

    #[test]
    fn test_message_too_short() {
        let short = vec![0, 0, 0, 0]; // 4 bytes, minimum is 5
        assert!(matches!(
            CompactMessage::decode(&short),
            Err(CompactError::Io(_))
        ));
    }

    #[test]
    fn test_payload_size_limit() {
        // Create a message claiming a huge payload
        let mut buf = Vec::new();
        buf.push(2); // Data type
        buf.push(0); // No flags
        encode_varint(&mut buf, 0).unwrap(); // sequence = 0
        encode_varint(&mut buf, (MAX_PAYLOAD_SIZE + 1) as u64).unwrap(); // len > max
        let crc = crc8(&buf);
        buf.push(crc);

        assert!(matches!(
            CompactMessage::decode(&buf),
            Err(CompactError::MessageTooLarge { .. })
        ));
    }

    #[test]
    fn test_fragmentation() {
        let fragmenter = Fragmenter::new(10);
        let msg = CompactMessage::data(b"hello world this is a long message".to_vec());

        let fragments = fragmenter.fragment(&msg);
        assert!(fragments.len() > 1);

        // All but last should have fragmented flag
        for frag in &fragments[..fragments.len() - 1] {
            assert!(frag.is_fragmented());
            assert!(!frag.is_last_fragment());
        }

        // Last should have last fragment flag
        let last = fragments.last().unwrap();
        assert!(last.is_last_fragment());
    }

    #[test]
    fn test_fragment_reassembly_info() {
        let fragmenter = Fragmenter::new(10);
        let msg =
            CompactMessage::data(b"hello world this is a long message".to_vec()).with_sequence(42);

        let fragments = fragmenter.fragment(&msg);

        for (i, frag) in fragments.iter().enumerate() {
            assert_eq!(frag.fragment_index(), i as u16);
            assert_eq!(frag.original_sequence(), 42);
        }
    }

    #[test]
    #[should_panic(expected = "max_fragment_size must be positive")]
    fn test_fragmenter_zero_size() {
        let _ = Fragmenter::new(0);
    }

    #[test]
    fn test_encoded_size() {
        let msg = CompactMessage::data(b"test data".to_vec());
        let encoded = msg.encode().unwrap();
        assert_eq!(msg.encoded_size(), encoded.len());
    }

    #[test]
    fn test_invalid_message_type() {
        let mut buf = Vec::new();
        buf.push(255); // Invalid type
        buf.push(0); // flags
        encode_varint(&mut buf, 0).unwrap();
        encode_varint(&mut buf, 0).unwrap();
        let crc = crc8(&buf);
        buf.push(crc);

        assert!(matches!(
            CompactMessage::decode(&buf),
            Err(CompactError::InvalidMessageType(255))
        ));
    }
}
