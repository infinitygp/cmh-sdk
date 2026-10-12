//! Discovery of connected modules and hot-plug tracking.

use hidapi::HidApi;

use crate::error::{Error, Result};
use crate::module::Module;
use crate::transport::{self, HidInterface, RawDevice};

/// A module arrived or left between two [`Manager::refresh`] calls.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HotplugEvent {
    Arrived(HidInterface),
    Left(HidInterface),
}

/// Tracks the set of connected modules by polling the HID device list.
///
/// The USB serial number identifies a module wherever it is mounted. A
/// module that re-enumerates under a different path is reported as left and
/// arrived again.
///
/// Polling cannot see a module that is pulled and re-seated between two
/// refreshes. The authoritative signal for a dead handle is
/// [`Error::Disconnected`] from the module itself; after it, drop the handle
/// and reopen whatever [`modules`](Self::modules) lists.
pub struct Manager {
    api: HidApi,
    known: Vec<HidInterface>,
}

impl Manager {
    /// Creates a manager with an empty module list; call
    /// [`refresh`](Self::refresh) to discover what is connected.
    pub fn new() -> Result<Self> {
        Ok(Self {
            api: transport::new_api()?,
            known: Vec::new(),
        })
    }

    /// Modules seen by the most recent refresh.
    pub fn modules(&self) -> &[HidInterface] {
        &self.known
    }

    /// Re-scans the bus and reports what changed since the last call.
    pub fn refresh(&mut self) -> Result<Vec<HotplugEvent>> {
        let current = transport::scan(&mut self.api)?;
        let events = diff(&self.known, &current);
        self.known = current;
        Ok(events)
    }

    /// Opens the module with the given serial number.
    pub fn open(&self, serial: &str) -> Result<Module> {
        let interface = self
            .known
            .iter()
            .find(|i| i.serial.as_deref() == Some(serial))
            .ok_or_else(|| Error::NotFound(format!("serial {serial}")))?;
        self.open_interface(interface)
    }

    /// Opens a module listed by [`modules`](Self::modules).
    ///
    /// Fails with [`Error::WrongDevice`] if another device has taken over
    /// the path since the last refresh.
    pub fn open_interface(&self, interface: &HidInterface) -> Result<Module> {
        let device = RawDevice::open(&self.api, interface)?;
        Ok(Module::new(
            interface.kind,
            interface.serial.clone(),
            Box::new(device),
        ))
    }
}

fn same_module(a: &HidInterface, b: &HidInterface) -> bool {
    a.serial == b.serial && a.path == b.path
}

fn diff(old: &[HidInterface], new: &[HidInterface]) -> Vec<HotplugEvent> {
    let left = old
        .iter()
        .filter(|o| !new.iter().any(|n| same_module(o, n)))
        .cloned()
        .map(HotplugEvent::Left);
    let arrived = new
        .iter()
        .filter(|n| !old.iter().any(|o| same_module(o, n)))
        .cloned()
        .map(HotplugEvent::Arrived);
    left.chain(arrived).collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;

    use super::*;
    use crate::devices::ModuleKind;

    fn interface(kind: ModuleKind, serial: Option<&str>, path: &str) -> HidInterface {
        HidInterface {
            kind,
            path: CString::new(path).unwrap(),
            product_id: kind.product_id(),
            serial: serial.map(str::to_owned),
            manufacturer: None,
            product: None,
            release_number: 0,
            usage_page: 0x0c,
            usage: 1,
            interface_number: 0,
        }
    }

    #[test]
    fn reports_arrivals_and_departures() {
        let knobs = interface(ModuleKind::Knobs3, Some("K1"), "p1");
        let keys = interface(ModuleKind::Keys15, Some("D1"), "p2");
        let both = [knobs.clone(), keys.clone()];
        let only_keys = [keys.clone()];
        assert_eq!(
            diff(&[], &both),
            [
                HotplugEvent::Arrived(knobs.clone()),
                HotplugEvent::Arrived(keys)
            ]
        );
        assert_eq!(diff(&both, &only_keys), [HotplugEvent::Left(knobs)]);
        assert!(diff(&only_keys, &only_keys).is_empty());
    }

    #[test]
    fn a_module_that_moved_ports_leaves_and_arrives() {
        let before = [interface(ModuleKind::Knobs3, Some("K1"), "port-a")];
        let after = [interface(ModuleKind::Knobs3, Some("K1"), "port-b")];
        assert_eq!(
            diff(&before, &after),
            [
                HotplugEvent::Left(before[0].clone()),
                HotplugEvent::Arrived(after[0].clone())
            ]
        );
    }

    #[test]
    fn a_reused_path_is_a_departure_followed_by_an_arrival() {
        // Linux hands the lowest free hidraw node to the next device.
        let before = [interface(ModuleKind::Keys15, Some("D1"), "/dev/hidraw5")];
        let after = [interface(ModuleKind::Knobs3, Some("K1"), "/dev/hidraw5")];
        assert_eq!(
            diff(&before, &after),
            [
                HotplugEvent::Left(before[0].clone()),
                HotplugEvent::Arrived(after[0].clone())
            ]
        );
    }

    #[test]
    fn two_units_of_one_model_are_distinct() {
        let one = [interface(ModuleKind::Knobs3, Some("K1"), "p1")];
        let two = [
            one[0].clone(),
            interface(ModuleKind::Knobs3, Some("K2"), "p2"),
        ];
        assert_eq!(diff(&one, &two), [HotplugEvent::Arrived(two[1].clone())]);
    }
}
