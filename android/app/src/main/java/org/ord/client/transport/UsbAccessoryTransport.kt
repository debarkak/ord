package org.ord.client.transport

import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import android.os.ParcelFileDescriptor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.ord.client.protocol.OrdHeader
import org.ord.client.protocol.OrdPacket
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

class UsbAccessoryTransport(
    private val usbManager: UsbManager,
    private val accessory: UsbAccessory
) : OrdTransport {
    private var fileDescriptor: ParcelFileDescriptor? = null
    private var inputStream: DataInputStream? = null
    private var outputStream: DataOutputStream? = null

    suspend fun open(): Boolean = withContext(Dispatchers.IO) {
        val pfd = usbManager.openAccessory(accessory) ?: return@withContext false
        fileDescriptor = pfd
        val fd = pfd.fileDescriptor
        inputStream = DataInputStream(BufferedInputStream(FileInputStream(fd), 256 * 1024))
        outputStream = DataOutputStream(BufferedOutputStream(FileOutputStream(fd), 64 * 1024))
        true
    }

    override suspend fun readPacket(): OrdPacket = withContext(Dispatchers.IO) {
        val stream = inputStream ?: throw IllegalStateException("USB Accessory stream not open")

        val headerBytes = ByteArray(OrdPacket.HEADER_SIZE)
        stream.readFully(headerBytes)

        val headerBuf = ByteBuffer.wrap(headerBytes).order(ByteOrder.LITTLE_ENDIAN)
        val magic = ByteArray(4)
        headerBuf.get(magic)
        if (!magic.contentEquals(OrdPacket.MAGIC)) {
            throw IllegalArgumentException("Invalid magic header received over USB")
        }

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
            stream.readFully(payload)
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
    }
}
