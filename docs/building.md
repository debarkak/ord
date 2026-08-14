# Building ORD

## 1. Building the Linux Host Daemon (`ord`)

### Prerequisites (EndeavourOS / Arch Linux)
```bash
sudo pacman -S rust cargo gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav pipewire libva-mesa-driver mesa-utils
```

### Build Instructions
```bash
# Clone the repository
git clone https://github.com/debarkak/ord.git
cd ord

# Build in release mode
cargo build --release

# Run diagnostics
./target/release/ord diagnostics

# Run tests
cargo test
```

---

## 2. Building the Android Client (`ord-android`)

### Prerequisites
- JDK 17+ (e.g. OpenJDK 21)
- Android SDK (Build tools 36.0.0, Platform API 36)

### Build Instructions
```bash
cd android

# Build debug APK
./gradlew assembleDebug

# Install onto connected Samsung Galaxy Tab A9+ via ADB
adb install -r app/build/outputs/apk/debug/app-debug.apk
```
