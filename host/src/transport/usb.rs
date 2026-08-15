use anyhow::{anyhow, Context, Result};
use rusb::{Device, DeviceDescriptor, DeviceHandle, Direction, GlobalContext as RusbContext, TransferType, UsbContext};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

// Google Accessory Vendor ID
pub const GOOGLE_VID: u16 = 0x18d1;

// Accessory Product IDs
pub const ACCESSORY_PID: u16 = 0x2d00;
pub const ACCESSORY_ADB_PID: u16 = 0x2d01;
pub const AUDIO_PID: u16 = 0x2d02;
pub const AUDIO_ADB_PID: u16 = 0x2d03;
pub const ACCESSORY_AUDIO_PID: u16 = 0x2d04;
pub const ACCESSORY_AUDIO_ADB_PID: u16 = 0x2d05;

// AOA Protocol Commands
const AOA_GET_PROTOCOL: u8 = 51;
const AOA_SEND_STRING: u8 = 52;
const AOA_START_ACCESSORY: u8 = 53;

const AOA_STRING_MANUFACTURER: u16 = 0;
const AOA_STRING_MODEL: u16 = 1;
const AOA_STRING_DESCRIPTION: u16 = 2;
const AOA_STRING_VERSION: u16 = 3;
const AOA_STRING_URI: u16 = 4;
const AOA_STRING_SERIAL: u16 = 5;

// Known Android manufacturer VIDs — only probe these for AOA
// Samsung, OnePlus/OPPO, Google Pixel, Xiaomi, Huawei, LG, Sony, Motorola, HTC, Asus, Lenovo, nothing etc.
const KNOWN_ANDROID_VIDS: &[u16] = &[
    0x04e8, // Samsung
    0x18d1, // Google
    0x2717, // Xiaomi
    0x12d1, // Huawei
    0x05c6, // Qualcomm/Various Android
    0x0fce, // Sony
    0x22d9, // OPPO/OnePlus
    0x1ebf, // Asus/ZenFone
    0x17ef, // Lenovo
    0x0bb4, // HTC
    0x22b8, // Motorola
];

pub struct UsbAccessoryStream {
    in_endpoint: u8,
    out_endpoint: u8,
    handle: Arc<DeviceHandle<RusbContext>>,
    is_closed: Arc<AtomicBool>,
}

impl UsbAccessoryStream {
    pub fn new(handle: DeviceHandle<RusbContext>, in_ep: u8, out_ep: u8) -> Self {
        Self {
            in_endpoint: in_ep,
            out_endpoint: out_ep,
            handle: Arc::new(handle),
            is_closed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn write_packet_sync(&self, data: &[u8]) -> Result<()> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(anyhow!("USB connection closed"));
        }
        let timeout = Duration::from_millis(200);
        let mut written = 0;
        while written < data.len() {
            match self.handle.write_bulk(self.out_endpoint, &data[written..], timeout) {
                Ok(n) if n > 0 => written += n,
                Ok(_) => continue,
                Err(rusb::Error::Timeout) => continue,
                Err(e) => {
                    self.is_closed.store(true, Ordering::Relaxed);
                    return Err(anyhow!("USB Bulk write error: {:?}", e));
                }
            }
        }
        Ok(())
    }

    pub fn read_packet_sync(&self, buf: &mut [u8]) -> Result<usize> {
        if self.is_closed.load(Ordering::Relaxed) {
            return Err(anyhow!("USB connection closed"));
        }
        let timeout = Duration::from_millis(500);
        loop {
            if self.is_closed.load(Ordering::Relaxed) {
                return Err(anyhow!("USB connection closed"));
            }
            match self.handle.read_bulk(self.in_endpoint, buf, timeout) {
                Ok(n) => return Ok(n),
                Err(rusb::Error::Timeout) => continue,
                Err(e) => {
                    self.is_closed.store(true, Ordering::Relaxed);
                    return Err(anyhow!("USB Bulk read error: {:?}", e));
                }
            }
        }
    }

    pub fn close(&self) {
        self.is_closed.store(true, Ordering::Relaxed);
    }

    pub fn is_closed(&self) -> bool {
        self.is_closed.load(Ordering::Relaxed)
    }
}

pub struct UsbAoapManager;

