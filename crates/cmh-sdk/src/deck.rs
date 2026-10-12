//! All connected modules handled as one unit.
//!
//! [`Deck`] is the entry point most applications want: it finds the modules,
//! opens them, follows hot-plug and turns everything into one stream of
//! [`DeckEvent`]s. Drawing and lighting go through [`Deck::module`].
//!
//! ```no_run
//! use std::time::Duration;
//! use cmh_sdk::{Deck, DeckEvent};
//!
//! let mut deck = Deck::new()?;
//! loop {
//!     for event in deck.poll(Duration::from_millis(50))? {
//!         match event {
//!             DeckEvent::Connected(info) => println!("+ {} at {:?}", info.kind, info.placement),
//!             DeckEvent::Disconnected(info) => println!("- {}", info.kind),
//!             DeckEvent::Input { id, event, .. } => println!("{id}: {event:?}"),
//!             _ => {}
//!         }
//!     }
//! }
//! # Ok::<(), cmh_sdk::Error>(())
//! ```

use std::fmt;
use std::time::{Duration, Instant};

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::input::InputEvent;
use crate::manager::{self, Manager};
use crate::module::Module;
use crate::placement::{self, Placement};
use crate::transport::HidInterface;

/// Identity of a module for as long as the application runs: its USB serial
/// number, which stays the same wherever the module is mounted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(String);

