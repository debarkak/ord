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
                if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.R) {
                    setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                }
                if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.M) {
                    setInteger(MediaFormat.KEY_PRIORITY, 0)
                    setInteger(MediaFormat.KEY_OPERATING_RATE, 240)
                }
            }

            var mediaCodec: MediaCodec? = null

            // 1. Try Qualcomm dedicated low-latency hardware decoder first
            val preferredCodecs = listOf(
                "c2.qti.avc.decoder.low_latency",
                "c2.qti.avc.decoder",
                "OMX.qcom.video.decoder.avc.low_latency",
                "OMX.qcom.video.decoder.avc"
            )

            for (codecName in preferredCodecs) {
                try {
                    val c = MediaCodec.createByCodecName(codecName)
                    c.configure(format, surface, null, 0)
                    c.start()
                    mediaCodec = c
                    break
                } catch (e: Exception) {
                    mediaCodec?.release()
                    mediaCodec = null
                }
            }

            // 2. Try standard system decoder by type
            if (mediaCodec == null) {
                try {
                    val c = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
                    c.configure(format, surface, null, 0)
                    c.start()
                    mediaCodec = c
                } catch (e: Exception) {
                    mediaCodec?.release()
                    mediaCodec = null
                }
            }

            // 3. Fallback to software decoder only if hardware is unavailable
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

            val activeCodec = mediaCodec ?: throw IllegalStateException("Could not start any H.264 decoder on device")
            codec = activeCodec
            isRunning.set(true)

            // Worker Job: Continuous Ultra-Fast Draining & Feeding
            workerJob = scope.launch(Dispatchers.Default) {
                val bufferInfo = MediaCodec.BufferInfo()

                // Output Draining Coroutine (Max Framerate, Zero Delay)
                launch(Dispatchers.Default) {
                    while (isRunning.get() && isActive) {
                        try {
                            // Use 1000us (1ms) native blocking wait instead of CPU sleeping
                            val outIndex = activeCodec.dequeueOutputBuffer(bufferInfo, 1_000)
                            if (outIndex >= 0) {
                                activeCodec.releaseOutputBuffer(outIndex, true)
                                decodedFramesCount.incrementAndGet()
                            }
                        } catch (e: Exception) {
                            if (!isRunning.get()) break
                        }
                    }
                }

                // Input Feeding Loop (Instantaneous queueing with 0ms wakeup)
                while (isRunning.get() && isActive) {
                    val frame = frameBuffer.poll(2)
                    if (frame != null) {
                        try {
                            val inIndex = activeCodec.dequeueInputBuffer(2_000) // 2ms native timeout
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
                                }
                            } else {
                                droppedFramesCount.incrementAndGet()
                            }
                        } catch (e: Exception) {
                            if (!isRunning.get()) break
                        }
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
