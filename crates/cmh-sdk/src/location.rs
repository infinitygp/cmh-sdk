//! USB location of a module.
//!
//! The base wires every mounting position to a fixed port of its internal
//! hub tree, so the chain of hub ports leading to a module tells where the
//! module sits (docs/experiments/0005 and 0007).

use std::ffi::CStr;

/// Port chain of the base's own controller below the base's first hub.
pub(crate) const BASE_CONTROLLER_PATH: [u8; 3] = [1, 4, 4];

/// Where a device hangs on the USB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UsbLocation {
    /// Identifies the host controller. Port chains are only comparable
    /// between devices on the same bus.
    pub bus: String,
    /// Hub port numbers from the root hub down to the device.
    pub ports: Vec<u8>,
}

/// Location of the device behind the HID interface at `path`, or `None`
/// when the platform cannot tell.
pub(crate) fn usb_location(path: &CStr) -> Option<UsbLocation> {
    platform::usb_location(path)
}

/// Parses a Windows location path such as
/// `PCIROOT(0)#PCI(0801)#USBROOT(0)#USB(1)#USB(4)`.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn parse_windows_location_path(location: &str) -> Option<UsbLocation> {
    let (controller, tail) = location.split_once("#USBROOT(")?;
    let (root, rest) = tail.split_once(')')?;
    let mut ports = Vec::new();
    for part in rest.split('#').skip(1) {
        // Elements behind the ports (for example `USBMI(0)` of a composite
        // device) end the chain; a port that cannot be read voids it.
        let Some(port) = part.strip_prefix("USB(") else {
            break;
        };
        ports.push(port.strip_suffix(')')?.parse().ok()?);
    }
    (!ports.is_empty()).then(|| UsbLocation {
        bus: format!("{controller}#USBROOT({root})"),
        ports,
    })
}

/// Parses a Linux sysfs device path: its deepest component of the form
/// `<bus>-<port>[.<port>...]`.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn parse_linux_sysfs_path(path: &str) -> Option<UsbLocation> {
    path.split('/').rev().find_map(|component| {
        let (bus, ports) = component.split_once('-')?;
        if bus.is_empty() || !bus.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let ports = ports
            .split('.')
            .map(|port| port.parse().ok())
            .collect::<Option<Vec<u8>>>()?;
        Some(UsbLocation {
            bus: bus.to_owned(),
            ports,
        })
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

    use super::UsbLocation;

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

    pub(super) fn usb_location(path: &CStr) -> Option<UsbLocation> {
        let interface = wide(path.to_str().ok()?);
        // SAFETY: `interface` is NUL terminated and outlives the call; the
        // out pointers come from `first_string` and describe its buffer,
        // whose size is passed in bytes.
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

    use super::UsbLocation;

    pub(super) fn usb_location(path: &CStr) -> Option<UsbLocation> {
        let node = Path::new(path.to_str().ok()?).file_name()?;
        let device = Path::new("/sys/class/hidraw").join(node).join("device");
        let resolved = std::fs::canonicalize(device).ok()?;
        super::parse_linux_sysfs_path(resolved.to_str()?)
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    pub(super) fn usb_location(_path: &std::ffi::CStr) -> Option<super::UsbLocation> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn windows(location: &str) -> Option<(String, Vec<u8>)> {
        parse_windows_location_path(location).map(|l| (l.bus, l.ports))
    }

    fn linux(path: &str) -> Option<(String, Vec<u8>)> {
        parse_linux_sysfs_path(path).map(|l| (l.bus, l.ports))
    }

    #[test]
    fn parses_windows_location_paths() {
        // Location path of the base controller on the test machine.
        assert_eq!(
            windows(
                "PCIROOT(0)#PCI(0801)#PCI(0003)#USBROOT(0)#USB(1)#USB(4)#USB(3)#USB(1)#USB(4)#USB(4)"
            ),
            Some((
                "PCIROOT(0)#PCI(0801)#PCI(0003)#USBROOT(0)".to_owned(),
                vec![1, 4, 3, 1, 4, 4]
            ))
        );
        assert_eq!(
            windows("PCIROOT(0)#PCI(1400)#USBROOT(0)#USB(2)#USBMI(0)").map(|l| l.1),
            Some(vec![2])
        );
        assert_eq!(windows("PCIROOT(0)#PCI(1400)#USBROOT(0)"), None);
        assert_eq!(windows("ACPI(_SB_)#ACPI(PCI0)"), None);
        // A port that cannot be read must not yield a shortened chain.
        assert_eq!(
            windows("PCIROOT(0)#USBROOT(0)#USB(1)#USB(300)#USB(2)"),
            None
        );
    }

    #[test]
    fn different_controllers_are_different_buses() {
        let a = windows("PCIROOT(0)#PCI(0801)#USBROOT(0)#USB(1)").unwrap();
        let b = windows("PCIROOT(0)#PCI(0802)#USBROOT(0)#USB(1)").unwrap();
        assert_eq!(a.1, b.1);
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn parses_linux_sysfs_paths() {
        assert_eq!(
            linux(
                "/sys/devices/pci0000:00/0000:00:14.0/usb1/1-4/1-4.3/1-4.3.1/1-4.3.1.3/1-4.3.1.3.4/1-4.3.1.3.4:1.0/0003:2516:0202.0007"
            ),
            Some(("1".to_owned(), vec![4, 3, 1, 3, 4]))
        );
        assert_eq!(
            linux("/sys/devices/pci0000:00/0000:00:14.0/usb3/3-2/3-2:1.0/0003:2516:01F6.0001"),
            Some(("3".to_owned(), vec![2]))
        );
        assert_eq!(
            linux("/sys/devices/virtual/misc/uhid/0005:046D:B023.0003"),
            None
        );
    }
}
