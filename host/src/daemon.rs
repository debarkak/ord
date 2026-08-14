use crate::config::Config;
use crate::session::SessionHandler;
use crate::transport::discovery::DiscoveryBroadcaster;
use crate::transport::tcp::TcpTransportServer;
use anyhow::Result;
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

        info!("ORD Host Ready! Waiting for Android client connection on port {}...", self.config.server.port);

        let handler = Arc::new(SessionHandler::new(self.config.clone(), self.force_test_pattern));

        loop {
            tokio::select! {
                accept_res = listener.accept() => {
                    match accept_res {
                        Ok((stream, addr)) => {
                            info!("Incoming connection from {}", addr);
                            let handler_clone = Arc::clone(&handler);
                            tokio::spawn(async move {
                                if let Err(e) = handler_clone.handle_client(stream).await {
                                    error!("Session error with {}: {:?}", addr, e);
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