impl ModuleId {
    fn of(interface: &HidInterface) -> Self {
        // Every module seen so far has a serial; the path is a last resort.
        Self(
            interface
                .serial
                .clone()
                .unwrap_or_else(|| interface.path.to_string_lossy().into_owned()),
        )
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A module the deck has open.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ModuleInfo {
    pub id: ModuleId,
    pub kind: ModuleKind,
    pub interface: HidInterface,
    /// USB port path below the base, see [`Manager::mount_path`].
    pub mount_path: Option<Vec<u8>>,
    /// Cells covered on the base. A module that is moved disconnects and
    /// connects again with its new placement.
    pub placement: Option<Placement>,
}

/// Something that happened on the deck.
#[derive(Debug)]
#[non_exhaustive]
pub enum DeckEvent {
    /// A module was found, opened and prepared.
    Connected(ModuleInfo),
    /// A module was removed or stopped responding.
    Disconnected(ModuleInfo),
    /// A control was operated. Faders report their positions once right
    /// after [`Connected`](DeckEvent::Connected).
    Input {
        id: ModuleId,
        kind: ModuleKind,
        event: InputEvent,
    },
    /// A raw input report; only with [`DeckOptions::raw_reports`].
    RawInput {
        id: ModuleId,
        kind: ModuleKind,
        report: Vec<u8>,
    },
    /// An input report could not be decoded. The module stays usable.
    Undecodable {
        id: ModuleId,
        kind: ModuleKind,
        error: Error,
    },
    /// A listed module could not be opened. Reported once; the deck keeps
    /// retrying on every rescan.
    OpenFailed {
        interface: HidInterface,
        error: Error,
    },
}

type Filter = Box<dyn Fn(&HidInterface) -> bool + Send>;

/// Behaviour of a [`Deck`].
pub struct DeckOptions {
    software_mode: bool,
    raw_reports: bool,
    rescan: Duration,
    filter: Option<Filter>,
}

impl Default for DeckOptions {
    fn default() -> Self {
        Self {
            software_mode: true,
            raw_reports: false,
            rescan: Duration::from_secs(1),
            filter: None,
        }
    }
}

impl DeckOptions {
    /// Whether display modules are switched to software mode when they
    /// connect, as the vendor application does before drawing. Default:
    /// `true`. With `false` the deck never writes to a module on its own.
    pub fn software_mode(mut self, enabled: bool) -> Self {
        self.software_mode = enabled;
        self
    }

    /// Also emit every input report as [`DeckEvent::RawInput`].
    pub fn raw_reports(mut self, enabled: bool) -> Self {
        self.raw_reports = enabled;
        self
    }

    /// How often the device list is checked for arrivals and departures.
    /// Default: once per second.
    pub fn rescan_interval(mut self, interval: Duration) -> Self {
        self.rescan = interval;
        self
    }

    /// Only open modules for which `keep` returns `true`.
    pub fn filter(mut self, keep: impl Fn(&HidInterface) -> bool + Send + 'static) -> Self {
        self.filter = Some(Box::new(keep));
        self
    }
}

/// Source of modules; hidapi in production, scripted in tests.
pub(crate) trait Backend: Send {
    fn scan(&mut self) -> Result<Vec<HidInterface>>;
    fn open(&self, interface: &HidInterface) -> Result<Module>;
}

impl Backend for Manager {
    fn scan(&mut self) -> Result<Vec<HidInterface>> {
        self.refresh()?;
        Ok(self.modules().to_vec())
    }

    fn open(&self, interface: &HidInterface) -> Result<Module> {
        self.open_interface(interface)
    }
}

struct Slot {
    info: ModuleInfo,
    module: Module,
}

/// Everything that is connected, opened and polled together.
///
/// A deck is meant to live on one thread: call [`poll`](Self::poll) in a
/// loop and send commands through [`module`](Self::module) between polls.
pub struct Deck {
    backend: Box<dyn Backend>,
    options: DeckOptions,
    slots: Vec<Slot>,
    /// Modules whose open error has been reported and not yet resolved.
    failed: Vec<HidInterface>,
    last_scan: Option<Instant>,
}

impl Deck {
    /// Creates a deck with default options. Modules show up as
    /// [`DeckEvent::Connected`] from the first [`poll`](Self::poll).
    pub fn new() -> Result<Self> {
        Self::with_options(DeckOptions::default())
    }

    pub fn with_options(options: DeckOptions) -> Result<Self> {
        Ok(Self::with_backend(Box::new(Manager::new()?), options))
    }

    pub(crate) fn with_backend(backend: Box<dyn Backend>, options: DeckOptions) -> Self {
        Self {
            backend,
            options,
            slots: Vec::new(),
            failed: Vec::new(),
            last_scan: None,
        }
    }

    /// Modules that are currently open.
    pub fn modules(&self) -> impl Iterator<Item = &ModuleInfo> {
        self.slots.iter().map(|s| &s.info)
    }

    /// The open module with this ID, for drawing, lighting and queries.
    ///
    /// If a command fails with [`Error::Disconnected`] simply carry on: the
    /// next poll reports the module as disconnected.
    pub fn module(&mut self, id: &ModuleId) -> Option<&mut Module> {
        self.slots
            .iter_mut()
            .find(|s| s.info.id == *id)
            .map(|s| &mut s.module)
    }

    /// Waits up to `timeout` for something to happen and returns it.
    ///
    /// Returns early as soon as there is at least one event; an empty list
    /// means the timeout elapsed. Fails only when the HID backend itself
    /// does; problems with single modules are reported as events.
    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<DeckEvent>> {
        const READ_SLICE: Duration = Duration::from_millis(1);
        const IDLE_SLICE: Duration = Duration::from_millis(10);

        let deadline = Instant::now() + timeout;
        let mut events = Vec::new();
        loop {
            if self
                .last_scan
                .is_none_or(|t| t.elapsed() >= self.options.rescan)
            {
                self.last_scan = Some(Instant::now());
                self.rescan(&mut events)?;
            }
            self.read_all(READ_SLICE, &mut events);

            let now = Instant::now();
            if !events.is_empty() || now >= deadline {
                return Ok(events);
            }
            if self.slots.is_empty() {
                std::thread::sleep(IDLE_SLICE.min(deadline - now));
            }
        }
    }

    fn rescan(&mut self, events: &mut Vec<DeckEvent>) -> Result<()> {
        let listed = self.backend.scan()?;
        let wanted: Vec<&HidInterface> = listed
            .iter()
            .filter(|i| self.options.filter.as_ref().is_none_or(|keep| keep(i)))
            .collect();

        let (gone, kept): (Vec<Slot>, Vec<Slot>) = std::mem::take(&mut self.slots)
            .into_iter()
            .partition(|slot| !wanted.iter().any(|w| **w == slot.info.interface));
        self.slots = kept;
        events.extend(gone.into_iter().map(|s| DeckEvent::Disconnected(s.info)));
        self.failed.retain(|f| wanted.contains(&f));

        for interface in wanted {
            if self.slots.iter().any(|s| s.info.interface == *interface) {
                continue;
            }
            let mount_path = manager::mount_path(&listed, interface);
            let info = ModuleInfo {
                id: ModuleId::of(interface),
                kind: interface.kind,
                interface: interface.clone(),
                placement: mount_path
                    .as_deref()
                    .and_then(|path| placement::placement(interface.kind, path)),
                mount_path,
            };
            match self.open(interface) {
                Ok((module, initial)) => {
                    self.failed.retain(|f| f != interface);
                    events.push(DeckEvent::Connected(info.clone()));
                    events.extend(initial.into_iter().map(|event| DeckEvent::Input {
                        id: info.id.clone(),
                        kind: info.kind,
                        event,
                    }));
                    self.slots.push(Slot { info, module });
                }
                Err(error) => {
                    if !self.failed.contains(interface) {
                        self.failed.push(interface.clone());
                        events.push(DeckEvent::OpenFailed {
                            interface: interface.clone(),
                            error,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Opens and prepares a module; returns it with the events that describe
    /// its initial state.
    fn open(&self, interface: &HidInterface) -> Result<(Module, Vec<InputEvent>)> {
        let mut module = self.backend.open(interface)?;
        let capabilities = interface.kind.capabilities();
        if self.options.software_mode && capabilities.display.is_some() {
            module.set_software_mode(true)?;
        }
        let mut initial = Vec::new();
        if capabilities.faders > 0 {
            let positions = module.fader_positions()?;
            initial.extend(positions.iter().enumerate().map(|(index, value)| {
                InputEvent::AnalogChanged {
                    index: index as u8,
                    value: *value,
                }
            }));
        }
        Ok((module, initial))
    }

    fn read_all(&mut self, slice: Duration, events: &mut Vec<DeckEvent>) {
        let mut lost = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let (id, kind) = (&slot.info.id, slot.info.kind);
            let report = match slot.module.read_raw(slice) {
                Ok(Some(report)) => report,
                Ok(None) => continue,
                Err(_) => {
                    lost.push(index);
                    continue;
                }
            };
            if self.options.raw_reports {
                events.push(DeckEvent::RawInput {
                    id: id.clone(),
                    kind,
                    report: report.clone(),
                });
            }
            match slot.module.decode(&report) {
                Ok(decoded) => events.extend(decoded.into_iter().map(|event| DeckEvent::Input {
                    id: id.clone(),
                    kind,
                    event,
                })),
                Err(error) => events.push(DeckEvent::Undecodable {
                    id: id.clone(),
                    kind,
                    error,
                }),
            }
        }
        // A module that is still (or again) listed is reopened by the next
        // rescan and announced with a fresh `Connected`.
        for index in lost.into_iter().rev() {
            let slot = self.slots.remove(index);
            events.push(DeckEvent::Disconnected(slot.info));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::testing::{Mock, Sent};

    /// What the fake bus currently lists, and the transport behind each path.
    #[derive(Default)]
    struct Bus {
        listed: Vec<HidInterface>,
        transports: Vec<(CString, Mock)>,
        refuse: Vec<CString>,
        scans: usize,
    }

    #[derive(Clone, Default)]
    struct FakeBackend(Arc<Mutex<Bus>>);

    impl Backend for FakeBackend {
        fn scan(&mut self) -> Result<Vec<HidInterface>> {
            let mut bus = self.0.lock().unwrap();
            bus.scans += 1;
            Ok(bus.listed.clone())
        }

        fn open(&self, interface: &HidInterface) -> Result<Module> {
            let bus = self.0.lock().unwrap();
            if bus.refuse.contains(&interface.path) {
                return Err(Error::PermissionDenied {
                    path: interface.path.to_string_lossy().into_owned(),
                });
            }
            let (_, mock) = bus
                .transports
                .iter()
                .find(|(path, _)| *path == interface.path)
                .expect("no transport scripted for this path");
            // The real backend talks to the device while opening it.
            if mock.0.lock().unwrap().unplugged {
                return Err(Error::Disconnected("mock unplugged".to_owned()));
            }
            Ok(Module::new(
                interface.kind,
                interface.serial.clone(),
                Box::new(mock.clone()),
            ))
        }
    }

    impl FakeBackend {
        /// Plugs a module in and returns the handle that scripts it.
        fn plug(&self, kind: ModuleKind, serial: &str, chain: &[u8]) -> Mock {
            let mock = Mock::with_features(&[&[0x33, 0x05, 10, 20, 30, 40, 50]]);
            let path = CString::new(format!("path-{serial}-{chain:?}")).unwrap();
            let mut bus = self.0.lock().unwrap();
            bus.listed
                .push(interface(kind, serial, path.clone(), chain));
            bus.transports.push((path, mock.clone()));
            mock
        }

        fn unplug(&self, serial: &str) {
            let mut bus = self.0.lock().unwrap();
            bus.listed.retain(|i| i.serial.as_deref() != Some(serial));
        }

        fn deck(&self, options: DeckOptions) -> Deck {
            Deck::with_backend(
                Box::new(self.clone()),
                options.rescan_interval(Duration::ZERO),
            )
        }
    }

    fn interface(kind: ModuleKind, serial: &str, path: CString, chain: &[u8]) -> HidInterface {
        HidInterface {
            kind,
            path,
            product_id: kind.product_id(),
            serial: Some(serial.to_owned()),
            manufacturer: None,
            product: None,
            release_number: 0,
            usage_page: 0x0c,
            usage: 1,
            interface_number: 0,
            port_chain: Some(chain.to_vec()),
        }
    }

    fn poll(deck: &mut Deck) -> Vec<DeckEvent> {
        deck.poll(Duration::ZERO).unwrap()
    }

    fn summary(events: &[DeckEvent]) -> Vec<String> {
        events
            .iter()
            .map(|e| match e {
                DeckEvent::Connected(i) => format!("+{}", i.id),
                DeckEvent::Disconnected(i) => format!("-{}", i.id),
                DeckEvent::Input { id, event, .. } => format!("{id}:{event:?}"),
                DeckEvent::RawInput { id, report, .. } => format!("{id}:raw{}", report.len()),
                DeckEvent::Undecodable { id, .. } => format!("{id}:undecodable"),
                DeckEvent::OpenFailed { interface, .. } => {
                    format!("!{}", interface.serial.as_deref().unwrap())
                }
            })
            .collect()
    }

    #[test]
    fn connects_modules_with_their_placement() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Base, "B", &[7, 1, 4, 4]);
        let keys = bus.plug(ModuleKind::Keys15, "K", &[7, 2, 2]);
        let mut deck = bus.deck(DeckOptions::default());

        let events = poll(&mut deck);
        assert_eq!(summary(&events), ["+B", "+K"]);
        let DeckEvent::Connected(info) = &events[1] else {
            panic!()
        };
        assert_eq!(info.mount_path.as_deref(), Some(&[2u8, 2][..]));
        let placement = info.placement.unwrap();
        assert_eq!(
            (
                placement.column,
                placement.row,
                placement.columns,
                placement.rows
            ),
            (1, 1, 4, 3)
        );
        // Display modules are switched to software mode, nothing else is sent.
        let mut mode = vec![0u8; 32];
        mode[..3].copy_from_slice(&[0x03, 0x3a, 0x01]);
        assert_eq!(keys.sent(), [Sent::Feature(mode)]);
        assert_eq!(deck.modules().count(), 2);
    }

    #[test]
    fn passive_deck_writes_nothing() {
        let bus = FakeBackend::default();
        let keys = bus.plug(ModuleKind::Keys15, "K", &[1]);
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[2]);
        let mut deck = bus.deck(DeckOptions::default().software_mode(false));
        poll(&mut deck);
        assert!(keys.sent().is_empty());
        assert!(knobs.sent().is_empty());
    }

    #[test]
    fn faders_announce_their_positions() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Faders5, "F", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        let events = summary(&poll(&mut deck));
        assert_eq!(events.len(), 6);
        assert_eq!(events[0], "+F");
        assert_eq!(events[1], "F:AnalogChanged { index: 0, value: 10 }");
        assert_eq!(events[5], "F:AnalogChanged { index: 4, value: 50 }");
    }

    #[test]
    fn delivers_input_and_optionally_raw_reports() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default().raw_reports(true));
        poll(&mut deck);

        knobs.push_input(&[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0xff, 0xff, 0x01]);
        assert_eq!(
            summary(&poll(&mut deck)),
            ["N:raw64", "N:EncoderDelta { index: 2, delta: 1 }"]
        );
        // A report that cannot be decoded does not cost the module.
        knobs.push_input(&[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0x09, 0xff, 0xff]);
        assert_eq!(summary(&poll(&mut deck)), ["N:raw64", "N:undecodable"]);
        assert_eq!(deck.modules().count(), 1);
        assert!(poll(&mut deck).is_empty());
    }

    #[test]
    fn a_pulled_module_disconnects_and_comes_back() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        // The handle dies first; the bus still lists the module.
        knobs.unplug();
        assert_eq!(summary(&poll(&mut deck)), ["-N"]);
        // Still unusable: reported once, then retried silently.
        assert_eq!(summary(&poll(&mut deck)), ["!N"]);
        assert!(poll(&mut deck).is_empty());

        bus.unplug("N");
        assert!(poll(&mut deck).is_empty());
        bus.plug(ModuleKind::Knobs3, "N", &[3]);
        assert_eq!(summary(&poll(&mut deck)), ["+N"]);
    }

    #[test]
    fn a_module_that_leaves_the_bus_is_dropped() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.plug(ModuleKind::Rollers2, "R", &[2]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);
        bus.unplug("N");
        assert_eq!(summary(&poll(&mut deck)), ["-N"]);
        assert_eq!(deck.modules().count(), 1);
    }

    #[test]
    fn a_moved_module_reconnects_with_its_new_place() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Base, "B", &[1, 4, 4]);
        bus.plug(ModuleKind::Knobs3, "N", &[1, 3, 4]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        // Same serial, different port: Windows even keeps the HID path.
        bus.unplug("N");
        bus.plug(ModuleKind::Knobs3, "N", &[1, 2, 4]);
        let events = poll(&mut deck);
        assert_eq!(summary(&events), ["-N", "+N"]);
        let DeckEvent::Connected(info) = &events[1] else {
            panic!()
        };
        assert_eq!(info.placement.map(|p| (p.column, p.row)), Some((1, 1)));
    }

    #[test]
    fn open_failures_are_reported_once_and_retried() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let path = bus.0.lock().unwrap().listed[0].path.clone();
        bus.0.lock().unwrap().refuse.push(path);
        let mut deck = bus.deck(DeckOptions::default());

        assert_eq!(summary(&poll(&mut deck)), ["!N"]);
        assert!(poll(&mut deck).is_empty());
        bus.0.lock().unwrap().refuse.clear();
        assert_eq!(summary(&poll(&mut deck)), ["+N"]);
    }

