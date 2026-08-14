package org.ord.client.transport

import kotlinx.coroutines.*
import org.ord.client.protocol.OrdHeader
import org.ord.client.protocol.OrdPacket
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.DataInputStream
import java.io.DataOutputStream
import java.net.InetSocketAddress
import java.net.Socket
import java.nio.ByteBuffer
import java.nio.ByteOrder

class TcpClient(
    private val host: String,
    private val port: Int
) {
    private var socket: Socket? = null
    private var inputStream: DataInputStream? = null
    private var outputStream: DataOutputStream? = null

    suspend fun connect(timeoutMs: Int = 5000) = withContext(Dispatchers.IO) {
        val s = Socket()
        s.tcpNoDelay = true
        s.receiveBufferSize = 256 * 1024
        s.sendBufferSize = 64 * 1024
        s.connect(InetSocketAddress(host, port), timeoutMs)
        socket = s
        inputStream = DataInputStream(BufferedInputStream(s.getInputStream()))
        outputStream = DataOutputStream(BufferedOutputStream(s.getOutputStream()))
    }

    suspend fun readPacket(): OrdPacket = withContext(Dispatchers.IO) {
        val stream = inputStream ?: throw IllegalStateException("Socket not connected")

        val headerBytes = ByteArray(OrdPacket.HEADER_SIZE)
        stream.readFully(headerBytes)

        val headerBuf = ByteBuffer.wrap(headerBytes).order(ByteOrder.LITTLE_ENDIAN)
        val magic = ByteArray(4)
        headerBuf.get(magic)
        if (!magic.contentEquals(OrdPacket.MAGIC)) {
            throw IllegalArgumentException("Invalid magic header received")
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

    suspend fun sendPacket(packet: OrdPacket) = withContext(Dispatchers.IO) {
        val stream = outputStream ?: throw IllegalStateException("Socket not connected")
        val encoded = packet.encode()
        stream.write(encoded)
        stream.flush()
    }

    suspend fun sendRaw(msgType: Byte, flags: Short, sequence: Int, payload: ByteArray) = withContext(Dispatchers.IO) {
        val stream = outputStream ?: throw IllegalStateException("Socket not connected")
        val header = OrdHeader(OrdPacket.PROTOCOL_VERSION, msgType, flags, sequence, payload.size)
        val packet = OrdPacket(header, payload)
        stream.write(packet.encode())
        stream.flush()
    }

    fun close() {
        try {
            socket?.close()
        } catch (e: Exception) {
            e.printStackTrace()
        }
        socket = null
        inputStream = null
        outputStream = null
    }
}
