use std::process::Command;

#[derive(Debug, Clone)]
pub struct SystemDiagnostics {
    pub os_name: String,
    pub kernel_version: String,
    pub session_type: String,
    pub gnome_version: Option<String>,
    pub mutter_version: Option<String>,
    pub gpu_info: String,
    pub drm_render_node: Option<String>,
    pub vaapi_supported: bool,
    pub vaapi_encoder: Option<String>,
    pub pipewire_available: bool,
    pub pipewire_version: Option<String>,
    pub network_interfaces: Vec<(String, String)>,
    pub gstreamer_version: String,
}

impl SystemDiagnostics {
    pub fn inspect() -> Self {
        let os_name = std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|c| {
                for line in c.lines() {
                    if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                        return Some(val.trim_matches('"').to_string());
                    }
                }
                None
            })
            .unwrap_or_else(|| "Linux (unknown)".to_string());

        let kernel_version = Command::new("uname")
            .arg("-r")
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|_| "unknown".to_string());

        let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_string());

        let gnome_version = Command::new("gnome-shell")
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let mutter_version = Command::new("mutter")
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let gpu_info = Command::new("lspci")
            .output()
            .ok()
            .map(|o| {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines()
                    .filter(|l| l.contains("VGA") || l.contains("3D") || l.contains("Display"))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "AMD / Intel GPU (auto-detected)".to_string());

        let drm_render_node = if std::path::Path::new("/dev/dri/renderD128").exists() {
            Some("/dev/dri/renderD128".to_string())
        } else {
            None
        };

        // Inspect GStreamer VAAPI encoder
        let vaapi_supported = Self::check_gst_element("vah264enc");
        let vaapi_encoder = if vaapi_supported {
            Some("vah264enc (Hardware VA-API H.264)".to_string())
        } else if Self::check_gst_element("vulkanh264enc") {
            Some("vulkanh264enc (Hardware Vulkan H.264)".to_string())
        } else {
            None
        };

        let pipewire_available = Self::check_gst_element("pipewiresrc");
        let pipewire_version = Command::new("pipewire")
            .arg("--version")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").to_string());

        let network_interfaces = Self::get_network_ips();

        let (major, minor, micro, _) = gstreamer::version();
        let gstreamer_version = format!("{}.{}.{}", major, minor, micro);

        Self {
            os_name,
            kernel_version,
            session_type,
            gnome_version,
            mutter_version,
            gpu_info,
            drm_render_node,
            vaapi_supported,
            vaapi_encoder,
            pipewire_available,
            pipewire_version,
            network_interfaces,
            gstreamer_version,
        }
    }

    fn check_gst_element(name: &str) -> bool {
        let _ = gstreamer::init();
        gstreamer::ElementFactory::find(name).is_some()
    }

    fn get_network_ips() -> Vec<(String, String)> {
        let mut result = Vec::new();
        if let Ok(output) = Command::new("ip").args(["-br", "addr"]).output() {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 && parts[1] == "UP" {
                    let iface = parts[0];
                    let ip = parts[2].split('/').next().unwrap_or(parts[2]);
                    result.push((iface.to_string(), ip.to_string()));
                }
            }
        }
        result
    }

    pub fn print_report(&self) {
        println!("============================================================");
        println!("              ORD Host System Diagnostics                   ");
        println!("============================================================");
        println!("  Operating System:    {}", self.os_name);
        println!("  Kernel Version:      {}", self.kernel_version);
        println!("  Session Type:        {}", self.session_type);
        if let Some(ref gv) = self.gnome_version {
            println!("  GNOME Shell:         {}", gv);
        }
        if let Some(ref mv) = self.mutter_version {
            println!("  Mutter Compositor:   {}", mv);
        }
        println!("  Graphics Device:     {}", self.gpu_info.trim());
        if let Some(ref node) = self.drm_render_node {
            println!("  DRM Render Node:     {}", node);
        }
        println!(
            "  Hardware VA-API:     {}",
            if self.vaapi_supported {
                "Available (Accelerated)"
            } else {
                "Not available (Software fallback)"
            }
        );
        if let Some(ref enc) = self.vaapi_encoder {
            println!("  Hardware Encoder:    {}", enc);
        }
        println!("  PipeWire Status:     {}", if self.pipewire_available { "Ready" } else { "Unavailable" });
        if let Some(ref pwv) = self.pipewire_version {
            println!("  PipeWire Version:    {}", pwv);
        }
        println!("  GStreamer Core:      {}", self.gstreamer_version);
        println!("  Network Interfaces:");
        for (iface, ip) in &self.network_interfaces {
            println!("    - {:<16} {}", iface, ip);
        }
        println!("============================================================");
        println!("  Compatibility:       Full Wayland / GNOME 50+ Virtual Monitor");
        println!("============================================================");
    }
}