    #[test]
    fn filter_limits_what_is_opened() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.plug(ModuleKind::Rollers2, "R", &[2]);
        let options = DeckOptions::default().filter(|i| i.kind == ModuleKind::Rollers2);
        let mut deck = bus.deck(options);
        assert_eq!(summary(&poll(&mut deck)), ["+R"]);
    }

    #[test]
    fn commands_reach_the_module() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);
        let id = deck.modules().next().unwrap().id.clone();
        deck.module(&id).unwrap().set_led_brightness(1, 40).unwrap();
        assert!(
            matches!(&knobs.sent()[..], [Sent::Output(r)] if r[..5] == [0x02, 0x2e, 0x02, 0x01, 0x28])
        );
        assert!(deck.module(&ModuleId("nobody".to_owned())).is_none());
    }

    #[test]
    fn rescans_at_the_configured_interval() {
        let bus = FakeBackend::default();
        let mut deck = Deck::with_backend(
            Box::new(bus.clone()),
            DeckOptions::default().rescan_interval(Duration::from_secs(3600)),
        );
        for _ in 0..5 {
            deck.poll(Duration::ZERO).unwrap();
        }
        assert_eq!(bus.0.lock().unwrap().scans, 1);
    }

    #[test]
    fn deck_can_move_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<Deck>();
    }
}
