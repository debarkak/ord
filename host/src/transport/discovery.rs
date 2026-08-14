use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{debug, error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveryBeacon {
    pub magic: String,
    pub version: u32,
    pub hostname: String,
    pub port: u16,
    pub supported_codecs: Vec<String>,
    pub auth_required: bool,
}

pub struct DiscoveryBroadcaster {
    port: u16,
    tcp_port: u16,
    is_running: Arc<AtomicBool>,
}

impl DiscoveryBroadcaster {
    pub fn new(discovery_port: u16, tcp_port: u16) -> Self {
        Self {
            port: discovery_port,
            tcp_port,
            is_running: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn start(&self) -> Result<()> {
        let is_running = Arc::clone(&self.is_running);
        is_running.store(true, Ordering::SeqCst);
        let discovery_port = self.port;
        let tcp_port = self.tcp_port;

        tokio::spawn(async move {
            let bind_addr: SocketAddr = "0.0.0.0:0".parse().unwrap();
            let socket = match UdpSocket::bind(bind_addr).await {
                Ok(s) => s,
                Err(e) => {
                    error!("Failed to bind UDP discovery socket: {}", e);
                    return;
                }
            };

            if let Err(e) = socket.set_broadcast(true) {
                warn!("Failed to set UDP socket broadcast: {}", e);
            }

            let broadcast_target: SocketAddr = format!("255.255.255.255:{}", discovery_port)
                .parse()
                .unwrap();

            let host_name = hostname::get()
                .map(|h| h.to_string_lossy().to_string())
                .unwrap_or_else(|_| "ORD-Host".to_string());

            info!(
                "UDP Discovery Broadcaster active on port {} (advertising TCP port {})",
                discovery_port, tcp_port
            );

            while is_running.load(Ordering::Relaxed) {
                let beacon = DiscoveryBeacon {
                    magic: "ORD_DISCOVERY".to_string(),
                    version: 1,
                    hostname: host_name.clone(),
                    port: tcp_port,
                    supported_codecs: vec!["h264".to_string()],
                    auth_required: false,
                };

                if let Ok(payload) = serde_json::to_vec(&beacon) {
                    let _ = socket.send_to(&payload, broadcast_target).await;
                }

                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            debug!("UDP Discovery Broadcaster stopped");
        });

        Ok(())
    }

    pub fn stop(&self) {
        self.is_running.store(false, Ordering::SeqCst);
    }
}

impl Drop for DiscoveryBroadcaster {
    fn drop(&mut self) {
        self.stop();
    }
}
