package org.ord.client.ui

import android.app.AlertDialog
import android.os.Bundle
import android.view.LayoutInflater
import android.view.View
import android.view.ViewGroup
import android.widget.EditText
import androidx.fragment.app.Fragment
import androidx.lifecycle.lifecycleScope
import androidx.recyclerview.widget.LinearLayoutManager
import kotlinx.coroutines.flow.collectLatest
import kotlinx.coroutines.launch
import org.ord.client.R
import org.ord.client.databinding.FragmentDiscoveryBinding
import org.ord.client.discovery.DiscoveredHost
import org.ord.client.discovery.HostDiscoveryManager
import org.ord.client.util.PreferencesHelper

class DiscoveryFragment : Fragment() {

    private var _binding: FragmentDiscoveryBinding? = null
    private val binding get() = _binding!!

    private lateinit var discoveryManager: HostDiscoveryManager
    private lateinit var hostAdapter: HostAdapter
    private lateinit var prefs: PreferencesHelper

    override fun onCreateView(
        inflater: LayoutInflater,
        container: ViewGroup?,
        savedInstanceState: Bundle?
    ): View {
        _binding = FragmentDiscoveryBinding.inflate(inflater, container, false)
        return binding.root
    }

    override fun onViewCreated(view: View, savedInstanceState: Bundle?) {
        super.onViewCreated(view, savedInstanceState)
        prefs = PreferencesHelper(requireContext())
        discoveryManager = HostDiscoveryManager()

        hostAdapter = HostAdapter { host ->
            connectToHost(host.ip, host.port)
        }

        binding.rvHosts.apply {
            layoutManager = LinearLayoutManager(requireContext())
            adapter = hostAdapter
        }

        binding.btnUsbConnect.setOnClickListener {
            (activity as? MainActivity)?.navigateTo(DisplayFragment.newUsbInstance())
        }

        binding.btnManualConnect.setOnClickListener {
            showManualConnectDialog()
        }

        binding.btnSettings.setOnClickListener {
            (activity as? MainActivity)?.navigateTo(SettingsFragment())
        }

        binding.btnDiagnostics.setOnClickListener {
            (activity as? MainActivity)?.navigateTo(DiagnosticsFragment())
        }

        viewLifecycleOwner.lifecycleScope.launch {
            discoveryManager.hosts.collectLatest { list ->
                hostAdapter.submitList(list)
                binding.llEmptyState.visibility = if (list.isEmpty()) View.VISIBLE else View.GONE
            }
        }
    }

    override fun onResume() {
        super.onResume()
        discoveryManager.start(viewLifecycleOwner.lifecycleScope)
    }

    override fun onPause() {
        super.onPause()
        discoveryManager.stop()
    }

    private fun showManualConnectDialog() {
        val input = EditText(requireContext()).apply {
            hint = "192.168.1.100:9090"
            setText(prefs.lastHostIp)
        }

        AlertDialog.Builder(requireContext(), R.style.Theme_ORD)
            .setTitle("Connect to Linux Host")
            .setMessage("Enter the IP address and port of your ORD host:")
            .setView(input)
            .setPositiveButton("Connect") { _, _ ->
                val text = input.text.toString().trim()
                if (text.isNotEmpty()) {
                    prefs.lastHostIp = text
                    val parts = text.split(":")
                    val ip = parts[0]
                    val port = if (parts.size > 1) parts[1].toIntOrNull() ?: 9090 else 9090
                    connectToHost(ip, port)
                }
            }
            .setNegativeButton("Cancel", null)
            .show()
    }

    private fun connectToHost(ip: String, port: Int) {
        val fragment = DisplayFragment.newInstance(ip, port)
        (activity as? MainActivity)?.navigateTo(fragment)
    }

    override fun onDestroyView() {
        super.onDestroyView()
        _binding = null
    }
}
