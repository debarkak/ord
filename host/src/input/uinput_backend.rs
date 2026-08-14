use super::InputBackend;
use crate::protocol::types::InputEvent;
use anyhow::Result;
use async_trait::async_trait;
use tracing::debug;

pub struct UInputBackend {}

impl UInputBackend {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl InputBackend for UInputBackend {
    async fn handle_event(&mut self, event: &InputEvent) -> Result<()> {
        debug!("UInputBackend fallback received event: {:?}", event);
        Ok(())
    }
}
