use crate::config::Config;
use crate::display::mutter::MutterDisplayBackend;
use crate::display::test_source::TestSourceDisplayBackend;
use crate::display::{DisplayBackend, VirtualDisplayInfo};
use crate::encoder::{EncoderConfig, VideoFrame, VideoPipeline};
use crate::input::mutter_input::MutterInputBackend;
use crate::input::uinput_backend::UInputBackend;
use crate::input::InputBackend;
use crate::protocol::packet::OrdPacket;
use crate::protocol::types::*;
use crate::transport::tcp::FramedStream;
use anyhow::{anyhow, Context, Result};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

pub struct SessionHandler {
    config: Config,
    force_test_pattern: bool,
}

impl SessionHandler {
    pub fn new(config: Config, force_test_pattern: bool) -> Self {
        Self {
            config,
            force_test_pattern,
        }
    }

    pub async fn handle_client(&self, stream: TcpStream) -> Result<()> {
        let mut framed = FramedStream::new(stream)?;
        let peer = framed.peer_addr()?;
        info!("Accepted connection from {}", peer);

        // 1. Handshake: Wait for HELLO
        let hello_packet = framed.read_packet().await?;
        if hello_packet.header.msg_type != MSG_HELLO {
            return Err(anyhow!("Expected MSG_HELLO, got {}", hello_packet.header.msg_type));
        }

        let hello: HelloMessage = serde_json::from_slice(&hello_packet.payload)
            .context("Failed to deserialize HelloMessage")?;

        info!(
            "Client connected: '{}' (v{}) Screen: {}x{} @ {}fps, DPI: {}",
            hello.client_name, hello.client_version, hello.screen_width, hello.screen_height, hello.max_fps, hello.density_dpi
        );

        // Determine display parameters
        let width = if hello.screen_width > 0 { hello.screen_width } else { self.config.display.default_width };
        let height = if hello.screen_height > 0 { hello.screen_height } else { self.config.display.default_height };
        let fps = if hello.max_fps > 0 { hello.max_fps.min(120) } else { self.config.display.default_fps };

        // 2. Initialize Display Backend
        let mut display_backend: Box<dyn DisplayBackend> = if self.force_test_pattern {
            Box::new(TestSourceDisplayBackend::new())
        } else {
            // Try Mutter backend first, fallback to TestSource if running outside GNOME
            match MutterDisplayBackend::new().create_virtual_display(width, height, fps).await {
                Ok(info) => {
                    info!("Successfully created GNOME Mutter Virtual Display: {:?}", info);
                    // Wrap existing backend
                    struct ActiveMutter(MutterDisplayBackend, VirtualDisplayInfo);
                    #[async_trait::async_trait]
                    impl DisplayBackend for ActiveMutter {
                        async fn create_virtual_display(&mut self, _: u32, _: u32, _: u32) -> Result<VirtualDisplayInfo> {
                            Ok(self.1.clone())
                        }
                        async fn destroy_virtual_display(&mut self) -> Result<()> {
                            self.0.destroy_virtual_display().await
                        }
                    }
                    Box::new(ActiveMutter(MutterDisplayBackend::new(), info))
                }
                Err(e) => {
                    warn!("Failed to create Mutter virtual display ({}). Falling back to test pattern source.", e);
                    Box::new(TestSourceDisplayBackend::new())
                }
            }
        };

        let display_info = display_backend.create_virtual_display(width, height, fps).await?;

        // 3. Send HELLO_ACK
        let host_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "ORD-Host".to_string());

        let hello_ack = HelloAckMessage {
            server_name: host_name,
            server_version: "1.0.0".to_string(),
            session_id: format!("ord-{}", std::process::id()),
            width: display_info.width,
            height: display_info.height,
            fps: display_info.fps,
            selected_codec: "h264".to_string(),
            auth_required: false,
        };

        let ack_packet = OrdPacket::new(MSG_HELLO_ACK, 0, 0, serde_json::to_vec(&hello_ack)?);
        framed.write_packet(&ack_packet).await?;

        // 4. Send DISPLAY_CONFIG
        let display_config = DisplayConfigMessage {
            width: display_info.width,
            height: display_info.height,
            refresh_rate: display_info.fps,
            orientation: 0,
            scale_factor: self.config.display.scale_factor,
        };
        let cfg_packet = OrdPacket::new(MSG_DISPLAY_CONFIG, 0, 1, serde_json::to_vec(&display_config)?);
        framed.write_packet(&cfg_packet).await?;

