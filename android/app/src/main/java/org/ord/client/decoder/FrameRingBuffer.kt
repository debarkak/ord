package org.ord.client.decoder

import org.ord.client.protocol.OrdConstants
import java.util.concurrent.ConcurrentLinkedQueue
import java.util.concurrent.atomic.AtomicInteger

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
    private val maxCapacity: Int = 3
) {
    private val queue = ConcurrentLinkedQueue<EncodedVideoFrame>()
    private val count = AtomicInteger(0)
    private var isDropping = false

    fun offer(frame: EncodedVideoFrame): Boolean {
        // If queue exceeds capacity and a keyframe arrives, reset to keyframe
        if (frame.isKeyframe && count.get() > 3) {
            queue.clear()
            count.set(0)
            isDropping = false
        }

        if (count.get() >= maxCapacity) {
            if (!frame.isKeyframe) {
                return false
            }
        }

        queue.offer(frame)
        count.incrementAndGet()
        return true
    }

    fun poll(): EncodedVideoFrame? {
        val frame = queue.poll()
        if (frame != null) {
            count.decrementAndGet()
        }
        return frame
    }

    fun clear() {
        queue.clear()
        count.set(0)
        isDropping = false
    }

    val size: Int
        get() = count.get()
}
