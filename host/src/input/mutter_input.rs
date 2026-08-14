use super::InputBackend;
use crate::protocol::coordinate::CoordinateTransformer;
use crate::protocol::types::{InputEvent, InputEventType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;
use zbus::zvariant::OwnedObjectPath;
use zbus::Connection;

pub struct MutterInputBackend {
    connection: Connection,
    session_path: String,
    stream_path: String,
    transformer: CoordinateTransformer,
}

impl MutterInputBackend {
    pub async fn new(stream_path: &str, display_width: u32, display_height: u32) -> Result<Self> {
        let conn = Connection::session().await.context("Failed to connect to D-Bus for input")?;

        let remote_desktop_proxy = zbus::Proxy::new(
            &conn,
            "org.gnome.Mutter.RemoteDesktop",
            "/org/gnome/Mutter/RemoteDesktop",
            "org.gnome.Mutter.RemoteDesktop",
        )
        .await
        .context("Failed to create RemoteDesktop proxy")?;

        let session_path: OwnedObjectPath = remote_desktop_proxy
            .call("CreateSession", &())
            .await
            .context("Failed to create RemoteDesktop session")?;

        info!("Created Mutter RemoteDesktop session: {}", session_path.as_str());

        let session_proxy = zbus::Proxy::new(
            &conn,
            "org.gnome.Mutter.RemoteDesktop",
            session_path.as_str(),
            "org.gnome.Mutter.RemoteDesktop.Session",
        )
        .await
        .context("Failed to create RemoteDesktop.Session proxy")?;

        let (): () = session_proxy
            .call("Start", &())
            .await
            .context("Failed to start RemoteDesktop.Session")?;

        info!("Mutter RemoteDesktop input session active for stream {}", stream_path);

        Ok(Self {
            connection: conn,
            session_path: session_path.as_str().to_string(),
            stream_path: stream_path.to_string(),
            transformer: CoordinateTransformer::new(display_width, display_height),
        })
    }
}

#[async_trait]
impl InputBackend for MutterInputBackend {
    async fn handle_event(&mut self, event: &InputEvent) -> Result<()> {
        let session_proxy = zbus::Proxy::new(
            &self.connection,
            "org.gnome.Mutter.RemoteDesktop",
            self.session_path.as_str(),
            "org.gnome.Mutter.RemoteDesktop.Session",
        )
        .await?;

        let (px_x, px_y) = self.transformer.normalized_to_display(event.x, event.y);

        match event.event_type {
            InputEventType::TouchDown => {
                let slot = event.slot as u32;
                let _: Result<(), _> = session_proxy
                    .call(
                        "NotifyTouchDown",
                        &(self.stream_path.as_str(), slot, px_x, px_y),
                    )
                    .await;
            }
            InputEventType::TouchMove => {
                let slot = event.slot as u32;
                let _: Result<(), _> = session_proxy
                    .call(
                        "NotifyTouchMotion",
                        &(self.stream_path.as_str(), slot, px_x, px_y),
                    )
                    .await;
            }
            InputEventType::TouchUp | InputEventType::TouchCancel => {
                let slot = event.slot as u32;
                let _: Result<(), _> = session_proxy.call("NotifyTouchUp", &(slot,)).await;
            }
            InputEventType::PointerMotionAbsolute => {
                let _: Result<(), _> = session_proxy
                    .call(
                        "NotifyPointerMotionAbsolute",
                        &(self.stream_path.as_str(), px_x, px_y),
                    )
                    .await;
            }
            InputEventType::PointerButton => {
                let button = event.code_or_btn as i32; // 1 = Left, 2 = Middle, 3 = Right
                let pressed = event.state_or_flags != 0;
                let _: Result<(), _> = session_proxy
                    .call("NotifyPointerButton", &(button, pressed))
                    .await;
            }
            InputEventType::PointerAxis => {
                let dx = event.x as f64 - 32768.0;
                let dy = event.y as f64 - 32768.0;
                let flags = event.state_or_flags;
                let _: Result<(), _> = session_proxy
                    .call("NotifyPointerAxis", &(dx, dy, flags))
                    .await;
            }
            InputEventType::KeyboardKey => {
                let keycode = event.code_or_btn;
                let pressed = event.state_or_flags != 0;
                let _: Result<(), _> = session_proxy
                    .call("NotifyKeyboardKeycode", &(keycode, pressed))
                    .await;
            }
        }

        Ok(())
    }
}

impl Drop for MutterInputBackend {
    fn drop(&mut self) {
        let conn = self.connection.clone();
        let path = self.session_path.clone();
        tokio::spawn(async move {
            if let Ok(proxy) = zbus::Proxy::new(
                &conn,
                "org.gnome.Mutter.RemoteDesktop",
                path.as_str(),
                "org.gnome.Mutter.RemoteDesktop.Session",
            )
            .await
            {
                let _: Result<(), _> = proxy.call("Stop", &()).await;
            }
        });
    }
}
