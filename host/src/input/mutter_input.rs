use super::InputBackend;
use crate::protocol::coordinate::CoordinateTransformer;
use crate::protocol::types::{InputEvent, InputEventType};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tracing::info;
use zbus::zvariant::OwnedObjectPath;
use zbus::Connection;

pub struct MutterInputBackend {
    /// Cached session proxy — NOT recreated on every event (that would be ~5-20ms overhead per touch)
    session_proxy: zbus::Proxy<'static>,
    stream_path: String,
    transformer: CoordinateTransformer,
    // Keep connection alive so session remains valid
    _connection: Connection,
    session_path: String,
}

impl MutterInputBackend {
    pub async fn new(stream_path: &str, display_width: u32, display_height: u32) -> Result<Self> {
        let conn = Connection::session()
            .await
            .context("Failed to connect to D-Bus for input")?;

        let root_proxy = zbus::Proxy::new(
            &conn,
            "org.gnome.Mutter.RemoteDesktop",
            "/org/gnome/Mutter/RemoteDesktop",
            "org.gnome.Mutter.RemoteDesktop",
        )
        .await
        .context("Failed to create RemoteDesktop proxy")?;

        let session_path: OwnedObjectPath = root_proxy
            .call("CreateSession", &())
            .await
            .context("Failed to create RemoteDesktop session")?;

        let session_path_str = session_path.as_str().to_string();
        info!("Created Mutter RemoteDesktop session: {}", session_path_str);

        // Build proxy with owned String path — avoids 'static lifetime issue
        let session_proxy: zbus::Proxy<'static> = zbus::proxy::Builder::new(&conn)
            .destination("org.gnome.Mutter.RemoteDesktop")?
            .path(session_path_str.clone())?
            .interface("org.gnome.Mutter.RemoteDesktop.Session")?
            .build()
            .await
            .context("Failed to create RemoteDesktop.Session proxy")?;

        // Note: SelectDevices does NOT exist in GNOME 46+/50.x — just call Start directly.
        // The session supports all device types (keyboard, pointer, touch) by default.
        let (): () = session_proxy
            .call("Start", &())
            .await
            .context("Failed to start RemoteDesktop.Session")?;

        info!("Mutter RemoteDesktop input active: stream={}", stream_path);

        Ok(Self {
            session_proxy,
            stream_path: stream_path.to_string(),
            transformer: CoordinateTransformer::new(display_width, display_height),
            _connection: conn,
            session_path: session_path_str,
        })
    }
}

#[async_trait]
impl InputBackend for MutterInputBackend {
    async fn handle_event(&mut self, event: &InputEvent) -> Result<()> {
        // Use the cached proxy — no re-creation overhead per event
        let sp = &self.session_proxy;
        let stream = self.stream_path.as_str();

        let (px_x, px_y) = self.transformer.normalized_to_display(event.x, event.y);

        match event.event_type {
            InputEventType::TouchDown => {
                let slot = event.slot as u32;
                let _: Result<(), _> = sp.call("NotifyTouchDown", &(stream, slot, px_x, px_y)).await;
            }
            InputEventType::TouchMove => {
                let slot = event.slot as u32;
                let _: Result<(), _> = sp.call("NotifyTouchMotion", &(stream, slot, px_x, px_y)).await;
            }
            InputEventType::TouchUp | InputEventType::TouchCancel => {
                let slot = event.slot as u32;
                let _: Result<(), _> = sp.call("NotifyTouchUp", &(slot,)).await;
            }
            InputEventType::PointerMotionAbsolute => {
                let _: Result<(), _> = sp.call("NotifyPointerMotionAbsolute", &(stream, px_x, px_y)).await;
            }
            InputEventType::PointerButton => {
                // Map logical button numbers to evdev BTN codes
                let button = match event.code_or_btn {
                    1 => 0x110i32, // BTN_LEFT
                    2 => 0x112i32, // BTN_MIDDLE
                    3 => 0x111i32, // BTN_RIGHT
                    other => other as i32,
                };
                let pressed = event.state_or_flags != 0;
                let _: Result<(), _> = sp.call("NotifyPointerButton", &(button, pressed)).await;
            }
            InputEventType::PointerAxis => {
                let dx = event.x as f64 - 32768.0;
                let dy = event.y as f64 - 32768.0;
                let flags = event.state_or_flags;
                let _: Result<(), _> = sp.call("NotifyPointerAxis", &(dx, dy, flags)).await;
            }
            InputEventType::KeyboardKey => {
                let keycode = event.code_or_btn;
                let pressed = event.state_or_flags != 0;
                let _: Result<(), _> = sp.call("NotifyKeyboardKeycode", &(keycode, pressed)).await;
            }
        }

        Ok(())
    }
}

impl Drop for MutterInputBackend {
    fn drop(&mut self) {
        let proxy = self.session_proxy.clone();
        let path = self.session_path.clone();
        tokio::spawn(async move {
            info!("Stopping Mutter RemoteDesktop session: {}", path);
            let _: Result<(), _> = proxy.call("Stop", &()).await;
        });
    }
}
