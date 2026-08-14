package org.ord.client.ui

import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import androidx.fragment.app.Fragment
import org.ord.client.R
import org.ord.client.databinding.FragmentSettingsBinding
import org.ord.client.util.PreferencesHelper

class SettingsFragment : Fragment() {

    private var _binding: FragmentSettingsBinding? = null
    private val binding get() = _binding!!
    private lateinit var prefs: PreferencesHelper

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _binding = FragmentSettingsBinding.inflate(inflater, container, false)
        return binding.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        prefs = PreferencesHelper(requireContext())

        // Load saved state
        when {
            prefs.preferredWidth == 1920 && prefs.preferredHeight == 1200 -> binding.rbRes1200p.isChecked = true
            prefs.preferredWidth == 1920 && prefs.preferredHeight == 1080 -> binding.rbRes1080p.isChecked = true
            prefs.preferredWidth == 1280 && prefs.preferredHeight == 720 -> binding.rbRes720p.isChecked = true
            else -> binding.rbResNative.isChecked = true
        }

        binding.switchKeepScreenOn.isChecked = prefs.keepScreenOn
        binding.switchShowHud.isChecked = prefs.showHud

        binding.btnBack.setOnClickListener {
            (activity as? MainActivity)?.navigateBack()
        }

        binding.rgResolution.setOnCheckedChangeListener { _, checkedId ->
            when (checkedId) {
                R.id.rb_res_1200p -> {
                    prefs.preferredWidth = 1920
                    prefs.preferredHeight = 1200
                }
                R.id.rb_res_1080p -> {
                    prefs.preferredWidth = 1920
                    prefs.preferredHeight = 1080
                }
                R.id.rb_res_720p -> {
                    prefs.preferredWidth = 1280
                    prefs.preferredHeight = 720
                }
                else -> {
                    prefs.preferredWidth = 0
                    prefs.preferredHeight = 0
                }
            }
        }

        binding.switchKeepScreenOn.setOnCheckedChangeListener { _, isChecked ->
            prefs.keepScreenOn = isChecked
        }

        binding.switchShowHud.setOnCheckedChangeListener { _, isChecked ->
            prefs.showHud = isChecked
        }
    }

    override fun onDestroyView() {
        super.onDestroyView()
        _binding = null
    }
}
