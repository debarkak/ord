use crate::config::Config;
use crate::session::SessionHandler;
use crate::transport::discovery::DiscoveryBroadcaster;
use crate::transport::tcp::TcpTransportServer;
use anyhow::Result;
use rusb::UsbContext;
use std::sync::Arc;
use tokio::signal;
use tracing::{error, info, warn};

pub struct HostDaemon {
    config: Config,
    force_test_pattern: bool,
}

impl HostDaemon {
    pub fn new(config: Config, force_test_pattern: bool) -> Self {
        Self {
            config,
            force_test_pattern,
        }
    }

    pub async fn run(&self) -> Result<()> {
        info!("Starting ORD Host Daemon...");

        // 1. Start UDP Discovery Broadcaster
        let discovery = if self.config.server.enable_discovery {
            let broadcaster = DiscoveryBroadcaster::new(
                self.config.server.discovery_port,
                self.config.server.port,
            );
            if let Err(e) = broadcaster.start() {
                warn!("Could not start UDP discovery: {}", e);
            }
            Some(broadcaster)
        } else {
            None
        };

        // 2. Bind TCP Server
        let tcp_server = TcpTransportServer::new(
            self.config.server.bind_address.clone(),
            self.config.server.port,
        );
        let listener = tcp_server.bind().await?;

        info!("ORD Host Ready! Waiting for Android client connection on port {} or USB AOAP...", self.config.server.port);

        let handler = Arc::new(SessionHandler::new(self.config.clone(), self.force_test_pattern));

        let session_lock = Arc::new(tokio::sync::Mutex::new(()));

        // 3. Spawn background USB AOAP Device Watcher
        let usb_handler = Arc::clone(&handler);
        let usb_lock = Arc::clone(&session_lock);
        tokio::spawn(async move {
            let mut active_usb = false;
            loop {
                if !active_usb {
                    if let Ok(context) = rusb::Context::new() {
                        if let Ok(devices) = context.devices() {
                            for device in devices.iter() {
                                if let Ok(desc) = device.device_descriptor() {
                                    if crate::transport::usb::UsbAoapManager::is_accessory_device(&desc) {
                                        info!("Found Android device in USB AOA mode (0x{:04x}:0x{:04x})", desc.vendor_id(), desc.product_id());
                                        if let Ok(stream) = crate::transport::usb::UsbAoapManager::open_accessory(&device) {
                                            active_usb = true;
                                            let h = Arc::clone(&usb_handler);
                                            let stream_arc = Arc::new(stream);
                                            let s_clone = Arc::clone(&stream_arc);
                                            let lock_clone = Arc::clone(&usb_lock);
                                            tokio::spawn(async move {
                                                let _guard = lock_clone.lock().await;
                                                if let Err(e) = h.handle_usb_client(s_clone).await {
                                                    error!("USB AOAP session error: {:?}", e);
                                                }
                                            });
                                            break;
                                        }
                                    } else {
                                        // Try switching Android devices to AOA mode
                                        let _ = crate::transport::usb::UsbAoapManager::switch_to_accessory(&device);
                                    }
                                }
                            }
                        }
                    }
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
            }
        });

        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, addr)) => {
                            info!("Incoming connection from {}", addr);
                            let handler_clone = Arc::clone(&handler);
                            let lock_clone = Arc::clone(&session_lock);
                            tokio::spawn(async move {
                                if let Ok(_guard) = lock_clone.try_lock() {
                                    if let Err(e) = handler_clone.handle_client(stream).await {
                                        error!("Session error with {}: {:?}", addr, e);
                                    }
                                } else {
                                    warn!("Rejecting connection from {}: another session (USB/TCP) is already active", addr);
                                }
                            });
                        }
                        Err(e) => {
                            error!("TCP accept error: {:?}", e);
                        }
                    }
                }
                _ = signal::ctrl_c() => {
                    info!("Shutdown signal received, shutting down ORD host...");
                    break;
                }
            }
        }

        if let Some(disc) = discovery {
            disc.stop();
        }

        info!("ORD Host Daemon stopped cleanly.");
        Ok(())
    }
}
