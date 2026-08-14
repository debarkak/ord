# Android Client Architecture

## Device Optimization: Samsung Galaxy Tab A9+
- **SoC**: Qualcomm Snapdragon 695 / Adreno 619
- **Display**: 11-inch 1920x1200 WUXGA LCD @ 90Hz
- **OS Target**: One UI 8.0 / Android 16 (Chinese ROM & Global)
- **Decoder**: Qualcomm hardware H.264 video decoder (`c2.qti.avc.decoder`)

## Compatibility Matrix
- **Minimum Android Version**: Android 10 (API 29)
- **Target Android Version**: Android 16 (API 36)
- **Google Mobile Services**: 0% dependence. Fully functional on GMS-free ROMs.

## Video Decoding Pipeline
```
TCP Socket Stream → OrdPacket Parser → FrameRingBuffer → MediaCodec (AVC) → SurfaceView
```

### MediaCodec Configuration
```kotlin
val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
    setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
    setInteger(MediaFormat.KEY_PRIORITY, 0) // Real-time priority
}
codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).apply {
    configure(format, surface, null, 0)
    start()
}
```

### Touch Interaction
Multi-touch gestures (`ACTION_DOWN`, `ACTION_MOVE`, `ACTION_UP`, `ACTION_POINTER_DOWN`, `ACTION_POINTER_UP`) are captured on the `SurfaceView`, normalized into `[0..65535]`, and sent back to the Linux host with <5ms latency.
