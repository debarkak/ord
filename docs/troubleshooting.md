# Troubleshooting & Diagnostics

## 1. Checking Host Status
Run the built-in diagnostics command:
```bash
ord diagnostics
```
Verify that:
- **Session Type** is `wayland`.
- **GNOME Shell** is detected.
- **Hardware VA-API** shows `Available (Accelerated)`.
- **PipeWire Status** shows `Ready`.

## 2. Testing Video Pipeline Independently
If the Android client connects but shows a black screen, verify the encoder pipeline with synthetic test patterns:
```bash
ord test-pattern --port 9090
```
This bypasses Mutter and streams a known SMPTE color bar pattern to verify the Android `MediaCodec` decoder independently.

## 3. Firewall Ports
If the Android client cannot discover or connect to the host, open ports `9090` (TCP) and `9091` (UDP):
```bash
# UFW
sudo ufw allow 9090/tcp
sudo ufw allow 9091/udp

# firewalld
sudo firewall-cmd --add-port=9090/tcp --permanent
sudo firewall-cmd --add-port=9091/udp --permanent
sudo firewall-cmd --reload
```

## 4. Hardware Encoder Permissions
Ensure your user belongs to the `video` and `render` groups:
```bash
sudo usermod -aG video,render $USER
```
