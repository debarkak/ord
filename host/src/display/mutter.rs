use super::{DisplayBackend, VirtualDisplayInfo};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use futures_util::StreamExt;
use std::collections::HashMap;
use tracing::{info, warn};
use zbus::zvariant::{OwnedObjectPath, Value};
use zbus::Connection;

pub struct MutterDisplayBackend {
    connection: Option<Connection>,
    session_path: Option<String>,
    stream_path: Option<String>,
    node_id: Option<u32>,
    display_info: Option<VirtualDisplayInfo>,
}

impl MutterDisplayBackend {
    pub fn new() -> Self {
        Self {
            connection: None,
            session_path: None,
            stream_path: None,
            node_id: None,
            display_info: None,
        }
    }
}

#[async_trait]
impl DisplayBackend for MutterDisplayBackend {
    async fn create_virtual_display(&mut self, width: u32, height: u32, fps: u32) -> Result<VirtualDisplayInfo> {
        info!("Connecting to Mutter ScreenCast via D-Bus session bus...");
        let conn = Connection::session().await.context("Failed to connect to D-Bus session bus")?;

        // 1. Call CreateSession on org.gnome.Mutter.ScreenCast
        let screencast_proxy = zbus::Proxy::new(
            &conn,
            "org.gnome.Mutter.ScreenCast",
            "/org/gnome/Mutter/ScreenCast",
            "org.gnome.Mutter.ScreenCast",
        )
        .await
        .context("Failed to create Mutter ScreenCast proxy")?;

        let empty_props: HashMap<&str, Value> = HashMap::new();
        let session_path: OwnedObjectPath = screencast_proxy
            .call("CreateSession", &(empty_props,))
            .await
            .context("Failed to call ScreenCast.CreateSession")?;

        info!("Created ScreenCast session: {}", session_path.as_str());

        // 2. Create Session Proxy
        let session_proxy = zbus::Proxy::new(
            &conn,
            "org.gnome.Mutter.ScreenCast",
            session_path.as_str(),
            "org.gnome.Mutter.ScreenCast.Session",
        )
        .await
        .context("Failed to create ScreenCast.Session proxy")?;

        // 3. Call RecordVirtual with properties
        let mut virtual_props: HashMap<&str, Value> = HashMap::new();
        virtual_props.insert("is-recording-indicator", Value::from(false));
        virtual_props.insert("cursor-mode", Value::from(2u32));

        let stream_path: OwnedObjectPath = session_proxy
            .call("RecordVirtual", &(virtual_props,))
            .await
            .context("Failed to call ScreenCast.Session.RecordVirtual")?;

        info!("Created Virtual Monitor stream: {}", stream_path.as_str());

        // 4. Create Stream Proxy to listen for PipeWireStreamAdded signal
        let stream_proxy = zbus::Proxy::new(
            &conn,
            "org.gnome.Mutter.ScreenCast",
            stream_path.as_str(),
            "org.gnome.Mutter.ScreenCast.Stream",
        )
        .await
        .context("Failed to create ScreenCast.Stream proxy")?;

        let mut signal_stream = stream_proxy.receive_signal("PipeWireStreamAdded").await?;

        // 5. Start the ScreenCast Session
        let (): () = session_proxy
            .call("Start", &())
            .await
            .context("Failed to call ScreenCast.Session.Start")?;

        info!("ScreenCast session started, awaiting PipeWireStreamAdded signal...");

        // 6. Wait for the signal with timeout
        let node_id = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            if let Some(msg) = signal_stream.next().await {
                let node_id: u32 = msg.body().deserialize()?;
                return Ok::<u32, anyhow::Error>(node_id);
            }
            Err(anyhow!("Stream signal stream ended prematurely"))
        })
        .await
        .context("Timed out waiting for PipeWireStreamAdded signal from Mutter")??;

        info!("Received PipeWire Node ID: {}", node_id);

        let info = VirtualDisplayInfo {
            name: "ORD-Virtual-1".to_string(),
            width,
            height,
            fps,
            pipewire_node_id: node_id,
            stream_path: stream_path.as_str().to_string(),
            session_path: session_path.as_str().to_string(),
        };

        self.connection = Some(conn);
        self.session_path = Some(session_path.as_str().to_string());
        self.stream_path = Some(stream_path.as_str().to_string());
        self.node_id = Some(node_id);
        self.display_info = Some(info.clone());

        Ok(info)
    }

    async fn destroy_virtual_display(&mut self) -> Result<()> {
        if let (Some(conn), Some(session_path)) = (&self.connection, &self.session_path) {
            info!("Stopping Mutter ScreenCast session: {}", session_path);
            if let Ok(session_proxy) = zbus::Proxy::new(
                conn,
                "org.gnome.Mutter.ScreenCast",
                session_path.as_str(),
                "org.gnome.Mutter.ScreenCast.Session",
            )
            .await
            {
                let _: Result<(), _> = session_proxy.call("Stop", &()).await;
            }
        }
        self.session_path = None;
        self.stream_path = None;
        self.node_id = None;
        self.display_info = None;
        info!("Virtual display destroyed cleanly.");
        Ok(())
    }
}

impl Drop for MutterDisplayBackend {
    fn drop(&mut self) {
        if self.session_path.is_some() {
            warn!("MutterDisplayBackend dropped without explicit destroy_virtual_display call");
        }
    }
}
