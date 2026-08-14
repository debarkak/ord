package org.ord.client.ui

import android.media.MediaCodecList
import android.os.Build
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.Fragment
import org.ord.client.databinding.FragmentDiagnosticsBinding

class DiagnosticsFragment : Fragment() {

    private var _binding: FragmentDiagnosticsBinding? = null
    private val binding get() = _binding!!

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _binding = FragmentDiagnosticsBinding.inflate(inflater, container, false)
        return binding.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)

        binding.btnDiagBack.setOnClickListener {
            (activity as? MainActivity)?.navigateBack()
        }

        // Populate device information
        binding.tvDeviceModel.text = "Device: ${Build.MANUFACTURER} ${Build.MODEL} (${Build.PRODUCT})"
        binding.tvAndroidVersion.text = "Android OS: Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})"

        val metrics = resources.displayMetrics
        val refreshRate = activity?.display?.mode?.refreshRate ?: 60f
        binding.tvScreenInfo.text = "Display: ${metrics.widthPixels} x ${metrics.heightPixels} @ ${refreshRate.toInt()} Hz (${metrics.densityDpi} DPI)"

        // Enumerate H.264 decoders
        val codecList = MediaCodecList(MediaCodecList.ALL_CODECS)
        val decoders = StringBuilder("Available H.264 Decoders:\n")

        for (info in codecList.codecInfos) {
            if (!info.isEncoder) {
                for (type in info.supportedTypes) {
                    if (type.equals("video/avc", ignoreCase = true)) {
                        val hw = if (info.isHardwareAccelerated) " [Hardware]" else " [Software]"
                        decoders.append(" • ${info.name}$hw\n")
                    }
                }
            }
        }

        binding.tvDecodersList.text = decoders.toString().trim()
    }

    override fun onDestroyView() {
        super.onDestroyView()
        _binding = null
    }
}
