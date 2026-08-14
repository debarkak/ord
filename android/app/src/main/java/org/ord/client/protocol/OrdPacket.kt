package org.ord.client.protocol

import java.nio.ByteBuffer
import java.nio.ByteOrder

class OrdPacket(
    val header: OrdHeader,
    val payload: ByteArray
) {
    companion object {
        val MAGIC = byteArrayOf(0x4F, 0x52, 0x44, 0x31) // "ORD1"
        const val PROTOCOL_VERSION: Byte = 1
        const val HEADER_SIZE = 16

        fun decode(buffer: ByteBuffer): OrdPacket? {
            if (buffer.remaining() < HEADER_SIZE) return null
            buffer.order(ByteOrder.LITTLE_ENDIAN)

            val magic = ByteArray(4)
            buffer.get(magic)
            if (!magic.contentEquals(MAGIC)) {
                throw IllegalArgumentException("Invalid magic header")
            }

            val version = buffer.get()
            val msgType = buffer.get()
            val flags = buffer.short
            val sequence = buffer.int
            val payloadLen = buffer.int

            if (payloadLen < 0 || payloadLen > 16 * 1024 * 1024) {
                throw IllegalArgumentException("Invalid payload length: $payloadLen")
            }

            if (buffer.remaining() < payloadLen) {
                // Not enough bytes yet
                return null
            }

            val payload = ByteArray(payloadLen)
            buffer.get(payload)

            val header = OrdHeader(version, msgType, flags, sequence, payloadLen)
            return OrdPacket(header, payload)
        }
    }

    fun encode(): ByteArray {
        val buffer = ByteBuffer.allocate(HEADER_SIZE + payload.size)
        buffer.order(ByteOrder.LITTLE_ENDIAN)
        buffer.put(MAGIC)
        buffer.put(header.version)
        buffer.put(header.msgType)
        buffer.putShort(header.flags)
        buffer.putInt(header.sequence)
        buffer.putInt(payload.size)
        buffer.put(payload)
        return buffer.array()
    }
}

data class OrdHeader(
    val version: Byte,
    val msgType: Byte,
    val flags: Short,
    val sequence: Int,
    val payloadLen: Int
)
