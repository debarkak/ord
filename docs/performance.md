# Performance & Latency Optimization

## Performance Targets
- **End-to-End Latency**: <25 ms over 5GHz Wi-Fi / Ethernet
- **Framerate**: 60 FPS sustained (up to 90/120 FPS on supported panels)
- **Host CPU Usage**: <5% with VA-API hardware acceleration
- **Client Battery Consumption**: <8% per hour during active streaming

## Optimization Strategies

### 1. TCP_NODELAY (Nagle's Algorithm Disabled)
Both host and Android client enable `TCP_NODELAY` immediately upon socket creation. This eliminates the 40ms TCP packet coalescing delay.

### 2. GStreamer AppSink Bounded Queue
`appsink` on the host is configured with:
```
appsink emit-signals=true max-buffers=2 drop=true sync=false
```
If network bandwidth temporarily drops, the host drops old frames inside GStreamer rather than allowing a backlog to develop in memory.

### 3. Client Ring-Buffer Keyframe Recovery
On Android, `FrameRingBuffer` monitors the queue depth. If the buffer exceeds 3 frames, non-keyframe packets are discarded until a fresh IDR Keyframe arrives.

### 4. Direct Surface Rendering
Frames decoded by Android's `MediaCodec` are rendered directly into the underlying hardware `Surface` with `releaseOutputBuffer(index, true)`. No intermediate `Bitmap` allocations or CPU memory copies occur.
