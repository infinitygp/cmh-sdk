//! HID enumeration and raw report I/O.

use std::ffi::CString;
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::devices::{ModuleKind, VENDOR_ID};
use crate::error::Result;

/// One HID interface belonging to a MasterHUB module.
#[derive(Debug, Clone)]
pub struct HidInterface {
    pub kind: ModuleKind,
    /// Platform specific path; not stable across reconnects.
    pub path: CString,
    pub product_id: u16,
    /// USB serial number; the stable identity of a module.
    pub serial: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub release_number: u16,
    pub usage_page: u16,
    pub usage: u16,
    pub interface_number: i32,
}

/// Lists every HID interface exposed by connected MasterHUB modules.
pub fn enumerate() -> Result<Vec<HidInterface>> {
    let api = HidApi::new()?;
    let mut found: Vec<HidInterface> = api
        .device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID)
        .filter_map(|d| {
            let kind = ModuleKind::from_product_id(d.product_id())?;
            Some(HidInterface {
                kind,
                path: d.path().to_owned(),
                product_id: d.product_id(),
                serial: d.serial_number().map(str::to_owned),
                manufacturer: d.manufacturer_string().map(str::to_owned),
                product: d.product_string().map(str::to_owned),
                release_number: d.release_number(),
                usage_page: d.usage_page(),
                usage: d.usage(),
                interface_number: d.interface_number(),
            })
        })
        .collect();
    found.sort_by(|a, b| {
        (a.product_id, &a.serial, &a.path).cmp(&(b.product_id, &b.serial, &b.path))
    });
    Ok(found)
}

/// An open HID interface.
pub struct RawDevice {
    device: HidDevice,
}

impl RawDevice {
    pub fn open(interface: &HidInterface) -> Result<Self> {
        let api = HidApi::new()?;
        let device = api.open_path(&interface.path)?;
        Ok(Self { device })
    }

    /// Reads the HID report descriptor (reconstructed by the OS on Windows).
    pub fn report_descriptor(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
        let len = self.device.get_report_descriptor(&mut buf)?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Reads one input report. Returns `Ok(0)` when `timeout` elapses.
    pub fn read_input(&self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        Ok(self.device.read_timeout(buf, ms)?)
    }
}
