use anyhow::{anyhow, Context, Result};
use rusb::{Context as RusbContext, Device, DeviceDescriptor, DeviceHandle, Direction, TransferType, UsbContext};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

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
        let timeout = Duration::from_millis(500);
        let mut written = 0;
        while written < data.len() {
            let chunk = &data[written..];
            match self.handle.write_bulk(self.out_endpoint, chunk, timeout) {
                Ok(n) => written += n,
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

    /// Try switching an Android device into AOA mode
    pub fn switch_to_accessory(device: &Device<RusbContext>) -> Result<bool> {
        let handle = match device.open() {
            Ok(h) => h,
            Err(e) => {
                debug!("Cannot open USB device for AOA query: {:?}", e);
                return Ok(false);
            }
        };

        // 1. Check AOA protocol version
        let mut protocol_buf = [0u8; 2];
        let timeout = Duration::from_millis(1000);
        let req_type = rusb::request_type(
            rusb::Direction::In,
            rusb::RequestType::Vendor,
            rusb::Recipient::Device,
        );

        let res = handle.read_control(req_type, AOA_GET_PROTOCOL, 0, 0, &mut protocol_buf, timeout);
        let protocol_version = match res {
            Ok(2) => u16::from_le_bytes(protocol_buf),
            _ => return Ok(false), // Device doesn't support AOA
        };

        if protocol_version < 1 {
            return Ok(false);
        }

        info!("Discovered Android device supporting AOA v{}", protocol_version);

        let send_str = |idx: u16, text: &str| -> Result<()> {
            let out_type = rusb::request_type(
                rusb::Direction::Out,
                rusb::RequestType::Vendor,
                rusb::Recipient::Device,
            );
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0); // Null terminator
            handle.write_control(out_type, AOA_SEND_STRING, 0, idx, &bytes, timeout)?;
            Ok(())
        };

        // 2. Send AOA identification strings matching Android accessory_filter
        send_str(AOA_STRING_MANUFACTURER, "ORD")?;
        send_str(AOA_STRING_MODEL, "OpenRemoteDisplay")?;
        send_str(AOA_STRING_DESCRIPTION, "OpenRemoteDisplay Virtual Monitor Host")?;
        send_str(AOA_STRING_VERSION, "1.0.0")?;
        send_str(AOA_STRING_URI, "https://github.com/debarkak/ord")?;
        send_str(AOA_STRING_SERIAL, "ORD-VIRTUAL-001")?;

        // 3. Trigger switch to accessory mode
        let start_type = rusb::request_type(
            rusb::Direction::Out,
            rusb::RequestType::Vendor,
            rusb::Recipient::Device,
        );
        handle.write_control(start_type, AOA_START_ACCESSORY, 0, 0, &[], timeout)?;
        info!("Sent AOA_START_ACCESSORY signal. Device will re-enumerate in accessory mode.");

        Ok(true)
    }

    /// Open an active AOA accessory device and locate Bulk IN / OUT endpoints
    pub fn open_accessory(device: &Device<RusbContext>) -> Result<UsbAccessoryStream> {
        let mut handle = device.open().context("Failed to open AOA USB device")?;
        let config_desc = device.active_config_descriptor().context("Failed to get config descriptor")?;

        // Detach kernel driver if needed
        for iface in config_desc.interfaces() {
            let iface_num = iface.number();
            if handle.kernel_driver_active(iface_num).unwrap_or(false) {
                let _ = handle.detach_kernel_driver(iface_num);
            }
        }

        let mut in_ep = None;
        let mut out_ep = None;
        let mut iface_to_claim = 0;

        for iface in config_desc.interfaces() {
            for iface_desc in iface.descriptors() {
                for ep_desc in iface_desc.endpoint_descriptors() {
                    if ep_desc.transfer_type() == TransferType::Bulk {
                        if ep_desc.direction() == Direction::In && in_ep.is_none() {
                            in_ep = Some(ep_desc.address());
                            iface_to_claim = iface_desc.interface_number();
                        } else if ep_desc.direction() == Direction::Out && out_ep.is_none() {
                            out_ep = Some(ep_desc.address());
                            iface_to_claim = iface_desc.interface_number();
                        }
                    }
                }
            }
        }

        let in_endpoint = in_ep.ok_or_else(|| anyhow!("Bulk IN endpoint not found on AOA accessory"))?;
        let out_endpoint = out_ep.ok_or_else(|| anyhow!("Bulk OUT endpoint not found on AOA accessory"))?;

        handle.claim_interface(iface_to_claim).context("Failed to claim USB AOA interface")?;
        info!("Claimed USB AOA interface {} (Bulk IN: 0x{:02x}, Bulk OUT: 0x{:02x})", iface_to_claim, in_endpoint, out_endpoint);

        Ok(UsbAccessoryStream::new(handle, in_endpoint, out_endpoint))
    }
}