        // 5. Initialize Input Backend
        let mut input_backend: Box<dyn InputBackend> = if display_info.pipewire_node_id > 0 && !display_info.stream_path.is_empty() {
            match MutterInputBackend::new(&display_info.stream_path, display_info.width, display_info.height).await {
                Ok(backend) => Box::new(backend),
                Err(e) => {
                    warn!("Failed to initialize Mutter RemoteDesktop input: {}. Using uinput fallback.", e);
                    Box::new(UInputBackend::new())
                }
            }
        } else {
            Box::new(UInputBackend::new())
        };

        // 6. Start Video Encoder Pipeline
        let (frame_tx, mut frame_rx) = mpsc::channel::<VideoFrame>(30);
        let encoder_config = EncoderConfig {
            width: display_info.width,
            height: display_info.height,
            fps: display_info.fps,
            bitrate_kbps: self.config.stream.bitrate_kbps,
            encoder_choice: self.config.stream.encoder.clone(),
            keyframe_interval_frames: self.config.stream.keyframe_interval_frames,
        };

        let pipeline = VideoPipeline::new(display_info.pipewire_node_id, &encoder_config, frame_tx)?;
        pipeline.start()?;

        info!("Display and video streaming session established with {}", peer);

        // 7. Run Bidirectional Session Loop
        let session_result = Self::run_session_loop(&mut framed, &mut frame_rx, &mut *input_backend).await;

        // 8. Clean Shutdown
        info!("Closing session for {}, cleaning up display...", peer);
        let _ = pipeline.stop();
        let _ = display_backend.destroy_virtual_display().await;

        session_result
    }

    async fn run_session_loop(
        framed: &mut FramedStream,
        frame_rx: &mut mpsc::Receiver<VideoFrame>,
        input_backend: &mut dyn InputBackend,
    ) -> Result<()> {
        let mut frame_seq: u32 = 0;
        let mut last_metrics_time = Instant::now();
        let mut frames_sent_since_metric = 0;
        let mut bytes_sent_since_metric = 0;

        loop {
            tokio::select! {
                // Incoming video frame from GStreamer encoder
                Some(frame) = frame_rx.recv() => {
                    frame_seq = frame_seq.wrapping_add(1);
                    let mut flags = 0u16;
                    if frame.is_keyframe {
                        flags |= FLAG_KEYFRAME;
                    }
                    flags |= FLAG_END_OF_FRAME;

                    bytes_sent_since_metric += frame.data.len();
                    frames_sent_since_metric += 1;

                    framed.write_frame_data(frame_seq, flags, &frame.data).await?;

                    // Periodically send metrics
                    if last_metrics_time.elapsed() >= Duration::from_secs(1) {
                        let elapsed_sec = last_metrics_time.elapsed().as_secs_f32();
                        let fps = frames_sent_since_metric as f32 / elapsed_sec;
                        let bitrate_kbps = ((bytes_sent_since_metric as f32 * 8.0) / (elapsed_sec * 1000.0)) as u32;

                        let metrics = MetricsMessage {
                            fps,
                            bitrate_kbps,
                            rtt_ms: 0,
                            dropped_frames: 0,
                            encoder_latency_us: 0,
                        };

                        if let Ok(payload) = serde_json::to_vec(&metrics) {
                            let packet = OrdPacket::new(MSG_METRICS, 0, frame_seq, payload);
                            let _ = framed.write_packet(&packet).await;
                        }

                        last_metrics_time = Instant::now();
                        frames_sent_since_metric = 0;
                        bytes_sent_since_metric = 0;
                    }
                }

                // Incoming control / input packet from Android Client
                res = framed.read_packet() => {
                    match res {
                        Ok(packet) => {
                            match packet.header.msg_type {
                                MSG_INPUT_EVENT => {
                                    if let Some(event) = InputEvent::decode(&packet.payload) {
                                        let _ = input_backend.handle_event(&event).await;
                                    }
                                }
                                MSG_PING => {
                                    let pong = OrdPacket::new(MSG_PONG, 0, packet.header.sequence, packet.payload);
                                    framed.write_packet(&pong).await?;
                                }
                                MSG_DISCONNECT => {
                                    info!("Client sent disconnect request");
                                    return Ok(());
                                }
                                _ => {
                                    debug!("Received unhandled packet type: 0x{:02x}", packet.header.msg_type);
                                }
                            }
                        }
                        Err(e) => {
                            info!("Client connection ended: {}", e);
                            return Ok(());
                        }
                    }
                }
            }
        }
    }
}
