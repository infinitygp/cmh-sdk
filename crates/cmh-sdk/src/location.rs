//! USB port chain of a module.
//!
//! The base wires every mounting position to a fixed port of its internal
//! hub tree, so the chain of hub ports leading to a module tells where the
//! module sits (docs/experiments/0005 and 0007).

use std::ffi::CStr;

/// Port chain of the base's own controller below the base's first hub.
pub(crate) const BASE_CONTROLLER_PATH: [u8; 3] = [1, 4, 4];

/// Hub port numbers from the root hub down to the device behind the HID
/// interface at `path`, or `None` when the platform cannot tell.
pub(crate) fn port_chain(path: &CStr) -> Option<Vec<u8>> {
    platform::port_chain(path)
}

/// Extracts the `USB(n)` elements of a Windows location path such as
/// `PCIROOT(0)#PCI(0801)#USBROOT(0)#USB(1)#USB(4)`.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn parse_windows_location_path(location: &str) -> Option<Vec<u8>> {
    let (_, tail) = location.split_once("#USBROOT(")?;
    let chain: Vec<u8> = tail
        .split('#')
        .skip(1)
        .map_while(|part| part.strip_prefix("USB(")?.strip_suffix(')')?.parse().ok())
        .collect();
    (!chain.is_empty()).then_some(chain)
}

/// Extracts the port chain from a Linux sysfs device path: its deepest
/// component of the form `<bus>-<port>[.<port>...]`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_linux_sysfs_path(path: &str) -> Option<Vec<u8>> {
    path.split('/').rev().find_map(|component| {
        let (bus, ports) = component.split_once('-')?;
        if bus.is_empty() || !bus.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        ports.split('.').map(|port| port.parse().ok()).collect()
    })
}

#[cfg(windows)]
mod platform {
    use std::ffi::CStr;

    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_Get_Parent,
        CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_SUCCESS,
    };
    use windows_sys::Win32::Devices::Properties::{
        DEVPKEY_Device_InstanceId, DEVPKEY_Device_LocationPaths,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    /// Runs a property query and returns its (first) string.
    fn first_string(query: impl Fn(*mut u32, *mut u8, *mut u32) -> u32) -> Option<String> {
        let mut buf = [0u16; 1024];
        let mut size = std::mem::size_of_val(&buf) as u32;
        let mut kind = 0u32;
        if query(&mut kind, buf.as_mut_ptr().cast(), &mut size) != CR_SUCCESS {
            return None;
        }
        let len = buf.iter().position(|c| *c == 0)?;
        Some(String::from_utf16_lossy(&buf[..len]))
    }

    pub(super) fn port_chain(path: &CStr) -> Option<Vec<u8>> {
        let interface = wide(path.to_str().ok()?);
        // SAFETY: `interface` is NUL terminated and outlives the call; the
        // out pointers come from `first_string` and describe its buffer.
        let instance = first_string(|kind, buf, size| unsafe {
            CM_Get_Device_Interface_PropertyW(
                interface.as_ptr(),
                &DEVPKEY_Device_InstanceId,
                kind,
                buf,
                size,
                0,
            )
        })?;
        let instance = wide(&instance);
        let mut node = 0u32;
        // SAFETY: `instance` is NUL terminated; `node` is a valid out pointer.
        if unsafe { CM_Locate_DevNodeW(&mut node, instance.as_ptr(), CM_LOCATE_DEVNODE_NORMAL) }
            != CR_SUCCESS
        {
            return None;
        }
        // Modules have a single interface, so the parent of the HID node is
        // the USB device itself.
        let mut usb_device = 0u32;
        // SAFETY: `usb_device` is a valid out pointer.
        if unsafe { CM_Get_Parent(&mut usb_device, node, 0) } != CR_SUCCESS {
            return None;
        }
        // SAFETY: as for the interface property above.
        let location = first_string(|kind, buf, size| unsafe {
            CM_Get_DevNode_PropertyW(
                usb_device,
                &DEVPKEY_Device_LocationPaths,
                kind,
                buf,
                size,
                0,
            )
        })?;
        super::parse_windows_location_path(&location)
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::ffi::CStr;
    use std::path::Path;

    pub(super) fn port_chain(path: &CStr) -> Option<Vec<u8>> {
        let node = Path::new(path.to_str().ok()?).file_name()?;
        let device = Path::new("/sys/class/hidraw").join(node).join("device");
        let resolved = std::fs::canonicalize(device).ok()?;
        super::parse_linux_sysfs_path(resolved.to_str()?)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    pub(super) fn port_chain(_path: &std::ffi::CStr) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_location_paths() {
        // Location path of the base controller on the test machine.
        assert_eq!(
            parse_windows_location_path(
                "PCIROOT(0)#PCI(0801)#PCI(0003)#USBROOT(0)#USB(1)#USB(4)#USB(3)#USB(1)#USB(4)#USB(4)"
            ),
            Some(vec![1, 4, 3, 1, 4, 4])
        );
        assert_eq!(
            parse_windows_location_path("PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(2)#USBMI(0)"),
            Some(vec![2])
        );
        assert_eq!(
            parse_windows_location_path("PCIROOT(0)#PCI(1400)#USBROOT(0)"),
            None
        );
        assert_eq!(parse_windows_location_path("ACPI(_SB_)#ACPI(PCI0)"), None);
    }

    #[test]
    fn parses_linux_sysfs_paths() {
        assert_eq!(
            parse_linux_sysfs_path(
                "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-4/1-4.3/1-4.3.1/1-4.3.1.3/1-4.3.1.3.4/1-4.3.1.3.4:1.0/0003:2516:0202.0007"
            ),
            Some(vec![4, 3, 1, 3, 4])
        );
        assert_eq!(
            parse_linux_sysfs_path(
                "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-2/1-2:1.0/0003:2516:01F6.0001"
            ),
            Some(vec![2])
        );
        assert_eq!(
            parse_linux_sysfs_path("/sys/devices/virtual/misc/uhid/0005:046D:B023.0003"),
            None
        );
    }
}
