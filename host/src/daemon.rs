use crate::config::Config;
use crate::session::SessionHandler;
use crate::transport::discovery::DiscoveryBroadcaster;
use crate::transport::tcp::TcpTransportServer;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
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

        info!(
            "ORD Host Ready! Waiting for Android client on port {} or USB AOAP...",
            self.config.server.port
        );

        let handler = Arc::new(SessionHandler::new(self.config.clone(), self.force_test_pattern));

        // Single-session guard: only one active session at a time
        let is_session_active = Arc::new(AtomicBool::new(false));

        // 3. Spawn background USB AOAP watcher with proper state machine
        let usb_handler = Arc::clone(&handler);
        let usb_session_flag = Arc::clone(&is_session_active);
        tokio::spawn(async move {
            // State machine:
            //   Idle -> Switching (when Android candidate found) -> Waiting (for re-enum) -> Connected -> Idle
            enum UsbState {
                Idle,
                WaitingForAccessory { waited_ms: u32 },
                Connected,
            }

            let mut state = UsbState::Idle;
            let mut switch_cooldown_ms = 0u32;

            loop {
                match state {
                    UsbState::Idle => {
                        if usb_session_flag.load(Ordering::Relaxed) {
                            // TCP session is active, don't interfere
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            continue;
                        }

                        if switch_cooldown_ms > 0 {
                            switch_cooldown_ms = switch_cooldown_ms.saturating_sub(500);
                            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                            continue;
                        }

                        // Check if device is already in accessory mode
                        if let Some(stream) = tokio::task::spawn_blocking(
                            crate::transport::usb::UsbAoapManager::find_and_open_accessory
                        ).await.unwrap_or(None) {
                            info!("Found Android device already in USB AOA accessory mode — connecting");
                            usb_session_flag.store(true, Ordering::Relaxed);
                            let h = Arc::clone(&usb_handler);
                            let flag = Arc::clone(&usb_session_flag);
                            let stream_arc = Arc::new(stream);
                            let s_clone = Arc::clone(&stream_arc);
                            tokio::spawn(async move {
                                if let Err(e) = h.handle_usb_client(s_clone).await {
                                    error!("USB AOAP session ended: {:?}", e);
                                }
                                flag.store(false, Ordering::Relaxed);
                                info!("USB AOAP session closed — ready for new connection");
                            });
                            state = UsbState::Connected;
                        } else {
                            // Try to switch any Android device to AOA mode
                            let switched = tokio::task::spawn_blocking(
                                crate::transport::usb::UsbAoapManager::scan_and_switch_any
                            ).await.unwrap_or(false);

                            if switched {
                                info!("AOA switch initiated — waiting for device to re-enumerate...");
                                state = UsbState::WaitingForAccessory { waited_ms: 0 };
                            } else {
                                // No candidate found, sleep and try again
                                tokio::time::sleep(tokio::time::Duration::from_millis(1500)).await;
                            }
                        }
                    }

                    UsbState::WaitingForAccessory { ref mut waited_ms } => {
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                        *waited_ms += 500;

                        // Check if re-enumeration completed
                        let found = tokio::task::spawn_blocking(
                            crate::transport::usb::UsbAoapManager::find_and_open_accessory
                        ).await.unwrap_or(None);

                        if let Some(stream) = found {
                            info!("Device re-enumerated in AOA mode after {}ms — connecting", waited_ms);
                            usb_session_flag.store(true, Ordering::Relaxed);
                            let h = Arc::clone(&usb_handler);
                            let flag = Arc::clone(&usb_session_flag);
                            let stream_arc = Arc::new(stream);
                            let s_clone = Arc::clone(&stream_arc);
                            tokio::spawn(async move {
                                if let Err(e) = h.handle_usb_client(s_clone).await {
                                    error!("USB AOAP session ended: {:?}", e);
                                }
                                flag.store(false, Ordering::Relaxed);
                                info!("USB AOAP session closed — ready for new connection");
                            });
                            state = UsbState::Connected;
                        } else if *waited_ms > 8000 {
                            warn!("Timed out waiting for AOA re-enumeration after 8s — retrying");
                            state = UsbState::Idle;
                            switch_cooldown_ms = 5000; // 5s cooldown before next switch attempt
                        }
                    }

                    UsbState::Connected => {
                        // Wait until session ends
                        if !usb_session_flag.load(Ordering::Relaxed) {
                            state = UsbState::Idle;
                            // Device re-plugged or app closed; wait a moment before trying again
                            switch_cooldown_ms = 2000;
                        }
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    }
                }
            }
        });

        // 4. TCP accept loop
        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, addr)) => {
                            if is_session_active.load(Ordering::Relaxed) {
                                warn!("Rejecting TCP connection from {}: a session is already active", addr);
                                // drop stream to close connection
                                drop(stream);
                                continue;
                            }
                            is_session_active.store(true, Ordering::Relaxed);
                            info!("Incoming TCP connection from {}", addr);
                            let handler_clone = Arc::clone(&handler);
                            let active_flag = Arc::clone(&is_session_active);
                            tokio::spawn(async move {
                                if let Err(e) = handler_clone.handle_client(stream).await {
                                    error!("TCP session error with {}: {:?}", addr, e);
                                }
                                active_flag.store(false, Ordering::Relaxed);
                                info!("TCP session with {} closed", addr);
                            });
                        }
                        Err(e) => {
                            error!("TCP accept error: {:?}", e);
                        }
                    }
                }
                _ = signal::ctrl_c() => {
                    info!("Shutdown signal received, stopping ORD host...");
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
