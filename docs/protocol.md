# ORD Protocol Specification (ORD/1)

The **ORD Protocol** (version `ORD/1`) is a low-latency, binary-framed network protocol designed specifically for transmitting virtual display frames and interactive input between a Linux host and an Android client.

## 1. Packet Framing

All packets in the ORD protocol start with a fixed **16-byte Little-Endian header**:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                 Magic: "ORD1" (0x4F, 0x52, 0x44, 0x31)        |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|  Version (1)  | MsgType (u8)  |          Flags (u16)          |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                 Sequence / Frame ID / Timestamp (u32)         |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                      Payload Length (u32)                     |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                       Payload Bytes ...                       |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

### Header Fields
- **Magic (4 bytes)**: `0x4F, 0x52, 0x44, 0x31` (`"ORD1"` in ASCII).
- **Version (1 byte)**: `0x01` for `ORD/1`.
- **MsgType (1 byte)**: The message identifier.
- **Flags (2 bytes, LE)**:
  - `0x0001` (`FLAG_KEYFRAME`): Indicates the video frame is an IDR/I-Frame (Keyframe).
  - `0x0002` (`FLAG_END_OF_FRAME`): Indicates completion of frame transmission.
  - `0x0004` (`FLAG_CONFIG_DATA`): Out-of-band codec parameters (SPS/PPS).
- **Sequence (4 bytes, LE)**: Monotonically increasing frame or packet counter.
- **Payload Length (4 bytes, LE)**: Length of the payload in bytes (0 to 16,777,216).

---

## 2. Message Types

| Value | Identifier | Direction | Description |
|-------|------------|-----------|-------------|
| `0x01` | `HELLO` | Client → Host | Client handshake, screen dimensions, DPI, and supported codecs. |
| `0x02` | `HELLO_ACK` | Host → Client | Server handshake acknowledgment with negotiated resolution. |
| `0x03` | `AUTH_REQ` | Client → Host | Optional PIN / authentication token. |
| `0x04` | `AUTH_ACK` | Host → Client | Authentication confirmation. |
| `0x05` | `DISPLAY_CONFIG` | Host → Client | Virtual display geometry, orientation, and scaling factor. |
| `0x07` | `STREAM_START` | Client → Host | Request video stream begin. |
| `0x08` | `STREAM_STOP` | Client → Host | Pause or stop video stream. |
| `0x10` | `VIDEO_CONFIG` | Host → Client | Codec configuration bytes (H.264 SPS/PPS). |
| `0x11` | `VIDEO_DATA` | Host → Client | Encoded video stream NAL unit payload. |
| `0x20` | `INPUT_EVENT` | Client → Host | Normalized touch, pointer, or keyboard interaction. |
| `0x30` | `PING` | Bidirectional | Latency probe. |
| `0x31` | `PONG` | Bidirectional | Latency reply. |
| `0x40` | `METRICS` | Host → Client | Real-time session stats (FPS, bitrate, encoder latency). |
| `0xFF` | `DISCONNECT` | Bidirectional | Graceful session termination. |

---

## 3. Input Event Format (`0x20`)

Input events use a compact **14-byte binary payload** for minimal latency:

```
Offset  Size    Type    Field           Description
0       1       u8      EventType       0=TouchDown, 1=TouchMove, 2=TouchUp, 3=TouchCancel, 10=PointerMotionAbsolute, 11=PointerButton, 12=PointerAxis, 20=KeyboardKey
1       1       u8      Slot            Multi-touch pointer slot ID (0-9)
2       2       u16     NormX           Normalized X coordinate [0..65535] (Little Endian)
4       2       u16     NormY           Normalized Y coordinate [0..65535] (Little Endian)
6       4       u32     CodeOrBtn       Keycode or mouse button (1=Left, 2=Middle, 3=Right)
10      4       u32     StateOrFlags    Pressed (1), Released (0), or axis scroll values
```

### Coordinate Normalization
Client touch coordinates are mapped into the range `[0..65535]`:
$$\text{NormX} = \text{clamp}\left(\frac{\text{TouchX}}{\text{ScreenWidth}} \times 65535, 0, 65535\right)$$
$$\text{NormY} = \text{clamp}\left(\frac{\text{TouchY}}{\text{ScreenHeight}} \times 65535, 0, 65535\right)$$

The host scales these back to the virtual display's exact resolution without rounding jitter.

---

## 4. Discovery Protocol

ORD hosts broadcast a JSON discovery beacon over UDP port `9091` to `255.255.255.255` every 2 seconds:

```json
{
  "magic": "ORD_DISCOVERY",
  "version": 1,
  "hostname": "debarka-laptop",
  "port": 9090,
  "supported_codecs": ["h264"],
  "auth_required": false
}
```
Clients listen on UDP port `9091` to dynamically populate the host picker screen.
