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
                // Low latency & real-time optimization for MediaCodec
                setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                setInteger(MediaFormat.KEY_PRIORITY, 0)
            }

            codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC).apply {
                configure(format, surface, null, 0)
                start()
            }

            isRunning.set(true)

            workerJob = scope.launch(Dispatchers.Default) {
                val bufferInfo = MediaCodec.BufferInfo()

                while (isRunning.get() && isActive) {
                    val mediaCodec = codec ?: break

                    // 1. Feed input buffer from frame ring-buffer
                    val frame = frameBuffer.poll()
                    if (frame != null) {
                        try {
                            val inIndex = mediaCodec.dequeueInputBuffer(10_000) // 10ms timeout
                            if (inIndex >= 0) {
                                val inputBuffer = mediaCodec.getInputBuffer(inIndex)
                                if (inputBuffer != null) {
                                    inputBuffer.clear()
                                    inputBuffer.put(frame.data)
                                    mediaCodec.queueInputBuffer(
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

                    // 2. Drain decoded output buffers to Surface
                    try {
                        var outIndex = mediaCodec.dequeueOutputBuffer(bufferInfo, 0)
                        while (outIndex >= 0) {
                            // Render directly to Surface with true
                            mediaCodec.releaseOutputBuffer(outIndex, true)
                            decodedFramesCount.incrementAndGet()
                            outIndex = mediaCodec.dequeueOutputBuffer(bufferInfo, 0)
                        }
                    } catch (e: Exception) {
                        if (!isRunning.get()) break
                    }

                    if (frame == null) {
                        // Yield CPU when queue is empty
                        delay(2)
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
