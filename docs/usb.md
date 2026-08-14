# USB Transport & Tethering

## 1. USB Connection Overview
While ORD runs primarily over Wi-Fi and local Ethernet, USB connectivity is fully supported for ultra-low latency and zero-interference environments.

## 2. Supported USB Modes

### Mode A: USB Tethering (Recommended & Out-of-the-Box)
1. Connect Samsung Galaxy Tab A9+ to Linux via USB-C cable.
2. On the tablet, enable **Settings → Connections → Mobile Hotspot and Tethering → USB Tethering** (or Ethernet tethering).
3. Linux will automatically assign an IP (e.g. `192.168.42.x`).
4. ORD UDP Discovery will automatically detect the host over the USB link with <1ms RTT.

### Mode B: ADB Port Forwarding (Development Mode)
When USB debugging is enabled:
```bash
adb reverse tcp:9090 tcp:9090
```
In the Android app, connect to `127.0.0.1:9090`.

### Mode C: Direct USB Accessory (Roadmap)
A native USB accessory transport implementing custom USB endpoints is planned for future milestones.
