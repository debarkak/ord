# ORD — OpenRemoteDisplay

**OpenRemoteDisplay (ORD)** is a high-performance, open-source, Linux-native virtual display and streaming system. It allows a Linux computer (optimized for GNOME on Wayland) to expose a genuine virtual monitor and stream it with low latency to an Android device (such as the Samsung Galaxy Tab A9+), turning the tablet into a real secondary display.

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

---

## Key Features

- **Genuine Virtual Monitor**: Linux detects a real secondary display. Windows can be moved onto it and arranged via standard GNOME Display settings.
- **Wayland Native**: Built directly on GNOME Mutter D-Bus APIs and PipeWire media streams.
- **Hardware Accelerated**: Zero-copy DMA-BUF capture with AMD/Intel VA-API H.264 hardware encoding on Linux and `MediaCodec` hardware decoding on Android.
- **Ultra-Low Latency**: Frame-dropping latency recovery guard ensures interactive responsiveness (<25ms over 5GHz Wi-Fi / USB).
- **100% GMS-Free**: The Android client is completely independent of Google Play Services and runs on Android 10 through Android 16 (including Chinese ROMs, One UI 8.0, LineageOS, and AOSP).
- **Interactive Multi-Touch**: Touch gestures on Android are mapped and injected back into Linux via Mutter RemoteDesktop.
- **LAN Discovery & Manual IP**: Automatic UDP broadcast discovery beacon with instant manual IP fallback.
- **Standalone Diagnostics & Test Pattern**: Built-in CLI for inspecting hardware encoder capabilities and testing network streaming independently.

---

## Quick Start

### 1. Run Linux Host Daemon

```bash
# Build the host binary
cd host
cargo build --release

# Inspect system hardware and encoder support
./target/release/ord diagnostics

# Start the ORD daemon
./target/release/ord start
```

### 2. Run Android Client

```bash
cd android

# Build debug APK
./gradlew assembleDebug

# Install APK onto connected Android tablet
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

Open the **ORD** app on your Android tablet, tap your Linux laptop in the discovered hosts list, and your tablet immediately becomes your secondary monitor!

---

## CLI Reference

```
OpenRemoteDisplay - Linux Virtual Display Host & CLI

Usage: ord [OPTIONS] [COMMAND]

Commands:
  start         Start the ORD host daemon
  diagnostics   Run host environment diagnostics and check hardware acceleration
  test-pattern  Stream synthetic test pattern for client decoder testing and latency measurement
  hosts         Discover other ORD hosts on the local network
  init-config   Generate or reset default configuration file in ~/.config/ord/config.toml
  help          Print this message or the help of the given subcommand(s)

Options:
  -c, --config <CONFIG>  Path to custom config file
  -v, --verbose          Enable verbose / debug logging
  -h, --help             Print help
  -V, --version          Print version
```

### Examples

```bash
# Start on custom port and force VA-API hardware encoder at 20 Mbps
ord start --port 9090 --encoder vaapi --bitrate 20000

# Stream synthetic test pattern to test Android decoder independently
ord test-pattern --port 9090

# Discover active ORD hosts on the local subnet
ord hosts --timeout 5
```

---

## Systemd Integration

To run ORD automatically in the background on your user session login:

```bash
mkdir -p ~/.config/systemd/user/
cp systemd/ord.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now ord
```

---

## Project Structure

```
ord/
├── Cargo.toml                  # Workspace Cargo config
├── host/                       # Linux Host Daemon & CLI (Rust)
│   ├── src/
│   │   ├── main.rs             # CLI entrypoint (ord start, diagnostics, test-pattern)
│   │   ├── daemon.rs           # Daemon service manager
│   │   ├── config.rs           # TOML configuration loader
│   │   ├── diagnostics.rs      # System hardware inspector
│   │   ├── display/            # Virtual display backends (Mutter D-Bus, Test Source)
│   │   ├── encoder/            # GStreamer VA-API & software H.264 pipelines
│   │   ├── input/              # Mutter RemoteDesktop & uinput backends
│   │   ├── protocol/           # Binary ORD/1 packet framing & coordinate mapping
│   │   ├── session/            # Session coordinator & stream pump
│   │   └── transport/          # Async TCP transport & UDP discovery
│   └── tests/                  # Automated integration tests
├── android/                    # Android Client App (Kotlin)
│   └── app/src/main/
│       ├── AndroidManifest.xml
│       ├── res/                # Layouts, themes, dark mode assets
│       └── java/org/ord/client/
│           ├── discovery/      # UDP discovery listener
│           ├── transport/      # Async TCP stream client
│           ├── decoder/        # MediaCodec low-latency pipeline & ring-buffer
│           ├── input/          # Multi-touch event mapper
│           └── ui/             # Discovery, Display, Settings, Diagnostics screens
├── protocol/                   # Protocol specification
├── docs/                       # Comprehensive documentation
│   ├── architecture.md
│   ├── protocol.md
│   ├── linux-display.md
│   ├── android.md
│   ├── building.md
│   ├── development.md
│   ├── troubleshooting.md
│   ├── performance.md
│   └── usb.md
├── systemd/                    # Systemd service unit
├── LICENSE                     # Apache-2.0
└── README.md
```

---

## License

ORD is licensed under the [Apache License, Version 2.0](LICENSE).
