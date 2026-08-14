use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use serde::{Deserialize, Serialize};
use std::io::Cursor;

pub const MSG_HELLO: u8 = 0x01;
pub const MSG_HELLO_ACK: u8 = 0x02;
pub const MSG_AUTH: u8 = 0x03;
pub const MSG_AUTH_ACK: u8 = 0x04;
pub const MSG_DISPLAY_CONFIG: u8 = 0x05;
pub const MSG_DISPLAY_CONFIG_ACK: u8 = 0x06;
pub const MSG_STREAM_START: u8 = 0x07;
pub const MSG_STREAM_STOP: u8 = 0x08;
pub const MSG_VIDEO_CONFIG: u8 = 0x10;
pub const MSG_VIDEO_DATA: u8 = 0x11;
pub const MSG_INPUT_EVENT: u8 = 0x20;
pub const MSG_PING: u8 = 0x30;
pub const MSG_PONG: u8 = 0x31;
pub const MSG_METRICS: u8 = 0x40;
pub const MSG_ERROR: u8 = 0xFE;
pub const MSG_DISCONNECT: u8 = 0xFF;

// Video Data Flags
pub const FLAG_KEYFRAME: u16 = 0x0001;
pub const FLAG_END_OF_FRAME: u16 = 0x0002;
pub const FLAG_CONFIG_DATA: u16 = 0x0004;

/// Client Handshake HELLO Message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloMessage {
    pub client_name: String,
    pub client_version: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub density_dpi: u32,
    pub max_fps: u32,
    pub supported_codecs: Vec<String>,
}

/// Server Handshake HELLO_ACK Message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloAckMessage {
    pub server_name: String,
    pub server_version: String,
    pub session_id: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub selected_codec: String,
    pub auth_required: bool,
}

/// Display Configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DisplayConfigMessage {
    pub width: u32,
    pub height: u32,
    pub refresh_rate: u32,
    pub orientation: u32, // 0 = Landscape, 1 = Portrait, 2 = Reverse Landscape, 3 = Reverse Portrait
    pub scale_factor: f64,
}

/// Video Codec Configuration / SPS-PPS
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoConfigMessage {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    #[serde(default)]
    pub extradata: Vec<u8>,
}

/// Input Event Type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum InputEventType {
    TouchDown = 0,
    TouchMove = 1,
    TouchUp = 2,
    TouchCancel = 3,
    PointerMotionAbsolute = 10,
    PointerButton = 11,
    PointerAxis = 12,
    KeyboardKey = 20,
}

impl InputEventType {
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            0 => Some(Self::TouchDown),
            1 => Some(Self::TouchMove),
            2 => Some(Self::TouchUp),
            3 => Some(Self::TouchCancel),
            10 => Some(Self::PointerMotionAbsolute),
            11 => Some(Self::PointerButton),
            12 => Some(Self::PointerAxis),
            20 => Some(Self::KeyboardKey),
            _ => None,
        }
    }
}

/// Binary 16-byte packed input event for low latency
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    pub event_type: InputEventType,
    pub slot: u8,
    pub x: u16,        // Normalized 0..65535
    pub y: u16,        // Normalized 0..65535
    pub code_or_btn: u32, // Keycode / button (1=Left, 2=Middle, 3=Right)
    pub state_or_flags: u32, // Pressed=1, Released=0 / Axis deltas
}

impl InputEvent {
    pub const SIZE: usize = 14;

    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        let mut cursor = Cursor::new(&mut buf[..]);
        cursor.write_u8(self.event_type as u8).unwrap();
        cursor.write_u8(self.slot).unwrap();
        cursor.write_u16::<LittleEndian>(self.x).unwrap();
        cursor.write_u16::<LittleEndian>(self.y).unwrap();
        cursor.write_u32::<LittleEndian>(self.code_or_btn).unwrap();
        cursor.write_u32::<LittleEndian>(self.state_or_flags).unwrap();
        buf
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < Self::SIZE {
            return None;
        }
        let mut cursor = Cursor::new(bytes);
        let event_type = InputEventType::from_u8(cursor.read_u8().ok()?)?;
        let slot = cursor.read_u8().ok()?;
        let x = cursor.read_u16::<LittleEndian>().ok()?;
        let y = cursor.read_u16::<LittleEndian>().ok()?;
        let code_or_btn = cursor.read_u32::<LittleEndian>().ok()?;
        let state_or_flags = cursor.read_u32::<LittleEndian>().ok()?;

        Some(Self {
            event_type,
            slot,
            x,
            y,
            code_or_btn,
            state_or_flags,
        })
    }
}

/// Real-time Session Metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsMessage {
    pub fps: f32,
    pub bitrate_kbps: u32,
    pub rtt_ms: u32,
    pub dropped_frames: u32,
    pub encoder_latency_us: u32,
}

/// Disconnect Message
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisconnectMessage {
    pub code: u32,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_event_roundtrip() {
        let event = InputEvent {
            event_type: InputEventType::TouchDown,
            slot: 1,
            x: 32768,
            y: 16384,
            code_or_btn: 0,
            state_or_flags: 1,
        };

        let encoded = event.encode();
        let decoded = InputEvent::decode(&encoded).unwrap();
        assert_eq!(event, decoded);
    }
}
