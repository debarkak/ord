package org.ord.client.transport

import org.ord.client.protocol.OrdPacket

interface OrdTransport {
    suspend fun readPacket(): OrdPacket
    suspend fun sendPacket(packet: OrdPacket)
    suspend fun sendRaw(msgType: Byte, flags: Short, sequence: Int, payload: ByteArray)
    fun close()
}
