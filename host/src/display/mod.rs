use anyhow::Result;
use async_trait::async_trait;

pub mod mutter;
pub mod test_source;

#[derive(Debug, Clone)]
pub struct VirtualDisplayInfo {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub pipewire_node_id: u32,
    pub stream_path: String,
    pub session_path: String,
}

#[async_trait]
pub trait DisplayBackend: Send + Sync {
    async fn create_virtual_display(&mut self, width: u32, height: u32, fps: u32) -> Result<VirtualDisplayInfo>;
    async fn destroy_virtual_display(&mut self) -> Result<()>;
}
