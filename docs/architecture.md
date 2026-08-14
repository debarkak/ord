# ORD Architecture & Systems Design

OpenRemoteDisplay (ORD) connects the Linux Wayland graphics subsystem with modern Android video decoding to transform an Android tablet into a true native secondary monitor.

```
┌─────────────────────────────────────────────────────────────┐
│                         LINUX HOST                          │
│                                                             │
│  GNOME Wayland (Mutter)                                     │
│     │  RecordVirtual (D-Bus)                                │
│     ▼                                                       │
│  Native Virtual Display (e.g. 1920x1200 @ 60Hz)             │
│     │  PipeWire Stream (DMA-BUF / SPA Buffers)              │
│     ▼                                                       │
│  Hardware VA-API H.264 Encoder (AMD / Intel GPU)            │
│     │  Annex-B NAL Units                                    │
│     ▼                                                       │
│  ORD/1 Framer & Transport (Tokio TCP Server, port 9090)     │
└──────────────────────────────┬──────────────────────────────┘
                               │
                       Wi-Fi / Ethernet / USB
                               │
                               ▼
┌─────────────────────────────────────────────────────────────┐
│                       ANDROID CLIENT                        │
│                                                             │
│  ORD/1 Packet Parser & TCP Receiver                         │
│     │                                                       │
│     ▼                                                       │
│  Frame Ring-Buffer (Drop-Stale-Frame Latency Guard)         │
│     │                                                       │
│     ▼                                                       │
│  Android MediaCodec (Hardware AVC Decoder)                  │
│     │                                                       │
│     ▼                                                       │
│  SurfaceView Direct Surface Rendering                       │
│     │                                                       │
│     ▼                                                       │
│  Touch / Stylus / Mouse Input Back-Channel                  │
└─────────────────────────────────────────────────────────────┘
```

## 1. Linux Host Pipeline

### 1.1 Virtual Display Manager (`host/src/display/`)
- Interacts with `org.gnome.Mutter.ScreenCast` over the user session D-Bus.
- Invokes `RecordVirtual` with embedded cursor mode.
- Obtains the PipeWire Node ID from the `PipeWireStreamAdded` signal.
- Cleanly releases the virtual display upon client disconnection to avoid ghost monitors.

### 1.2 Video Capture & Hardware Encoding (`host/src/encoder/`)
- Captures frames from PipeWire using `pipewiresrc path=<node_id>`.
- Automatically selects the best hardware encoder:
  - **AMD / Intel**: `vah264enc` (VA-API DRM render node `/dev/dri/renderD128`).
  - **Vulkan**: `vulkanh264enc`.
  - **Software Fallback**: `x264enc tune=zerolatency speed-preset=ultrafast`.
- Streams Annex-B NAL units directly into non-blocking Tokio async channels.

### 1.3 Input Subsystem (`host/src/input/`)
- Interfaces with `org.gnome.Mutter.RemoteDesktop.Session`.
- Injects `NotifyTouchDown`, `NotifyTouchMotion`, `NotifyTouchUp`, and `NotifyPointerMotionAbsolute` directly targeted at the virtual display stream.

---

## 2. Android Client Pipeline

### 2.1 Hardware MediaCodec Pipeline (`android/app/src/main/java/org/ord/client/decoder/`)
- Configures `MediaCodec` with `KEY_LOW_LATENCY = 1` and `KEY_PRIORITY = 0` (real-time).
- Uses direct `Surface` output to avoid costly CPU memory copies.

### 2.2 Latency Recovery Mechanism (`FrameRingBuffer.kt`)
- Video packets are buffered in a bounded queue with a maximum depth of 3 frames.
- If network jitter causes the buffer to fill beyond capacity, the client immediately drops all subsequent frames until an IDR Keyframe arrives. This bounds latency to <30ms under adverse Wi-Fi conditions.

### 2.3 GMS Independence
- 100% free of Google Play Services, Firebase, or cloud dependencies.
- Runs natively on AOSP, One UI 8.0, LineageOS, and custom Android 10-16 ROMs.
