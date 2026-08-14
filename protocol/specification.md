# ORD Wire Protocol v1 Reference

## Packet Constants
- Magic: `0x4F, 0x52, 0x44, 0x31` (`ORD1`)
- Protocol Version: `1`
- Header Size: `16` bytes

## Message Codes
- `0x01`: `HELLO` (Client -> Host)
- `0x02`: `HELLO_ACK` (Host -> Client)
- `0x05`: `DISPLAY_CONFIG` (Host -> Client)
- `0x07`: `STREAM_START` (Client -> Host)
- `0x08`: `STREAM_STOP` (Client -> Host)
- `0x10`: `VIDEO_CONFIG` (Host -> Client)
- `0x11`: `VIDEO_DATA` (Host -> Client)
- `0x20`: `INPUT_EVENT` (Client -> Host)
- `0x30`: `PING` (Bidirectional)
- `0x31`: `PONG` (Bidirectional)
- `0x40`: `METRICS` (Host -> Client)
- `0xFF`: `DISCONNECT` (Bidirectional)

## Video Flags
- `0x0001`: Keyframe (IDR frame)
- `0x0002`: End of frame
- `0x0004`: In-band parameter sets (SPS/PPS)
