mod config;
mod daemon;
mod diagnostics;
mod display;
mod encoder;
mod input;
mod protocol;
mod session;
mod transport;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::Config;
use daemon::HostDaemon;
use diagnostics::SystemDiagnostics;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[derive(Parser)]
#[command(name = "ord")]
#[command(author = "ORD Contributors")]
#[command(version = "0.1.0")]
#[command(about = "OpenRemoteDisplay - Linux Virtual Display Host & CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to custom config file
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,

    /// Enable verbose / debug logging
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the ORD host daemon
    Start {
        /// TCP port to listen on
        #[arg(short, long, default_value_t = 9090)]
        port: u16,

        /// Bind IP address
        #[arg(short, long, default_value = "0.0.0.0")]
        bind: String,

        /// Force synthetic test pattern instead of GNOME virtual monitor
        #[arg(long)]
        test_pattern: bool,

        /// Video encoder choice (auto, vaapi, software)
        #[arg(long, default_value = "auto")]
        encoder: String,

        /// Stream bitrate in kbps
        #[arg(long, default_value_t = 15000)]
        bitrate: u32,
    },

    /// Run host environment diagnostics and check hardware acceleration
    Diagnostics,

    /// Stream synthetic test pattern for client decoder testing and latency measurement
    TestPattern {
        /// TCP port to listen on
        #[arg(short, long, default_value_t = 9090)]
        port: u16,

        /// Stream bitrate in kbps
        #[arg(long, default_value_t = 10000)]
        bitrate: u32,
    },

    /// Discover other ORD hosts on the local network
    Hosts {
        /// Timeout in seconds to listen for discovery beacons
        #[arg(short, long, default_value_t = 3)]
        timeout: u64,
    },

    /// Generate or reset default configuration file in ~/.config/ord/config.toml
    InitConfig,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize structured logging
    let filter = if cli.verbose {
        EnvFilter::new("ord=debug,info")
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("ord=info"))
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();

    match cli.command.unwrap_or(Commands::Start {
        port: 9090,
        bind: "0.0.0.0".to_string(),
        test_pattern: false,
        encoder: "auto".to_string(),
        bitrate: 15000,
    }) {
        Commands::Start {
            port,
            bind,
            test_pattern,
            encoder,
            bitrate,
        } => {
            let mut config = Config::load_or_default();
            config.server.port = port;
            config.server.bind_address = bind;
            config.stream.encoder = encoder;
            config.stream.bitrate_kbps = bitrate;

            let daemon = HostDaemon::new(config, test_pattern);
            daemon.run().await?;
        }

        Commands::Diagnostics => {
            let diag = SystemDiagnostics::inspect();
            diag.print_report();
        }

        Commands::TestPattern { port, bitrate } => {
            let mut config = Config::load_or_default();
            config.server.port = port;
            config.stream.bitrate_kbps = bitrate;

            info!("Starting ORD in Test Pattern mode on port {}...", port);
            let daemon = HostDaemon::new(config, true);
            daemon.run().await?;
        }

        Commands::Hosts { timeout } => {
            info!("Listening for ORD hosts on LAN (timeout: {}s)...", timeout);
            let socket = tokio::net::UdpSocket::bind("0.0.0.0:9091").await?;
            let mut buf = [0u8; 4096];

            let start = std::time::Instant::now();
            let mut found_hosts = std::collections::HashSet::new();

            while start.elapsed() < std::time::Duration::from_secs(timeout) {
                if let Ok(Ok((n, addr))) = tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    socket.recv_from(&mut buf),
                )
                .await
                {
                    if let Ok(beacon) = serde_json::from_slice::<transport::discovery::DiscoveryBeacon>(&buf[..n]) {
                        if beacon.magic == "ORD_DISCOVERY" && found_hosts.insert(format!("{}:{}", addr.ip(), beacon.port)) {
                            println!(
                                "Found ORD Host: '{}' at {}:{} (Codecs: {:?})",
                                beacon.hostname, addr.ip(), beacon.port, beacon.supported_codecs
                            );
                        }
                    }
                }
            }

            if found_hosts.is_empty() {
                println!("No ORD hosts detected on LAN within timeout.");
            }
        }

        Commands::InitConfig => {
            let config = Config::default();
            config.save()?;
            println!("Default configuration written to: {:?}", Config::config_path());
        }
    }

    Ok(())
}