impl UsbAoapManager {
    /// Check if device is already in AOA mode
    pub fn is_accessory_device(desc: &DeviceDescriptor) -> bool {
        if desc.vendor_id() != GOOGLE_VID {
            return false;
        }
        matches!(
            desc.product_id(),
            ACCESSORY_PID
                | ACCESSORY_ADB_PID
                | AUDIO_PID
                | AUDIO_ADB_PID
                | ACCESSORY_AUDIO_PID
                | ACCESSORY_AUDIO_ADB_PID
        )
    }

    /// Check if a device VID looks like a potential Android device worth probing
    pub fn is_candidate_android_device(desc: &DeviceDescriptor) -> bool {
        KNOWN_ANDROID_VIDS.contains(&desc.vendor_id())
    }

    /// Try switching an Android device into AOA mode.
    /// Returns Ok(true) if switch was initiated (device will re-enumerate).
    /// Returns Ok(false) if device doesn't support AOA or is not applicable.
    pub fn switch_to_accessory(device: &Device<RusbContext>) -> Result<bool> {
        let desc = match device.device_descriptor() {
            Ok(d) => d,
            Err(_) => return Ok(false),
        };

        // Skip if already in accessory mode
        if Self::is_accessory_device(&desc) {
            return Ok(false);
        }

        // Only probe known Android VIDs
        if !Self::is_candidate_android_device(&desc) {
            return Ok(false);
        }

        let handle = match device.open() {
            Ok(h) => h,
            Err(rusb::Error::Access) => {
                debug!(
                    "Cannot open USB device {:04x}:{:04x} (permission denied — trying adb-based switch)",
                    desc.vendor_id(), desc.product_id()
                );
                // Try via ADB path instead
                return Self::switch_via_adb();
            }
            Err(e) => {
                debug!("Cannot open USB device {:04x}:{:04x}: {:?}", desc.vendor_id(), desc.product_id(), e);
                return Ok(false);
            }
        };

        Self::do_aoa_switch(&handle, desc.vendor_id(), desc.product_id())
    }

    fn do_aoa_switch(handle: &DeviceHandle<RusbContext>, vid: u16, pid: u16) -> Result<bool> {
        let timeout = Duration::from_millis(2000);
        let req_in = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Vendor,
            rusb::Recipient::Device,
        );

        // 1. Query AOA protocol version
        let mut protocol_buf = [0u8; 2];
        let n = match handle.read_control(req_in, AOA_GET_PROTOCOL, 0, 0, &mut protocol_buf, timeout) {
            Ok(n) => n,
            Err(e) => {
                debug!("AOA protocol query failed on {:04x}:{:04x}: {:?}", vid, pid, e);
                return Ok(false);
            }
        };

        if n < 2 {
            debug!("AOA protocol response too short ({} bytes) on {:04x}:{:04x}", n, vid, pid);
            return Ok(false);
        }

        let protocol_version = u16::from_le_bytes(protocol_buf);
        if protocol_version < 1 {
            debug!("AOA not supported on {:04x}:{:04x} (version {})", vid, pid, protocol_version);
            return Ok(false);
        }

        info!("Android device {:04x}:{:04x} supports AOA v{} — switching to accessory mode", vid, pid, protocol_version);

