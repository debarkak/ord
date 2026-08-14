package org.ord.client.protocol

import org.json.JSONArray
import org.json.JSONObject
import java.nio.ByteBuffer
import java.nio.ByteOrder

object OrdConstants {
    const val MSG_HELLO: Byte = 0x01
    const val MSG_HELLO_ACK: Byte = 0x02
    const val MSG_DISPLAY_CONFIG: Byte = 0x05
    const val MSG_STREAM_START: Byte = 0x07
    const val MSG_STREAM_STOP: Byte = 0x08
    const val MSG_VIDEO_CONFIG: Byte = 0x10
    const val MSG_VIDEO_DATA: Byte = 0x11
    const val MSG_INPUT_EVENT: Byte = 0x20
    const val MSG_PING: Byte = 0x30
    const val MSG_PONG: Byte = 0x31
    const val MSG_METRICS: Byte = 0x40
    const val MSG_DISCONNECT: Byte = -0x01 // 0xFF

    const val FLAG_KEYFRAME: Short = 0x0001
    const val FLAG_END_OF_FRAME: Short = 0x0002
}

data class HelloMessage(
    val clientName: String,
    val clientVersion: String,
    val screenWidth: Int,
    val screenHeight: Int,
    val densityDpi: Int,
    val maxFps: Int,
    val supportedCodecs: List<String>
) {
    fun toJson(): String {
        val json = JSONObject()
        json.put("client_name", clientName)
        json.put("client_version", clientVersion)
        json.put("screen_width", screenWidth)
        json.put("screen_height", screenHeight)
        json.put("density_dpi", densityDpi)
        json.put("max_fps", maxFps)
        val codecs = JSONArray()
        supportedCodecs.forEach { codecs.put(it) }
        json.put("supported_codecs", codecs)
        return json.toString()
    }
}

data class HelloAckMessage(
    val serverName: String,
    val serverVersion: String,
    val sessionId: String,
    val width: Int,
    val height: Int,
    val fps: Int,
    val selectedCodec: String,
    val authRequired: Boolean
) {
    companion object {
        fun fromJson(jsonStr: String): HelloAckMessage {
            val obj = JSONObject(jsonStr)
            return HelloAckMessage(
                serverName = obj.optString("server_name", "ORD-Host"),
                serverVersion = obj.optString("server_version", "1.0.0"),
                sessionId = obj.optString("session_id", ""),
                width = obj.optInt("width", 1920),
                height = obj.optInt("height", 1200),
                fps = obj.optInt("fps", 60),
                selectedCodec = obj.optString("selected_codec", "h264"),
                authRequired = obj.optBoolean("auth_required", false)
            )
        }
    }
}

data class MetricsMessage(
    val fps: Float,
    val bitrateKbps: Int,
    val rttMs: Int,
    val droppedFrames: Int
) {
    companion object {
        fun fromJson(jsonStr: String): MetricsMessage {
            val obj = JSONObject(jsonStr)
            return MetricsMessage(
                fps = obj.optDouble("fps", 0.0).toFloat(),
                bitrateKbps = obj.optInt("bitrate_kbps", 0),
                rttMs = obj.optInt("rtt_ms", 0),
                droppedFrames = obj.optInt("dropped_frames", 0)
            )
        }
    }
}

enum class InputEventType(val value: Byte) {
    TouchDown(0),
    TouchMove(1),
    TouchUp(2),
    TouchCancel(3),
    PointerMotionAbsolute(10),
    PointerButton(11),
    PointerAxis(12)
}

data class InputEvent(
    val eventType: InputEventType,
    val slot: Byte,
    val x: Int, // 0..65535
    val y: Int, // 0..65535
    val codeOrBtn: Int = 0,
    val stateOrFlags: Int = 0
) {
    fun encode(): ByteArray {
        val buf = ByteBuffer.allocate(14)
        buf.order(ByteOrder.LITTLE_ENDIAN)
        buf.put(eventType.value)
        buf.put(slot)
        buf.putShort(x.toShort())
        buf.putShort(y.toShort())
        buf.putInt(codeOrBtn)
        buf.putInt(stateOrFlags)
        return buf.array()
    }
}
