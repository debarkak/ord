package org.ord.client.discovery

import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import org.json.JSONObject
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress

data class DiscoveredHost(
    val hostname: String,
    val ip: String,
    val port: Int,
    val codecs: List<String>,
    val lastSeenMs: Long = System.currentTimeMillis()
)

class HostDiscoveryManager(
    private val port: Int = 9091
) {
    private val _hosts = MutableStateFlow<List<DiscoveredHost>>(emptyList())
    val hosts: StateFlow<List<DiscoveredHost>> = _hosts.asStateFlow()

    private var socket: DatagramSocket? = null
    private var job: Job? = null

    fun start(scope: CoroutineScope) {
        if (job?.isActive == true) return

        job = scope.launch(Dispatchers.IO) {
            try {
                socket = DatagramSocket(null).apply {
                    reuseAddress = true
                    bind(InetSocketAddress(port))
                    broadcast = true
                }

                val buffer = ByteArray(4096)
                val packet = DatagramPacket(buffer, buffer.size)

                while (isActive) {
                    try {
                        socket?.receive(packet)
                        val text = String(packet.data, 0, packet.length)
                        val obj = JSONObject(text)
                        if (obj.optString("magic") == "ORD_DISCOVERY") {
                            val hostname = obj.optString("hostname", "ORD-Host")
                            val tcpPort = obj.optInt("port", 9090)
                            val hostIp = packet.address.hostAddress ?: continue

                            val codecsList = mutableListOf<String>()
                            val codecsArr = obj.optJSONArray("supported_codecs")
                            if (codecsArr != null) {
                                for (i in 0 until codecsArr.length()) {
                                    codecsList.add(codecsArr.getString(i))
                                }
                            }

                            val discovered = DiscoveredHost(
                                hostname = hostname,
                                ip = hostIp,
                                port = tcpPort,
                                codecs = codecsList
                            )

                            updateHost(discovered)
                        }
                    } catch (e: Exception) {
                        if (!isActive) break
                    }
                }
            } catch (e: Exception) {
                e.printStackTrace()
            }
        }
    }

    private fun updateHost(host: DiscoveredHost) {
        val current = _hosts.value.toMutableList()
        val index = current.indexOfFirst { it.ip == host.ip && it.port == host.port }
        if (index >= 0) {
            current[index] = host
        } else {
            current.add(host)
        }
        _hosts.value = current
    }

    fun stop() {
        job?.cancel()
        socket?.close()
        socket = null
        job = null
    }
}
