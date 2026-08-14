use anyhow::{anyhow, Context, Result};
use gstreamer::prelude::*;
use gstreamer::{BufferFlags, ElementFactory, Pipeline, State};
use gstreamer_app::{AppSink, AppSinkCallbacks};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct VideoFrame {
    pub pts_us: u64,
    pub is_keyframe: bool,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct EncoderConfig {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub bitrate_kbps: u32,
    pub encoder_choice: String, // "auto", "vaapi", "software"
    pub keyframe_interval_frames: u32,
}

impl Default for EncoderConfig {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1200,
            fps: 60,
            bitrate_kbps: 15000,
            encoder_choice: "auto".to_string(),
            keyframe_interval_frames: 60,
        }
    }
}

pub struct VideoPipeline {
    pipeline: Pipeline,
    is_running: Arc<AtomicBool>,
}

impl VideoPipeline {
    pub fn new(
        pipewire_node_id: u32,
        config: &EncoderConfig,
        frame_sender: mpsc::Sender<VideoFrame>,
    ) -> Result<Self> {
        gstreamer::init().context("Failed to initialize GStreamer")?;

        let pipeline_str = Self::build_pipeline_string(pipewire_node_id, config)?;
        info!("Creating GStreamer video pipeline: {}", pipeline_str);

        let pipeline = gstreamer::parse::launch(&pipeline_str)
            .context("Failed to parse and launch GStreamer pipeline")?
            .downcast::<Pipeline>()
            .map_err(|_| anyhow!("Launched element is not a Pipeline"))?;

        let appsink = pipeline
            .by_name("ord_sink")
            .context("Could not find ord_sink appsink element")?
            .downcast::<AppSink>()
            .map_err(|_| anyhow!("Element ord_sink is not an AppSink"))?;

        let is_running = Arc::new(AtomicBool::new(true));
        let is_running_clone = Arc::clone(&is_running);

        let callbacks = AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                if !is_running_clone.load(Ordering::Relaxed) {
                    return Ok(gstreamer::FlowSuccess::Ok);
                }

                match sink.pull_sample() {
                    Ok(sample) => {
                        if let Some(buffer) = sample.buffer() {
                            let pts_us = buffer.pts().map(|p| p.useconds()).unwrap_or(0);
                            let is_keyframe = !buffer.flags().contains(BufferFlags::DELTA_UNIT);

                            if let Ok(map) = buffer.map_readable() {
                                let frame = VideoFrame {
                                    pts_us,
                                    is_keyframe,
                                    data: map.as_slice().to_vec(),
                                };

                                // Use try_send to avoid blocking encoder callback thread on network congestion
                                if let Err(e) = frame_sender.try_send(frame) {
                                    match e {
                                        mpsc::error::TrySendError::Full(_) => {
                                            warn!("Frame channel full, dropping video frame to maintain real-time latency");
                                        }
                                        mpsc::error::TrySendError::Closed(_) => {
                                            debug!("Frame channel closed");
                                        }
                                    }
                                }
                            }
                        }
                        Ok(gstreamer::FlowSuccess::Ok)
                    }
                    Err(e) => {
                        warn!("Error pulling sample from appsink: {:?}", e);
                        Ok(gstreamer::FlowSuccess::Ok)
                    }
                }
            })
            .build();

        appsink.set_callbacks(callbacks);

        Ok(Self {
            pipeline,
            is_running,
        })
    }

    fn build_pipeline_string(pipewire_node_id: u32, config: &EncoderConfig) -> Result<String> {
        let source_str = if pipewire_node_id > 0 {
            format!("pipewiresrc path={} do-timestamp=true", pipewire_node_id)
        } else {
            format!(
                "videotestsrc is-live=true pattern=smpte ! video/x-raw,width={},height={},framerate={}/1",
                config.width, config.height, config.fps
            )
        };

        // Determine encoder element
        let use_vaapi = if config.encoder_choice == "software" {
            false
        } else if config.encoder_choice == "vaapi" {
            true
        } else {
            // Auto detect
            ElementFactory::find("vah264enc").is_some()
        };

        let encoder_str = if use_vaapi && ElementFactory::find("vah264enc").is_some() {
            info!("Using VA-API Hardware H.264 Encoder (vah264enc)");
            format!("videoconvert ! videoscale ! vah264enc bitrate={}", config.bitrate_kbps)
        } else if ElementFactory::find("x264enc").is_some() {
            info!("Using Software x264 Encoder (x264enc) with zerolatency tune");
            format!(
                "videoconvert ! videoscale ! x264enc tune=zerolatency speed-preset=ultrafast bitrate={} key-int-max={}",
                config.bitrate_kbps, config.keyframe_interval_frames
            )
        } else if ElementFactory::find("openh264enc").is_some() {
            info!("Using OpenH264 Encoder (openh264enc)");
            format!("videoconvert ! videoscale ! openh264enc bitrate={}", config.bitrate_kbps * 1000)
        } else {
            return Err(anyhow!("No compatible H.264 encoder found on system"));
        };

        let pipeline_str = format!(
            "{} ! {} ! h264parse config-interval=-1 ! appsink name=ord_sink emit-signals=true max-buffers=2 drop=true sync=false",
            source_str, encoder_str
        );

        Ok(pipeline_str)
    }

    pub fn start(&self) -> Result<()> {
        info!("Starting GStreamer pipeline...");
        self.pipeline
            .set_state(State::Playing)
            .map_err(|e| anyhow!("Failed to set pipeline state to Playing: {:?}", e))?;
        Ok(())
    }

    pub fn stop(&self) -> Result<()> {
        info!("Stopping GStreamer pipeline...");
        self.is_running.store(false, Ordering::Relaxed);
        let _ = self.pipeline.set_state(State::Null);
        Ok(())
    }
}

impl Drop for VideoPipeline {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
