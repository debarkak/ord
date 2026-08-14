use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read, Write};
use thiserror::Error;

pub const ORD_MAGIC: [u8; 4] = [0x4F, 0x52, 0x44, 0x31]; // "ORD1"
pub const ORD_PROTOCOL_VERSION: u8 = 1;
pub const ORD_HEADER_SIZE: usize = 16;

#[derive(Debug, Error)]
pub enum PacketError {
    #[error("Invalid magic: expected ORD1, got {0:?}")]
    InvalidMagic([u8; 4]),
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(u8),
    #[error("Incomplete packet: expected {expected} bytes, have {available} bytes")]
    IncompletePacket { expected: usize, available: usize },
    #[error("Payload exceeds maximum size: {0} bytes")]
    PayloadTooLarge(usize),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

pub const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024; // 16 MB max frame payload

/// Fixed 16-byte ORD Packet Header
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OrdHeader {
    pub version: u8,
    pub msg_type: u8,
    pub flags: u16,
    pub sequence: u32,
    pub payload_len: u32,
}

impl OrdHeader {
    pub fn new(msg_type: u8, flags: u16, sequence: u32, payload_len: u32) -> Self {
        Self {
            version: ORD_PROTOCOL_VERSION,
            msg_type,
            flags,
            sequence,
            payload_len,
        }
    }

    pub fn encode(&self) -> [u8; ORD_HEADER_SIZE] {
        let mut buf = [0u8; ORD_HEADER_SIZE];
        let mut cursor = Cursor::new(&mut buf[..]);
        cursor.write_all(&ORD_MAGIC).unwrap();
        cursor.write_u8(self.version).unwrap();
        cursor.write_u8(self.msg_type).unwrap();
        cursor.write_u16::<LittleEndian>(self.flags).unwrap();
        cursor.write_u32::<LittleEndian>(self.sequence).unwrap();
        cursor.write_u32::<LittleEndian>(self.payload_len).unwrap();
        buf
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PacketError> {
        if bytes.len() < ORD_HEADER_SIZE {
            return Err(PacketError::IncompletePacket {
                expected: ORD_HEADER_SIZE,
                available: bytes.len(),
            });
        }

        let mut cursor = Cursor::new(bytes);
        let mut magic = [0u8; 4];
        cursor.read_exact(&mut magic)?;
        if magic != ORD_MAGIC {
            return Err(PacketError::InvalidMagic(magic));
        }

        let version = cursor.read_u8()?;
        if version != ORD_PROTOCOL_VERSION {
            return Err(PacketError::UnsupportedVersion(version));
        }

        let msg_type = cursor.read_u8()?;
        let flags = cursor.read_u16::<LittleEndian>()?;
        let sequence = cursor.read_u32::<LittleEndian>()?;
        let payload_len = cursor.read_u32::<LittleEndian>()?;

        if payload_len as usize > MAX_PAYLOAD_SIZE {
            return Err(PacketError::PayloadTooLarge(payload_len as usize));
        }

        Ok(Self {
            version,
            msg_type,
            flags,
            sequence,
            payload_len,
        })
    }
}

/// A complete framed ORD Packet
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrdPacket {
    pub header: OrdHeader,
    pub payload: Vec<u8>,
}

impl OrdPacket {
    pub fn new(msg_type: u8, flags: u16, sequence: u32, payload: Vec<u8>) -> Self {
        let payload_len = payload.len() as u32;
        Self {
            header: OrdHeader::new(msg_type, flags, sequence, payload_len),
            payload,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(ORD_HEADER_SIZE + self.payload.len());
        bytes.extend_from_slice(&self.header.encode());
        bytes.extend_from_slice(&self.payload);
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, PacketError> {
        let header = OrdHeader::decode(bytes)?;
        let total_len = ORD_HEADER_SIZE + header.payload_len as usize;

        if bytes.len() < total_len {
            return Err(PacketError::IncompletePacket {
                expected: total_len,
                available: bytes.len(),
            });
        }

        let payload = bytes[ORD_HEADER_SIZE..total_len].to_vec();
        Ok(Self { header, payload })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_encode_decode() {
        let header = OrdHeader::new(0x11, 0x0001, 42, 1024);
        let encoded = header.encode();
        assert_eq!(encoded.len(), ORD_HEADER_SIZE);

        let decoded = OrdHeader::decode(&encoded).unwrap();
        assert_eq!(header, decoded);
    }

    #[test]
    fn test_packet_roundtrip() {
        let payload = b"Hello ORD Tablet!".to_vec();
        let packet = OrdPacket::new(0x01, 0, 100, payload.clone());
        let encoded = packet.encode();

        let decoded = OrdPacket::decode(&encoded).unwrap();
        assert_eq!(packet, decoded);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn test_invalid_magic() {
        let mut bad_bytes = [0u8; 16];
        bad_bytes[0..4].copy_from_slice(b"BAD1");
        assert!(OrdHeader::decode(&bad_bytes).is_err());
    }
}
