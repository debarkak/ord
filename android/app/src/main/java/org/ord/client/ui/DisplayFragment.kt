package org.ord.client.ui

import android.os.Build
import android.os.Bundle
import android.util.DisplayMetrics
import android.view.*
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.fragment.app.Fragment
import androidx.lifecycle.lifecycleScope
import kotlinx.coroutines.*
import org.ord.client.databinding.FragmentDisplayBinding
import org.ord.client.decoder.EncodedVideoFrame
import org.ord.client.decoder.FrameRingBuffer
import org.ord.client.decoder.H264Decoder
import org.ord.client.input.TouchInputHandler
import org.ord.client.protocol.*
import org.ord.client.transport.TcpClient
import org.ord.client.util.PreferencesHelper

import android.hardware.usb.UsbAccessory
import android.hardware.usb.UsbManager
import androidx.appcompat.app.AppCompatActivity
import org.ord.client.transport.OrdTransport
import org.ord.client.transport.UsbAccessoryTransport

class DisplayFragment : Fragment(), SurfaceHolder.Callback {

    private var _binding: FragmentDisplayBinding? = null
    private val binding get() = _binding!!

    private var isUsb: Boolean = false
    private var hostIp: String = ""
    private var hostPort: Int = 9090

    private var activeTransport: OrdTransport? = null
    private var decoder: H264Decoder? = null
    private val frameBuffer = FrameRingBuffer(maxCapacity = 10)
    private var touchHandler: TouchInputHandler? = null

    private var sessionJob: Job? = null
    private var surfaceReady = false
    private lateinit var prefs: PreferencesHelper

    companion object {
        private const val ARG_IS_USB = "arg_is_usb"
        private const val ARG_IP = "arg_ip"
        private const val ARG_PORT = "arg_port"

        fun newInstance(ip: String, port: Int): DisplayFragment {
            return DisplayFragment().apply {
                arguments = Bundle().apply {
                    putBoolean(ARG_IS_USB, false)
                    putString(ARG_IP, ip)
                    putInt(ARG_PORT, port)
                }
            }
        }

        fun newUsbInstance(): DisplayFragment {
            return DisplayFragment().apply {
                arguments = Bundle().apply {
                    putBoolean(ARG_IS_USB, true)
                }
            }
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        isUsb = arguments?.getBoolean(ARG_IS_USB, false) ?: false
        hostIp = arguments?.getString(ARG_IP) ?: ""
        hostPort = arguments?.getInt(ARG_PORT) ?: 9090
    }

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _binding = FragmentDisplayBinding.inflate(inflater, container, false)
        return binding.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        prefs = PreferencesHelper(requireContext())

        if (prefs.keepScreenOn) {
            activity?.window?.addFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        }

        enterImmersiveMode()

        binding.surfaceView.holder.addCallback(this)

        binding.btnDisconnect.setOnClickListener {
            disconnectAndExit()
        }

        binding.btnHudToggle.setOnClickListener {
            val visible = binding.llHudOverlay.visibility == View.VISIBLE
            binding.llHudOverlay.visibility = if (visible) View.GONE else View.VISIBLE
            prefs.showHud = !visible
        }

        if (prefs.showHud) {
            binding.llHudOverlay.visibility = View.VISIBLE
        }
    }

    private fun enterImmersiveMode() {
        activity?.window?.let { window ->
            WindowCompat.setDecorFitsSystemWindows(window, false)
            WindowInsetsControllerCompat(window, binding.root).let { controller ->
                controller.hide(WindowInsetsCompat.Type.systemBars())
                controller.systemBarsBehavior =
                    WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            }
        }
    }

    override fun surfaceCreated(holder: SurfaceHolder) {
        surfaceReady = true
        startDisplaySession(holder.surface)
    }

    override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {}

    override fun surfaceDestroyed(holder: SurfaceHolder) {
        surfaceReady = false
        stopDisplaySession()
    }

