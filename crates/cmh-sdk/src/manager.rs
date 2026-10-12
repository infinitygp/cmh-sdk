//! Discovery of connected modules and hot-plug tracking.

use hidapi::HidApi;

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::location::BASE_CONTROLLER_PATH;
use crate::module::Module;
use crate::placement::{self, Placement};
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
/// This is the low-level view: listing, hot-plug differences and opening
/// single modules. [`Deck`](crate::Deck) builds the usual event loop on top.
///
/// The USB serial number identifies a module wherever it is mounted. A
/// module that re-enumerates under a different path or on a different port
/// is reported as left and arrived again.
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

    /// Port path of a module below the first hub of the base it is mounted
    /// on, for example `[1, 3, 4]`.
    ///
    /// The base wires each mounting position to a fixed hub port, so this
    /// path identifies where the module sits. It is `None` when the platform
    /// does not expose USB topology or no base is connected upstream of the
    /// module. The base itself yields its controller path.
    pub fn mount_path(&self, interface: &HidInterface) -> Option<Vec<u8>> {
        mount_path(&self.known, interface)
    }

    /// The grid cells a module covers on its base and how it is turned,
    /// worked out from its [`mount_path`](Self::mount_path).
    pub fn placement(&self, interface: &HidInterface) -> Option<Placement> {
        placement::placement(interface.kind, &self.mount_path(interface)?)
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

pub(crate) fn mount_path(known: &[HidInterface], interface: &HidInterface) -> Option<Vec<u8>> {
    let chain = interface.port_chain.as_deref()?;
    known
        .iter()
        // Chains from different host controllers can look alike.
        .filter(|i| i.kind == ModuleKind::Base && i.usb_bus == interface.usb_bus)
        .filter_map(|base| {
            // Everything in front of the controller's own path is the route
            // from the host to the base's first hub.
            base.port_chain
                .as_deref()?
                .strip_suffix(&BASE_CONTROLLER_PATH)
        })
        .find_map(|prefix| chain.strip_prefix(prefix))
        .filter(|path| !path.is_empty())
        .map(<[u8]>::to_vec)
}

/// Whether two listings describe the same module in the same place.
///
/// The USB location is part of the comparison because Windows derives the
/// HID path from the serial number: a module moved to another pad keeps its
/// path. A location that is unknown on either side is not held against the
/// match, and descriptive strings never are: the OS can fail to read them
/// on a single enumeration.
pub(crate) fn same_module(a: &HidInterface, b: &HidInterface) -> bool {
    let same_place = match (&a.port_chain, &b.port_chain) {
        (Some(x), Some(y)) => x == y && a.usb_bus == b.usb_bus,
        _ => true,
    };
    a.serial == b.serial && a.path == b.path && same_place
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
            port_chain: None,
            usb_bus: None,
        }
    }

    fn at(kind: ModuleKind, chain: &[u8]) -> HidInterface {
        HidInterface {
            port_chain: Some(chain.to_vec()),
            usb_bus: Some("bus".to_owned()),
            ..interface(kind, Some("S"), "p")
        }
    }

    #[test]
    fn mount_path_is_relative_to_the_base() {
        // Chains recorded on the test machine (experiment 0005).
        let base = at(ModuleKind::Base, &[1, 4, 3, 1, 4, 4]);
        let knobs = at(ModuleKind::Knobs3, &[1, 4, 3, 1, 3, 4]);
        let keys = at(ModuleKind::Keys15, &[1, 4, 3, 2, 2]);
        let known = [base.clone(), knobs.clone(), keys.clone()];
        assert_eq!(mount_path(&known, &knobs), Some(vec![1, 3, 4]));
        assert_eq!(mount_path(&known, &keys), Some(vec![2, 2]));
        assert_eq!(mount_path(&known, &base), Some(vec![1, 4, 4]));
    }

    #[test]
    fn mount_path_needs_a_matching_base() {
        let knobs = at(ModuleKind::Knobs3, &[1, 4, 3, 1, 3, 4]);
        // No base at all.
        assert_eq!(mount_path(std::slice::from_ref(&knobs), &knobs), None);
        // A base on another branch of the bus.
        let other_base = at(ModuleKind::Base, &[2, 1, 4, 4]);
        assert_eq!(mount_path(&[other_base, knobs.clone()], &knobs), None);
        // Topology unknown on this platform.
        let unknown = interface(ModuleKind::Knobs3, Some("K"), "p");
        let base = at(ModuleKind::Base, &[1, 4, 3, 1, 4, 4]);
        assert_eq!(mount_path(&[base, unknown.clone()], &unknown), None);
    }

    #[test]
    fn mount_path_ignores_a_base_on_another_controller() {
        // Two bases whose chains look alike but hang on different host
        // controllers; the knobs sit on the first.
        let ours = at(ModuleKind::Base, &[2, 1, 4, 4]);
        let mut other = at(ModuleKind::Base, &[2, 1, 1, 4, 4]);
        other.usb_bus = Some("other".to_owned());
        let knobs = at(ModuleKind::Knobs3, &[2, 1, 3, 4]);
        for known in [
            [other.clone(), ours.clone(), knobs.clone()],
            [ours, other, knobs.clone()],
        ] {
            assert_eq!(mount_path(&known, &knobs), Some(vec![1, 3, 4]));
        }
    }

    #[test]
    fn listings_match_despite_cosmetic_differences() {
        let a = at(ModuleKind::Keys15, &[1, 2, 2]);
        let mut b = a.clone();
        b.product = Some("15xKey w/ IPS, 4x3".to_owned());
        b.manufacturer = Some("COOLER MASTER".to_owned());
        assert!(same_module(&a, &b));
        // An unknown location on one side does not make it another module.
        b.port_chain = None;
        b.usb_bus = None;
        assert!(same_module(&a, &b));
        assert!(diff(&[a], &[b]).is_empty());
    }

    #[test]
    fn mount_path_picks_the_right_base_of_two() {
        let first = at(ModuleKind::Base, &[1, 1, 4, 4]);
        let second = at(ModuleKind::Base, &[2, 1, 4, 4]);
        let knobs = at(ModuleKind::Knobs3, &[2, 3, 4]);
        let known = [first, second, knobs.clone()];
        assert_eq!(mount_path(&known, &knobs), Some(vec![3, 4]));
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
    fn a_module_moved_to_another_pad_leaves_and_arrives() {
        // Windows keeps the HID path of a module that changes ports.
        let before = [at(ModuleKind::Knobs3, &[1, 4, 3, 1, 3, 4])];
        let after = [at(ModuleKind::Knobs3, &[1, 4, 3, 1, 2, 4])];
        assert_eq!(before[0].path, after[0].path);
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
