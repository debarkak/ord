package org.ord.client.ui

import android.view.LayoutInflater
import android.view.ViewGroup
import androidx.recyclerview.widget.RecyclerView
import org.ord.client.databinding.ItemHostBinding
import org.ord.client.discovery.DiscoveredHost

class HostAdapter(
    private val onConnectClick: (DiscoveredHost) -> Unit
) : RecyclerView.Adapter<HostAdapter.HostViewHolder>() {

    private var items: List<DiscoveredHost> = emptyList()

    fun submitList(newItems: List<DiscoveredHost>) {
        items = newItems
        notifyDataSetChanged()
    }

    override fun onCreateViewHolder(parent: ViewGroup, viewType: Int): HostViewHolder {
        val binding = ItemHostBinding.inflate(LayoutInflater.from(parent.context), parent, false)
        return HostViewHolder(binding)
    }

    override fun onBindViewHolder(holder: HostViewHolder, position: Int) {
        holder.bind(items[position])
    }

    override fun getItemCount(): Int = items.size

    inner class HostViewHolder(private val binding: ItemHostBinding) : RecyclerView.ViewHolder(binding.root) {
        fun bind(host: DiscoveredHost) {
            binding.tvHostName.text = host.hostname
            binding.tvHostIp.text = "${host.ip}:${host.port}"
            binding.tvHostCaps.text = "Codecs: ${host.codecs.joinToString(", ")}"

            binding.btnConnect.setOnClickListener {
                onConnectClick(host)
            }
        }
    }
}
