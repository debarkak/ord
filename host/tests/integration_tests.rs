use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::sync::mpsc;

use ord_host::config::Config;
use ord_host::encoder::{EncoderConfig, VideoFrame, VideoPipeline};
use ord_host::protocol::coordinate::CoordinateTransformer;
use ord_host::protocol::packet::{OrdHeader, OrdPacket, ORD_HEADER_SIZE};
use ord_host::protocol::types::*;
use ord_host::session::SessionHandler;
use ord_host::transport::tcp::FramedStream;

#[tokio::test]
async fn test_test_pattern_pipeline_generation() {
    let (tx, mut rx) = mpsc::channel::<VideoFrame>(10);
    let config = EncoderConfig {
        width: 640,
        height: 480,
        fps: 30,
        bitrate_kbps: 2000,
        encoder_choice: "software".to_string(),
        keyframe_interval_frames: 30,
    };

    // pipewire_node_id = 0 generates synthetic videotestsrc pattern
    let pipeline = VideoPipeline::new(0, &config, tx).expect("Failed to create test pipeline");
    pipeline.start().expect("Failed to start pipeline");

    let frame = tokio::time::timeout(Duration::from_secs(3), rx.recv())
        .await
        .expect("Timed out waiting for encoded frame")
        .expect("Channel closed without receiving frame");

    assert!(!frame.data.is_empty(), "Encoded frame data must not be empty");
    pipeline.stop().expect("Failed to stop pipeline");
}

#[tokio::test]
async fn test_end_to_end_session_handshake() {
    let mut config = Config::default();
    config.server.port = 19092; // Use test port
    config.stream.encoder = "software".to_string();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:19092")
        .await
        .expect("Failed to bind test listener");

    // Spawn server in test pattern mode
    let handler = SessionHandler::new(config, true);
    tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let _ = handler.handle_client(stream).await;
        }
    });

    // Client connection simulation
    tokio::time::sleep(Duration::from_millis(100)).await;
    let stream = TcpStream::connect("127.0.0.1:19092")
        .await
        .expect("Client failed to connect to test server");

    let mut framed = FramedStream::new(stream).expect("Failed to frame stream");

    // 1. Send HELLO
    let hello = HelloMessage {
        client_name: "Test-Android-Tablet".to_string(),
        client_version: "1.0.0".to_string(),
        screen_width: 1920,
        screen_height: 1200,
        density_dpi: 240,
        max_fps: 60,
        supported_codecs: vec!["h264".to_string()],
    };
    let hello_pkt = OrdPacket::new(MSG_HELLO, 0, 0, serde_json::to_vec(&hello).unwrap());
    framed.write_packet(&hello_pkt).await.expect("Failed to send HELLO");

    // 2. Receive HELLO_ACK
    let ack_pkt = tokio::time::timeout(Duration::from_secs(3), framed.read_packet())
        .await
        .expect("Timeout waiting for HELLO_ACK")
        .expect("Failed to read HELLO_ACK packet");

    assert_eq!(ack_pkt.header.msg_type, MSG_HELLO_ACK);
    let ack: HelloAckMessage = serde_json::from_slice(&ack_pkt.payload).unwrap();
    assert_eq!(ack.width, 1920);
    assert_eq!(ack.height, 1200);

    // 3. Receive DISPLAY_CONFIG
    let cfg_pkt = tokio::time::timeout(Duration::from_secs(3), framed.read_packet())
        .await
        .expect("Timeout waiting for DISPLAY_CONFIG")
        .expect("Failed to read DISPLAY_CONFIG packet");

    assert_eq!(cfg_pkt.header.msg_type, MSG_DISPLAY_CONFIG);

    // 4. Send INPUT_EVENT
    let event = InputEvent {
        event_type: InputEventType::TouchDown,
        slot: 0,
        x: 32768,
        y: 32768,
        code_or_btn: 0,
        state_or_flags: 1,
    };
    let input_pkt = OrdPacket::new(MSG_INPUT_EVENT, 0, 1, event.encode().to_vec());
    framed.write_packet(&input_pkt).await.expect("Failed to send INPUT_EVENT");

    // 5. Receive at least one VIDEO_DATA packet
    let video_pkt = tokio::time::timeout(Duration::from_secs(3), framed.read_packet())
        .await
        .expect("Timeout waiting for VIDEO_DATA")
        .expect("Failed to read VIDEO_DATA packet");

    assert_eq!(video_pkt.header.msg_type, MSG_VIDEO_DATA);
    assert!(!video_pkt.payload.is_empty());

    // 6. Clean Disconnect
    let disc_pkt = OrdPacket::new(MSG_DISCONNECT, 0, 2, vec![]);
    framed.write_packet(&disc_pkt).await.expect("Failed to send DISCONNECT");
}
