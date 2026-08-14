use crate::protocol::types::InputEvent;
use anyhow::Result;
use async_trait::async_trait;

pub mod mutter_input;
pub mod uinput_backend;

#[async_trait]
pub trait InputBackend: Send + Sync {
    async fn handle_event(&mut self, event: &InputEvent) -> Result<()>;
}
