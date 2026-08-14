package org.ord.client.decoder

import android.media.MediaCodec
import android.media.MediaFormat
import android.view.Surface
import kotlinx.coroutines.*
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

class H264Decoder(
    private val surface: Surface,
    private val width: Int,
    private val height: Int,
    private val frameBuffer: FrameRingBuffer
) {
    private var codec: MediaCodec? = null
    private val isRunning = AtomicBoolean(false)
    private var workerJob: Job? = null

    val decodedFramesCount = AtomicInteger(0)
    val droppedFramesCount = AtomicInteger(0)

    fun start(scope: CoroutineScope) {
        if (isRunning.get()) return

        try {
            val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
                setInteger(MediaFormat.KEY_COLOR_FORMAT, android.media.MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
                    setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                }
                if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.M) {
                    setInteger(MediaFormat.KEY_PRIORITY, 0)
                    setInteger(MediaFormat.KEY_OPERATING_RATE, 120)
                }
            }

            var mediaCodec: MediaCodec? = null
            try {
                val c = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
                c.configure(format, surface, null, 0)
                c.start()
                mediaCodec = c
            } catch (e: Exception) {
                mediaCodec?.release()
                mediaCodec = null
            }

            if (mediaCodec == null) {
                try {
                    val basicFormat = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height)
                    val c = MediaCodec.createByCodecName("c2.android.avc.decoder")
                    c.configure(basicFormat, surface, null, 0)
                    c.start()
                    mediaCodec = c
                } catch (e: Exception) {
                    mediaCodec?.release()
                    mediaCodec = null
                }
            }

            codec = mediaCodec ?: throw IllegalStateException("Could not start any H.264 decoder on device")
            isRunning.set(true)

            workerJob = scope.launch(Dispatchers.Default) {
                val bufferInfo = MediaCodec.BufferInfo()

                while (isRunning.get() && isActive) {
                    val activeCodec = codec ?: break
                    var hadActivity = false

                    // 1. Drain available output buffers to Surface with minimal latency
                    try {
                        var outIndex = activeCodec.dequeueOutputBuffer(bufferInfo, 0)
                        while (outIndex >= 0) {
                            activeCodec.releaseOutputBuffer(outIndex, true)
                            decodedFramesCount.incrementAndGet()
                            hadActivity = true
                            outIndex = activeCodec.dequeueOutputBuffer(bufferInfo, 0)
                        }
                    } catch (e: Exception) {
                        if (!isRunning.get()) break
                    }

                    // 2. Feed pending input frame
                    val frame = frameBuffer.poll()
                    if (frame != null) {
                        try {
                            val inIndex = activeCodec.dequeueInputBuffer(1_000) // 1ms max
                            if (inIndex >= 0) {
                                val inputBuffer = activeCodec.getInputBuffer(inIndex)
                                if (inputBuffer != null) {
                                    inputBuffer.clear()
                                    inputBuffer.put(frame.data)
                                    activeCodec.queueInputBuffer(
                                        inIndex,
                                        0,
                                        frame.data.size,
                                        frame.timestampUs,
                                        0
                                    )
                                    hadActivity = true
                                }
                            } else {
                                droppedFramesCount.incrementAndGet()
                            }
                        } catch (e: Exception) {
                            if (!isRunning.get()) break
                        }
                    }

                    if (!hadActivity) {
                        delay(1)
                    }
                }
            }
        } catch (e: Exception) {
            e.printStackTrace()
            stop()
        }
    }

    fun stop() {
        isRunning.set(false)
        workerJob?.cancel()
        workerJob = null

        try {
            codec?.stop()
            codec?.release()
        } catch (e: Exception) {
            e.printStackTrace()
        }
        codec = null
    }
}
