use std::process::{Child, Command};
use tracing::{debug, info};

/// FrameTicker runs the native X11/Wayland micro-damage driver (`ord-display-driver`)
/// that creates an override-redirect 2x2 micro-window explicitly positioned on the secondary
/// virtual monitor (`Meta-0`). It pulses X11 damage at 90 FPS, forcing GNOME Mutter's compositor
/// to schedule continuous 90 FPS stage repaints and record the mouse cursor at full 90 FPS
/// without obscuring wallpaper, icons, or windows.
pub struct FrameTicker {
    process: Option<Child>,
}

impl FrameTicker {
    pub fn start() -> Self {
        let driver_path = if std::path::Path::new("./host/ord-display-driver").exists() {
            "./host/ord-display-driver"
        } else if std::path::Path::new("./ord-display-driver").exists() {
            "./ord-display-driver"
        } else {
            "/tmp/ord-display-driver"
        };

        let child = match Command::new(driver_path).spawn() {
            Ok(c) => {
                info!("Started Native Display 90 FPS damage driver on secondary monitor (PID: {})", c.id());
                Some(c)
            }
            Err(e) => {
                debug!("Could not start native display frame ticker ({}): {:?}", driver_path, e);
                None
            }
        };

        Self { process: child }
    }
}

impl Drop for FrameTicker {
    fn drop(&mut self) {
        if let Some(mut child) = self.process.take() {
            debug!("Stopping native display frame ticker (PID: {})...", child.id());
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
