use crate::config::Config;
use crate::display::mutter::MutterDisplayBackend;
use crate::display::test_source::TestSourceDisplayBackend;
use crate::display::ticker::FrameTicker;
use crate::display::{DisplayBackend, VirtualDisplayInfo};
use crate::encoder::{EncoderConfig, VideoFrame, VideoPipeline};
use crate::input::mutter_input::MutterInputBackend;
use crate::input::uinput_backend::UInputBackend;
use crate::input::InputBackend;
use crate::protocol::packet::{OrdHeader, OrdPacket, ORD_HEADER_SIZE};
use crate::protocol::types::*;
use crate::transport::tcp::FramedStream;
use anyhow::{anyhow, Context, Result};
use std::sync::Arc;
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
        let fps = if hello.max_fps > 0 { hello.max_fps } else { self.config.display.default_fps };

        // 2. Initialize Display Backend
        let mut display_backend: Box<dyn DisplayBackend> = if self.force_test_pattern {
            Box::new(TestSourceDisplayBackend::new())
        } else {
            Box::new(MutterDisplayBackend::new())
        };

        let display_info = match display_backend.create_virtual_display(width, height, fps).await {
            Ok(info) => {
                info!("Successfully created GNOME Mutter Virtual Display: {:?}", info);
                info
            }
            Err(e) => {
                warn!("Failed to create Mutter virtual display ({}). Falling back to test pattern source.", e);
                display_backend = Box::new(TestSourceDisplayBackend::new());
                display_backend.create_virtual_display(width, height, fps).await?
            }
        };

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
        let (frame_tx, mut frame_rx) = mpsc::channel::<VideoFrame>(16);
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
        let _ticker = FrameTicker::start();

        info!("Display and video streaming session established with {}", peer);

        // 7. Run Bidirectional Session Loop
        let session_result = Self::run_session_loop(&mut framed, &mut frame_rx, &mut *input_backend).await;

        // 8. Clean Shutdown
        info!("Closing session for {}, cleaning up display...", peer);
        let _ = pipeline.stop();
        let _ = display_backend.destroy_virtual_display().await;

        session_result
    }

    pub async fn handle_usb_client(&self, stream: std::sync::Arc<crate::transport::usb::UsbAccessoryStream>) -> Result<()> {
        info!("Starting USB AOAP session handler...");
        let (usb_in_tx, mut usb_in_rx) = mpsc::channel::<OrdPacket>(120);
        let (usb_out_tx, mut usb_out_rx) = mpsc::channel::<Vec<u8>>(120);

        // USB Read Worker Thread
        let read_stream = stream.clone();
        let read_task = tokio::task::spawn_blocking(move || {
            let mut buf = vec![0u8; 65536];
            let mut accumulator = Vec::with_capacity(131072);
            loop {
                match read_stream.read_packet_sync(&mut buf) {
                    Ok(n) if n > 0 => {
                        accumulator.extend_from_slice(&buf[..n]);
                        while accumulator.len() >= ORD_HEADER_SIZE {
                            if let Ok(header) = OrdHeader::decode(&accumulator) {
                                let total_len = ORD_HEADER_SIZE + header.payload_len as usize;
                                if accumulator.len() >= total_len {
                                    let packet_bytes = accumulator.drain(..total_len).collect::<Vec<u8>>();
                                    if let Ok(packet) = OrdPacket::decode(&packet_bytes) {
                                        if usb_in_tx.blocking_send(packet).is_err() {
                                            return;
                                        }
                                    }
                                } else {
                                    break;
                                }
                            } else {
                                // Drop invalid leading byte to search for next valid magic
                                accumulator.remove(0);
                            }
                        }
                    }
                    _ => break,
                }
            }
        });

        // USB Write Worker Thread
        let write_stream = stream.clone();
        let write_task = tokio::task::spawn_blocking(move || {
            while let Some(data) = usb_out_rx.blocking_recv() {
                if write_stream.write_packet_sync(&data).is_err() {
                    break;
                }
            }
        });

        // 1. Handshake: Wait for HELLO
        let hello_packet = usb_in_rx.recv().await.ok_or_else(|| anyhow!("USB connection closed before HELLO"))?;
        if hello_packet.header.msg_type != MSG_HELLO {
            return Err(anyhow!("Expected MSG_HELLO, got {}", hello_packet.header.msg_type));
        }

        let hello: HelloMessage = serde_json::from_slice(&hello_packet.payload)
            .context("Failed to deserialize HelloMessage")?;

        info!(
            "USB Client connected: '{}' (v{}) Screen: {}x{} @ {}fps, DPI: {}",
            hello.client_name, hello.client_version, hello.screen_width, hello.screen_height, hello.max_fps, hello.density_dpi
        );

        let width = if hello.screen_width > 0 { hello.screen_width } else { self.config.display.default_width };
        let height = if hello.screen_height > 0 { hello.screen_height } else { self.config.display.default_height };
        let fps = if hello.max_fps > 0 { hello.max_fps } else { self.config.display.default_fps };

        // 2. Initialize Display Backend
        let mut display_backend: Box<dyn DisplayBackend> = if self.force_test_pattern {
            Box::new(TestSourceDisplayBackend::new())
        } else {
            Box::new(MutterDisplayBackend::new())
        };

        let display_info = match display_backend.create_virtual_display(width, height, fps).await {
            Ok(info) => {
                info!("Successfully created GNOME Mutter Virtual Display: {:?}", info);
                info
            }
            Err(e) => {
                warn!("Failed to create Mutter virtual display ({}). Falling back to test pattern source.", e);
                display_backend = Box::new(TestSourceDisplayBackend::new());
                display_backend.create_virtual_display(width, height, fps).await?
            }
        };

        // 3. Send HELLO_ACK
        let host_name = hostname::get()
            .map(|h| h.to_string_lossy().to_string())
            .unwrap_or_else(|_| "ORD-Host".to_string());

        let hello_ack = HelloAckMessage {
            server_name: host_name,
            server_version: "1.0.0".to_string(),
            session_id: format!("ord-usb-{}", std::process::id()),
            width: display_info.width,
            height: display_info.height,
            fps: display_info.fps,
            selected_codec: "h264".to_string(),
            auth_required: false,
        };

        let ack_packet = OrdPacket::new(MSG_HELLO_ACK, 0, 0, serde_json::to_vec(&hello_ack)?);
        let _ = usb_out_tx.send(ack_packet.encode()).await;

        // 4. Send DISPLAY_CONFIG
        let display_config = DisplayConfigMessage {
            width: display_info.width,
            height: display_info.height,
            refresh_rate: display_info.fps,
            orientation: 0,
            scale_factor: self.config.display.scale_factor,
        };
        let cfg_packet = OrdPacket::new(MSG_DISPLAY_CONFIG, 0, 1, serde_json::to_vec(&display_config)?);
        let _ = usb_out_tx.send(cfg_packet.encode()).await;

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
        let (mut frame_tx, mut frame_rx) = mpsc::channel::<VideoFrame>(16);
        let encoder_config = EncoderConfig {
            width: display_info.width,
            height: display_info.height,
            fps: display_info.fps,
            bitrate_kbps: self.config.stream.bitrate_kbps,
            encoder_choice: self.config.stream.encoder.clone(),
            keyframe_interval_frames: self.config.stream.keyframe_interval_frames,
        };

        let mut display_info = display_info;
        let mut pipeline = VideoPipeline::new(display_info.pipewire_node_id, &encoder_config, frame_tx)?;
        pipeline.start()?;
        let mut error_notifier = pipeline.error_notifier();
        let _ticker = FrameTicker::start();

        info!("Display and video streaming session established over USB AOAP");

        let mut frame_seq: u32 = 0;
        loop {
            tokio::select! {
                _ = error_notifier.notified() => {
                    warn!("Display stream error from GStreamer. Auto-recovering virtual display & encoder pipeline...");
                    Self::recover_display(
                        &mut pipeline,
                        &mut display_backend,
                        &mut display_info,
                        &mut frame_rx,
                        &mut error_notifier,
                        &encoder_config,
                        width,
                        height,
                        fps,
                    ).await;
                }
                recv_res = tokio::time::timeout(std::time::Duration::from_millis(500), frame_rx.recv()) => {
                    match recv_res {
                        Ok(Some(frame)) => {
                            frame_seq = frame_seq.wrapping_add(1);
                            let mut flags = 0u16;
                            if frame.is_keyframe {
                                flags |= FLAG_KEYFRAME;
                            }
                            flags |= FLAG_END_OF_FRAME;

                            let packet = OrdPacket::new(MSG_VIDEO_DATA, flags, frame_seq, frame.data);
                            // Use try_send: if USB write task is busy, drop the frame (keep real-time)
                            // This prevents the session loop from blocking on USB backpressure
                            match usb_out_tx.try_send(packet.encode()) {
                                Ok(_) => {}
                                Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                                    // USB is busy — frame is dropped. Decoder will interpolate.
                                }
                                Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                                    break;
                                }
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            // 500ms watchdog: no frames arrived from Mutter (e.g. GNOME Settings changed scaling/resolution)
                            warn!("No video frames received for 500ms (GNOME display scaling or resolution changed). Auto-recovering virtual display...");
                            Self::recover_display(
                                &mut pipeline,
                                &mut display_backend,
                                &mut display_info,
                                &mut frame_rx,
                                &mut error_notifier,
                                &encoder_config,
                                width,
                                height,
                                fps,
                            ).await;
                        }
                    }
                }
                Some(packet) = usb_in_rx.recv() => {
                    match packet.header.msg_type {
                        MSG_INPUT_EVENT => {
                            if let Some(event) = InputEvent::decode(&packet.payload) {
                                let _ = input_backend.handle_event(&event).await;
                            }
                        }
                        MSG_PING => {
                            let pong = OrdPacket::new(MSG_PONG, 0, packet.header.sequence, packet.payload);
                            let _ = usb_out_tx.send(pong.encode()).await;
                        }
                        MSG_DISCONNECT => {
                            info!("Client sent disconnect request");
                            break;
                        }
                        MSG_HELLO => {
                            info!("Client reconnected with new HELLO message — restarting session");
                            break;
                        }
                        _ => {}
                    }
                }
                else => break,
            }
        }

        info!("Closing USB AOAP session, cleaning up display...");
        stream.close();
        let _ = pipeline.stop();
        let _ = display_backend.destroy_virtual_display().await;
        let _ = read_task.await;
        let _ = write_task.await;

        Ok(())
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

    async fn recover_display(
        pipeline: &mut VideoPipeline,
        display_backend: &mut Box<dyn DisplayBackend>,
        display_info: &mut VirtualDisplayInfo,
        frame_rx: &mut mpsc::Receiver<VideoFrame>,
        error_notifier: &mut Arc<tokio::sync::Notify>,
        encoder_config: &EncoderConfig,
        width: u32,
        height: u32,
        fps: u32,
    ) {
        let _ = pipeline.stop();
        let _ = display_backend.destroy_virtual_display().await;
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        match display_backend.create_virtual_display(width, height, fps).await {
            Ok(new_info) => {
                *display_info = new_info;
                let (new_tx, new_rx) = mpsc::channel::<VideoFrame>(16);
                *frame_rx = new_rx;
                match VideoPipeline::new(display_info.pipewire_node_id, encoder_config, new_tx) {
                    Ok(new_pipeline) => {
                        let _ = new_pipeline.start();
                        *error_notifier = new_pipeline.error_notifier();
                        *pipeline = new_pipeline;
                        info!("Display stream successfully recovered! Streaming seamlessly at 165 FPS.");
                    }
                    Err(e) => {
                        warn!("Could not recreate video pipeline after display reconfig: {:?}", e);
                    }
                }
            }
            Err(e) => {
                warn!("Could not recreate virtual display after GNOME display reconfig: {:?}", e);
            }
        }
    }
}
