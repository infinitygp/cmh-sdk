//! HID enumeration and raw report I/O.
//!
//! Raw report access is deliberately private to this crate: callers can only
//! reach the closed set of commands offered by [`Module`](crate::Module).

use std::ffi::{CStr, CString};
use std::time::Duration;

use hidapi::{HidApi, HidDevice};

use crate::devices::{ModuleKind, VENDOR_ID};
use crate::error::{Error, Result};
use crate::location;

/// Raw report access to one module. Buffers always start with the report ID.
pub(crate) trait Transport: Send {
    /// Reads one input report. Returns `Ok(0)` when `timeout` elapses.
    fn read_input(&self, buf: &mut [u8], timeout: Duration) -> Result<usize>;
    /// Writes one output report.
    fn write_output(&self, report: &[u8]) -> Result<()>;
    /// Reads the feature report whose ID is in `buf[0]`.
    fn get_feature(&self, buf: &mut [u8]) -> Result<usize>;
    /// Writes one feature report.
    fn send_feature(&self, report: &[u8]) -> Result<()>;
    /// Reads the HID report descriptor (reconstructed by the OS on Windows).
    fn report_descriptor(&self) -> Result<Vec<u8>>;
}

/// One HID interface belonging to a MasterHUB module.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct HidInterface {
    pub kind: ModuleKind,
    /// Platform specific path. It may or may not survive a reconnect, so it
    /// must not be stored as an identity.
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
    /// Hub port numbers from the root hub down to this module, when the
    /// platform exposes them. Changes when the module is moved; see
    /// [`Manager::mount_path`](crate::Manager::mount_path).
    pub port_chain: Option<Vec<u8>>,
    /// Identifies the USB host controller the port chain belongs to.
    pub usb_bus: Option<String>,
}

/// Lists every HID interface exposed by connected MasterHUB modules.
pub fn enumerate() -> Result<Vec<HidInterface>> {
    scan(&mut new_api()?)
}

// `HidApi::new` scans every HID device once. Disabling that is a
// process-wide switch in hidapi and would affect other users of the library
// in the same process, so only the later rescans are narrowed down.
pub(crate) fn new_api() -> Result<HidApi> {
    HidApi::new().map_err(Error::backend)
}

/// Re-reads the device list, restricted to the MasterHUB vendor ID so that
/// unrelated HID devices are never opened or queried.
pub(crate) fn scan(api: &mut HidApi) -> Result<Vec<HidInterface>> {
    api.reset_devices().map_err(Error::backend)?;
    api.add_devices(VENDOR_ID, 0).map_err(Error::backend)?;
    let mut found: Vec<HidInterface> = api
        .device_list()
        .filter(|d| d.vendor_id() == VENDOR_ID)
        .filter_map(|d| {
            let kind = ModuleKind::from_product_id(d.product_id())?;
            let location = location::usb_location(d.path());
            Some(HidInterface {
                kind,
                path: d.path().to_owned(),
                product_id: d.product_id(),
                // An empty serial identifies nothing.
                serial: d
                    .serial_number()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned),
                manufacturer: d.manufacturer_string().map(str::to_owned),
                product: d.product_string().map(str::to_owned),
                release_number: d.release_number(),
                usage_page: d.usage_page(),
                usage: d.usage(),
                interface_number: d.interface_number(),
                port_chain: location.as_ref().map(|l| l.ports.clone()),
                usb_bus: location.map(|l| l.bus),
            })
        })
        .collect();
    found.sort_by(|a, b| {
        (a.product_id, &a.serial, &a.path).cmp(&(b.product_id, &b.serial, &b.path))
    });
    Ok(found)
}

/// An open HID interface.
pub(crate) struct RawDevice {
    device: HidDevice,
}

impl RawDevice {
    /// Opens `interface` and checks that the device behind the path still is
    /// the module that was enumerated. Paths can be reused by another
    /// device after a swap, and module families do not share command codes.
    pub(crate) fn open(api: &HidApi, interface: &HidInterface) -> Result<Self> {
        let path = interface.path.to_string_lossy().into_owned();
        let device = api.open_path(&interface.path).map_err(|err| {
            if permission_denied(&interface.path) {
                Error::PermissionDenied { path: path.clone() }
            } else {
                Error::backend(err)
            }
        })?;
        let info = device.get_device_info().map_err(Error::backend)?;
        let same = info.vendor_id() == VENDOR_ID
            && info.product_id() == interface.product_id
            && (interface.serial.is_none() || info.serial_number() == interface.serial.as_deref());
        if !same {
            return Err(Error::WrongDevice {
                path,
                found: format!(
                    "{:04x}:{:04x} serial {}",
                    info.vendor_id(),
                    info.product_id(),
                    info.serial_number().unwrap_or("?")
                ),
            });
        }
        Ok(Self { device })
    }
}

#[cfg(unix)]
fn permission_denied(path: &CStr) -> bool {
    use std::os::unix::ffi::OsStrExt;

    let path = std::path::Path::new(std::ffi::OsStr::from_bytes(path.to_bytes()));
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .is_err_and(|err| err.kind() == std::io::ErrorKind::PermissionDenied)
}

#[cfg(not(unix))]
fn permission_denied(_path: &CStr) -> bool {
    false
}

impl Transport for RawDevice {
    fn read_input(&self, buf: &mut [u8], timeout: Duration) -> Result<usize> {
        let ms = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);
        self.device
            .read_timeout(buf, ms)
            .map_err(Error::disconnected)
    }

    fn write_output(&self, report: &[u8]) -> Result<()> {
        self.device.write(report).map_err(Error::disconnected)?;
        Ok(())
    }

    fn get_feature(&self, buf: &mut [u8]) -> Result<usize> {
        self.device
            .get_feature_report(buf)
            .map_err(Error::disconnected)
    }

    fn send_feature(&self, report: &[u8]) -> Result<()> {
        self.device
            .send_feature_report(report)
            .map_err(Error::disconnected)
    }

    fn report_descriptor(&self) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; hidapi::MAX_REPORT_DESCRIPTOR_SIZE];
        let len = self
            .device
            .get_report_descriptor(&mut buf)
            .map_err(Error::disconnected)?;
        buf.truncate(len);
        Ok(buf)
    }
}
