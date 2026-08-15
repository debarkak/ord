package org.ord.client.decoder

import org.ord.client.protocol.OrdConstants
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

data class EncodedVideoFrame(
    val sequence: Int,
    val flags: Short,
    val data: ByteArray,
    val timestampUs: Long = System.nanoTime() / 1000
) {
    val isKeyframe: Boolean
        get() = (flags.toInt() and OrdConstants.FLAG_KEYFRAME.toInt()) != 0
}

class FrameRingBuffer(
    private val maxCapacity: Int = 16
) {
    private val queue = LinkedBlockingQueue<EncodedVideoFrame>(maxCapacity)
    @Volatile
    private var isDroppingUntilKeyframe = false

    fun offer(frame: EncodedVideoFrame): Boolean {
        if (frame.isKeyframe) {
            // Keyframe arrived: safe to resume decoding with zero glitching/tearing
            isDroppingUntilKeyframe = false
        } else if (isDroppingUntilKeyframe) {
            // Drop P-frames until next IDR keyframe to prevent reference-frame tearing
            return false
        }

        if (queue.remainingCapacity() == 0) {
            if (frame.isKeyframe) {
                // If queue full but keyframe arrived, clear queue and accept keyframe
                queue.clear()
            } else {
                // Cannot enqueue P-frame: enter drop-until-keyframe state to avoid macroblock tearing
                isDroppingUntilKeyframe = true
                return false
            }
        }
        return queue.offer(frame)
    }

    fun poll(timeoutMs: Long = 2): EncodedVideoFrame? {
        return queue.poll(timeoutMs, TimeUnit.MILLISECONDS)
    }

    fun clear() {
        queue.clear()
        isDroppingUntilKeyframe = false
    }

    val size: Int
        get() = queue.size
}