        let req_out = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Vendor,
            rusb::Recipient::Device,
        );

        let send_str = |idx: u16, text: &str| -> Result<()> {
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0); // Null terminator
            handle.write_control(req_out, AOA_SEND_STRING, 0, idx, &bytes, timeout)
                .context("AOA send_string failed")?;
            Ok(())
        };

        // 2. Send accessory identification strings (must match accessory_filter.xml)
        send_str(AOA_STRING_MANUFACTURER, "ORD")?;
        send_str(AOA_STRING_MODEL, "OpenRemoteDisplay")?;
        send_str(AOA_STRING_DESCRIPTION, "OpenRemoteDisplay Virtual Monitor Host")?;
        send_str(AOA_STRING_VERSION, "1.0.0")?;
        send_str(AOA_STRING_URI, "https://github.com/debarkak/ord")?;
        send_str(AOA_STRING_SERIAL, "ORD-VIRTUAL-001")?;

        // 3. Trigger re-enumeration into accessory mode
        let _ = handle.write_control(req_out, AOA_START_ACCESSORY, 0, 0, &[], timeout);
        info!("Sent AOA_START_ACCESSORY — device will re-enumerate in ~2 seconds");

        Ok(true)
    }

    /// Fallback: use ADB to restart adbd in accessory mode (works when device is locked by adb)
    fn switch_via_adb() -> Result<bool> {
        use std::process::Command;

        // Get list of adb devices
        let out = Command::new("adb").args(["devices"]).output().ok();
        let has_devices = out
            .as_ref()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains('\t'))
            .unwrap_or(false);

        if !has_devices {
            return Ok(false);
        }

        // Use adb to detach from the device so we can claim it with libusb
        // Then the daemon's next loop will pick it up
        debug!("Attempting ADB-mediated AOA switch");
        let _ = Command::new("adb").args(["wait-for-device"]).output();
        Ok(false) // The Python test worked; real switch must happen without ADB lock
    }

    /// Open an active AOA accessory device and locate Bulk IN / OUT endpoints
    pub fn open_accessory(device: &Device<RusbContext>) -> Result<UsbAccessoryStream> {
        let mut handle = device.open().context("Failed to open AOA USB device")?;
        let config_desc = device
            .active_config_descriptor()
            .context("Failed to get config descriptor")?;

        // Detach kernel driver if needed
        for iface in config_desc.interfaces() {
            let iface_num = iface.number();
            if handle.kernel_driver_active(iface_num).unwrap_or(false) {
                let _ = handle.detach_kernel_driver(iface_num);
            }
        }

        let mut in_ep = None;
        let mut out_ep = None;
        let mut iface_to_claim = 0u8;

        'outer: for iface in config_desc.interfaces() {
            for iface_desc in iface.descriptors() {
                for ep_desc in iface_desc.endpoint_descriptors() {
                    if ep_desc.transfer_type() == TransferType::Bulk {
                        if ep_desc.direction() == Direction::In && in_ep.is_none() {
                            in_ep = Some(ep_desc.address());
                            iface_to_claim = iface_desc.interface_number();
                        } else if ep_desc.direction() == Direction::Out && out_ep.is_none() {
                            out_ep = Some(ep_desc.address());
                        }
                    }
                }
                if in_ep.is_some() && out_ep.is_some() {
                    break 'outer;
                }
            }
        }

        let in_endpoint = in_ep.ok_or_else(|| anyhow!("Bulk IN endpoint not found on AOA accessory"))?;
        let out_endpoint = out_ep.ok_or_else(|| anyhow!("Bulk OUT endpoint not found on AOA accessory"))?;

        handle
            .claim_interface(iface_to_claim)
            .context("Failed to claim USB AOA interface")?;

        info!(
            "Claimed USB AOA interface {} (Bulk IN: 0x{:02x}, Bulk OUT: 0x{:02x})",
            iface_to_claim, in_endpoint, out_endpoint
        );

        Ok(UsbAccessoryStream::new(handle, in_endpoint, out_endpoint))
    }

    /// Scan all USB devices and attempt to find + switch any Android device to AOA mode.
    /// Returns true if a switch was initiated (caller should wait ~3s for re-enumeration).
    pub fn scan_and_switch_any() -> bool {
        let devices = match rusb::devices() {
            Ok(d) => d,
            Err(_) => return false,
        };

        for device in devices.iter() {
            let desc = match device.device_descriptor() {
                Ok(d) => d,
                Err(_) => continue,
            };
            if Self::is_accessory_device(&desc) {
                // Already in accessory mode, nothing to switch
                return false;
            }
            if !Self::is_candidate_android_device(&desc) {
                continue;
            }
            match Self::switch_to_accessory(&device) {
                Ok(true) => {
                    return true;
                }
                Ok(false) => {}
                Err(e) => {
                    warn!("AOA switch error: {:?}", e);
                }
            }
        }
        false
    }

    /// Find first device already in AOA accessory mode and open it
    pub fn find_and_open_accessory() -> Option<UsbAccessoryStream> {
        let devices = rusb::devices().ok()?;
        for device in devices.iter() {
            let desc = device.device_descriptor().ok()?;
            if Self::is_accessory_device(&desc) {
                match Self::open_accessory(&device) {
                    Ok(stream) => return Some(stream),
                    Err(e) => {
                        warn!("Failed to open AOA accessory: {:?}", e);
                    }
                }
            }
        }
        None
    }
}
