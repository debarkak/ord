package org.ord.client.transport

import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import android.os.ParcelFileDescriptor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.ord.client.protocol.OrdHeader
import org.ord.client.protocol.OrdPacket
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

class UsbAccessoryTransport(
    private val usbManager: UsbManager,
    private val accessory: UsbAccessory
) : OrdTransport {
    private var fileDescriptor: ParcelFileDescriptor? = null
    private var inputStream: FileInputStream? = null
    private var outputStream: FileOutputStream? = null

    // 64KB user-space ring buffer so all read() syscalls are bulk (required by Android /dev/usb_accessory)
    private val rxBuffer = ByteArray(64 * 1024)
    private var rxHead = 0
    private var rxTail = 0

    suspend fun open(): Boolean = withContext(Dispatchers.IO) {
        val pfd = usbManager.openAccessory(accessory) ?: return@withContext false
        fileDescriptor = pfd
        val fd = pfd.fileDescriptor
        inputStream = FileInputStream(fd)
        outputStream = FileOutputStream(fd)
        rxHead = 0
        rxTail = 0
        true
    }

    private fun readNextByte(stream: FileInputStream): Int {
        if (rxHead >= rxTail) {
            rxHead = 0
            val n = stream.read(rxBuffer, 0, rxBuffer.size)
            if (n <= 0) return -1
            rxTail = n
        }
        return rxBuffer[rxHead++].toInt() and 0xFF
    }

    private fun readExact(stream: FileInputStream, buffer: ByteArray, offset: Int = 0, length: Int = buffer.size) {
        var copied = 0
        while (copied < length) {
            val available = rxTail - rxHead
            if (available > 0) {
                val toCopy = minOf(available, length - copied)
                System.arraycopy(rxBuffer, rxHead, buffer, offset + copied, toCopy)
                rxHead += toCopy
                copied += toCopy
            } else {
                rxHead = 0
                val n = stream.read(rxBuffer, 0, rxBuffer.size)
                if (n <= 0) throw java.io.EOFException("USB stream reached EOF")
                rxTail = n
            }
        }
    }

    override suspend fun readPacket(): OrdPacket = withContext(Dispatchers.IO) {
        val stream = inputStream ?: throw IllegalStateException("USB Accessory stream not open")

        // Fast memory sliding-window sync to MAGIC (0x4F, 0x52, 0x44, 0x31 = "ORD1")
        var m0 = 0
        var m1 = 0
        var m2 = 0
        var m3 = 0
        while (true) {
            val b = readNextByte(stream)
            if (b == -1) throw java.io.EOFException("USB stream closed")
            m0 = m1
            m1 = m2
            m2 = m3
            m3 = b
            if (m0 == 0x4F && m1 == 0x52 && m2 == 0x44 && m3 == 0x31) {
                break
            }
        }

        // Read remaining 12 bytes of header (version, msgType, flags, sequence, payloadLen)
        val restHeader = ByteArray(12)
        readExact(stream, restHeader)

        val headerBuf = ByteBuffer.wrap(restHeader).order(ByteOrder.LITTLE_ENDIAN)
        val version = headerBuf.get()
        val msgType = headerBuf.get()
        val flags = headerBuf.short
        val sequence = headerBuf.int
        val payloadLen = headerBuf.int

        if (payloadLen < 0 || payloadLen > 16 * 1024 * 1024) {
            throw IllegalArgumentException("Invalid payload length: $payloadLen")
        }

        val payload = ByteArray(payloadLen)
        if (payloadLen > 0) {
            readExact(stream, payload)
        }

        val header = OrdHeader(version, msgType, flags, sequence, payloadLen)
        OrdPacket(header, payload)
    }

    override suspend fun sendPacket(packet: OrdPacket) = withContext(Dispatchers.IO) {
        val stream = outputStream ?: throw IllegalStateException("USB Accessory stream not open")
        val encoded = packet.encode()
        stream.write(encoded)
        stream.flush()
    }

    override suspend fun sendRaw(msgType: Byte, flags: Short, sequence: Int, payload: ByteArray) = withContext(Dispatchers.IO) {
        val stream = outputStream ?: throw IllegalStateException("USB Accessory stream not open")
        val header = OrdHeader(OrdPacket.PROTOCOL_VERSION, msgType, flags, sequence, payload.size)
        val packet = OrdPacket(header, payload)
        stream.write(packet.encode())
        stream.flush()
    }

    override fun close() {
        try {
            inputStream?.close()
        } catch (_: Exception) {}
        try {
            outputStream?.close()
        } catch (_: Exception) {}
        try {
            fileDescriptor?.close()
        } catch (_: Exception) {}
        inputStream = null
        outputStream = null
        fileDescriptor = null
        rxHead = 0
        rxTail = 0
    }
}
