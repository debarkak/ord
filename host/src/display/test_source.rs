use super::{DisplayBackend, VirtualDisplayInfo};
use anyhow::Result;
use async_trait::async_trait;
use tracing::info;

pub struct TestSourceDisplayBackend {
    info: Option<VirtualDisplayInfo>,
}

impl TestSourceDisplayBackend {
    pub fn new() -> Self {
        Self { info: None }
    }
}

#[async_trait]
impl DisplayBackend for TestSourceDisplayBackend {
    async fn create_virtual_display(&mut self, width: u32, height: u32, fps: u32) -> Result<VirtualDisplayInfo> {
        info!("Creating synthetic test pattern display source ({}x{} @ {} fps)", width, height, fps);
        let info = VirtualDisplayInfo {
            name: "ORD-Test-Pattern".to_string(),
            width,
            height,
            fps,
            pipewire_node_id: 0, // 0 indicates test pattern source
            stream_path: "test-pattern".to_string(),
            session_path: "test-session".to_string(),
        };
        self.info = Some(info.clone());
        Ok(info)
    }

    async fn destroy_virtual_display(&mut self) -> Result<()> {
        self.info = None;
        info!("Test pattern display destroyed.");
        Ok(())
    }
}