    private fun startDisplaySession(surface: Surface) {
        sessionJob = viewLifecycleOwner.lifecycleScope.launch(Dispatchers.IO) {
            try {
                val transport: OrdTransport = if (isUsb) {
                    withContext(Dispatchers.Main) {
                        binding.tvDisplayStatus.text = "Connecting via USB AOAP..."
                    }
                    val usbManager = requireContext().getSystemService(AppCompatActivity.USB_SERVICE) as UsbManager
                    val accessory = usbManager.accessoryList?.firstOrNull()
                        ?: throw IllegalStateException("No USB Accessory attached")
                    val usbTransport = UsbAccessoryTransport(usbManager, accessory)
                    if (!usbTransport.open()) {
                        throw IllegalStateException("Failed to open USB Accessory")
                    }
                    usbTransport
                } else {
                    withContext(Dispatchers.Main) {
                        binding.tvDisplayStatus.text = "Connecting to $hostIp:$hostPort..."
                    }
                    val client = TcpClient(hostIp, hostPort)
                    client.connect(5000)
                    client
                }
                activeTransport = transport

                // Handshake: Send HELLO
                val displayMetrics = resources.displayMetrics
                val screenW = if (prefs.preferredWidth > 0) prefs.preferredWidth else displayMetrics.widthPixels
                val screenH = if (prefs.preferredHeight > 0) prefs.preferredHeight else displayMetrics.heightPixels

                val hello = HelloMessage(
                    clientName = "${Build.MANUFACTURER} ${Build.MODEL}",
                    clientVersion = "1.0.0",
                    screenWidth = screenW,
                    screenHeight = screenH,
                    densityDpi = displayMetrics.densityDpi,
                    maxFps = 90,
                    supportedCodecs = listOf("h264")
                )

                transport.sendRaw(
                    msgType = OrdConstants.MSG_HELLO,
                    flags = 0,
                    sequence = 0,
                    payload = hello.toJson().toByteArray(Charsets.UTF_8)
                )

                // Await HELLO_ACK
                val ackPacket = transport.readPacket()
                if (ackPacket.header.msgType != OrdConstants.MSG_HELLO_ACK) {
                    throw IllegalStateException("Expected HELLO_ACK, got ${ackPacket.header.msgType}")
                }
                val helloAck = HelloAckMessage.fromJson(String(ackPacket.payload, Charsets.UTF_8))

                // Initialize Decoder
                val dec = H264Decoder(surface, helloAck.width, helloAck.height, frameBuffer)
                decoder = dec
                dec.start(viewLifecycleOwner.lifecycleScope)

                // Initialize Input
                touchHandler = TouchInputHandler(transport, viewLifecycleOwner.lifecycleScope)
                withContext(Dispatchers.Main) {
                    binding.surfaceView.setOnTouchListener(touchHandler)
                    binding.llConnectingState.visibility = View.GONE
                }

                // Packet Read Loop
                while (isActive) {
                    val packet = transport.readPacket()
                    when (packet.header.msgType) {
                        OrdConstants.MSG_VIDEO_DATA -> {
                            val frame = EncodedVideoFrame(
                                sequence = packet.header.sequence,
                                flags = packet.header.flags,
                                data = packet.payload
                            )
                            frameBuffer.offer(frame)
                        }
                        OrdConstants.MSG_METRICS -> {
                            val metrics = MetricsMessage.fromJson(String(packet.payload, Charsets.UTF_8))
                            withContext(Dispatchers.Main) {
                                binding.tvHudFps.text = String.format("FPS: %.1f", metrics.fps)
                                binding.tvHudBitrate.text = "Bitrate: ${metrics.bitrateKbps} kbps"
                            }
                        }
                        OrdConstants.MSG_PING -> {
                            transport.sendRaw(
                                msgType = OrdConstants.MSG_PONG,
                                flags = 0,
                                sequence = packet.header.sequence,
                                payload = packet.payload
                            )
                        }
                        OrdConstants.MSG_DISCONNECT -> {
                            break
                        }
                    }
                }
            } catch (e: Exception) {
                e.printStackTrace()
            } finally {
                withContext(Dispatchers.Main) {
                    disconnectAndExit()
                }
            }
        }
    }

    private fun stopDisplaySession() {
        sessionJob?.cancel()
        sessionJob = null
        decoder?.stop()
        decoder = null
        activeTransport?.close()
        activeTransport = null
    }

    private fun disconnectAndExit() {
        stopDisplaySession()
        (activity as? MainActivity)?.navigateBack()
    }

    override fun onDestroyView() {
        super.onDestroyView()
        activity?.window?.clearFlags(WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON)
        stopDisplaySession()
        _binding = null
    }
}
