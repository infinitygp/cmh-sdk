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

/// Identity of a module: its USB serial number, which stays the same
/// wherever the module is mounted and across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleId(String);

impl ModuleId {
    /// Builds an ID from a serial number, for example one stored earlier.
    pub fn new(serial: impl Into<String>) -> Self {
        Self(serial.into())
    }

    fn of(interface: &HidInterface) -> Self {
        // Every module seen so far has a serial. The path is a last resort
        // and not stable across reconnects.
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

impl From<&str> for ModuleId {
    fn from(serial: &str) -> Self {
        Self::new(serial)
    }
}

impl From<String> for ModuleId {
    fn from(serial: String) -> Self {
        Self::new(serial)
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
    /// Cells covered on the base. `None` while no base is known upstream of
    /// the module or where the platform hides the USB topology.
    pub placement: Option<Placement>,
}

/// Something that happened on the deck.
#[derive(Debug)]
#[non_exhaustive]
pub enum DeckEvent {
    /// A module was found, opened and prepared.
    Connected(ModuleInfo),
    /// A module was removed or stopped responding. A module that is moved
    /// to another place disconnects and connects again.
    Disconnected(ModuleInfo),
    /// The placement of an open module became known or changed without a
    /// reconnect, typically because its base was detected after it.
    Relocated(ModuleInfo),
    /// A control was operated. Faders report their positions and the base
    /// its orientation once right after [`Connected`](DeckEvent::Connected).
    #[non_exhaustive]
    Input {
        id: ModuleId,
        kind: ModuleKind,
        event: InputEvent,
    },
    /// A raw input report; only with [`DeckOptions::raw_reports`].
    #[non_exhaustive]
    RawInput {
        id: ModuleId,
        kind: ModuleKind,
        report: Vec<u8>,
    },
    /// An input report could not be decoded. The module stays usable.
    #[non_exhaustive]
    Undecodable {
        id: ModuleId,
        kind: ModuleKind,
        error: Error,
    },
    /// A listed module could not be opened. Reported once; the deck keeps
    /// retrying on every rescan. On Linux this can show up once right after
    /// plugging, before udev has applied the access rule.
    #[non_exhaustive]
    OpenFailed {
        id: ModuleId,
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

impl fmt::Debug for DeckOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeckOptions")
            .field("software_mode", &self.software_mode)
            .field("raw_reports", &self.raw_reports)
            .field("rescan", &self.rescan)
            .field("filter", &self.filter.is_some())
            .finish()
    }
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
    /// Shortest accepted rescan interval; a rescan walks the device list.
    pub const MIN_RESCAN_INTERVAL: Duration = Duration::from_millis(100);

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
    /// Default: once per second; never less than
    /// [`MIN_RESCAN_INTERVAL`](Self::MIN_RESCAN_INTERVAL).
    pub fn rescan_interval(mut self, interval: Duration) -> Self {
        self.rescan = interval.max(Self::MIN_RESCAN_INTERVAL);
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

impl fmt::Debug for Deck {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Deck")
            .field("options", &self.options)
            .field(
                "modules",
                &self.modules().map(|m| &m.id).collect::<Vec<_>>(),
            )
            .finish()
    }
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

    /// What is known about the open module with this ID.
    pub fn info(&self, id: &ModuleId) -> Option<&ModuleInfo> {
        self.modules().find(|info| info.id == *id)
    }

    /// The open module with this ID, for drawing, lighting and queries.
    /// Input stays with the deck: it arrives through [`poll`](Self::poll).
    ///
    /// If a command fails with [`Error::Disconnected`] simply carry on: the
    /// next poll reports the module as disconnected.
    pub fn module(&self, id: &ModuleId) -> Option<&Module> {
        self.slots
            .iter()
            .find(|s| s.info.id == *id)
            .map(|s| &s.module)
    }

    /// Waits up to `timeout` for something to happen and returns it.
    ///
    /// Returns as soon as there is at least one event; an empty list means
    /// the timeout elapsed. `Duration::ZERO` checks once without waiting,
    /// `Duration::MAX` waits for as long as it takes. Fails only when the
    /// HID backend itself does; problems with single modules are events.
    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<DeckEvent>> {
        // Pause between two passes over the modules. Reads themselves never
        // block: a blocking wait costs a whole timer tick per idle module on
        // Windows.
        const PAUSE: Duration = Duration::from_millis(2);
        const IDLE_PAUSE: Duration = Duration::from_millis(20);

        let deadline = Instant::now().checked_add(timeout);
        let mut events = Vec::new();
        loop {
            if self
                .last_scan
                .is_none_or(|t| t.elapsed() >= self.options.rescan)
            {
                self.last_scan = Some(Instant::now());
                self.rescan(&mut events)?;
            }
            self.read_all(&mut events);
            if !events.is_empty() {
                return Ok(events);
            }

            let mut pause = if self.slots.is_empty() {
                IDLE_PAUSE
            } else {
                PAUSE
            };
            if let Some(deadline) = deadline {
                let now = Instant::now();
                if now >= deadline {
                    return Ok(events);
                }
                pause = pause.min(deadline - now);
            }
            std::thread::sleep(pause);
        }
    }

    fn describe(listed: &[HidInterface], interface: &HidInterface) -> ModuleInfo {
        let mount_path = manager::mount_path(listed, interface);
        ModuleInfo {
            id: ModuleId::of(interface),
            kind: interface.kind,
            interface: interface.clone(),
            placement: mount_path
                .as_deref()
                .and_then(|path| placement::placement(interface.kind, path)),
            mount_path,
        }
    }

    fn rescan(&mut self, events: &mut Vec<DeckEvent>) -> Result<()> {
        let listed = self.backend.scan()?;
        let wanted: Vec<&HidInterface> = listed
            .iter()
            .filter(|i| self.options.filter.as_ref().is_none_or(|keep| keep(i)))
            .collect();
        let is_listed =
            |interface: &HidInterface| wanted.iter().any(|w| manager::same_module(w, interface));

        let (kept, gone): (Vec<Slot>, Vec<Slot>) = std::mem::take(&mut self.slots)
            .into_iter()
            .partition(|slot| is_listed(&slot.info.interface));
        self.slots = kept;
        events.extend(gone.into_iter().map(|s| DeckEvent::Disconnected(s.info)));
        self.failed.retain(|f| is_listed(f));

        for interface in wanted {
            let known = self
                .slots
                .iter_mut()
                .find(|s| manager::same_module(&s.info.interface, interface));
            if let Some(slot) = known {
                // Still the same module, but its base may have shown up or
                // gone in the meantime.
                let info = Self::describe(&listed, interface);
                let moved = (&info.mount_path, &info.placement)
                    != (&slot.info.mount_path, &slot.info.placement);
                slot.info = info;
                if moved {
                    events.push(DeckEvent::Relocated(slot.info.clone()));
                }
                continue;
            }
            let info = Self::describe(&listed, interface);
            match self.open(interface) {
                Ok((module, initial)) => {
                    self.failed.retain(|f| !manager::same_module(f, interface));
                    events.push(DeckEvent::Connected(info.clone()));
                    events.extend(initial.into_iter().map(|event| DeckEvent::Input {
                        id: info.id.clone(),
                        kind: info.kind,
                        event,
                    }));
                    self.slots.push(Slot { info, module });
                }
                Err(error) => {
                    if !self
                        .failed
                        .iter()
                        .any(|f| manager::same_module(f, interface))
                    {
                        self.failed.push(interface.clone());
                        events.push(DeckEvent::OpenFailed {
                            id: info.id,
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
            // Without the positions the module is still usable: the first
            // movement then reports all faders.
            if let Ok(positions) = module.fader_positions() {
                initial.extend(positions.iter().enumerate().map(|(index, value)| {
                    InputEvent::AnalogChanged {
                        index: index as u8,
                        value: *value,
                    }
                }));
            }
        }
        Ok((module, initial))
    }

    fn read_all(&mut self, events: &mut Vec<DeckEvent>) {
        // Enough to empty the report queue the HID backend keeps per device.
        const MAX_REPORTS_PER_PASS: usize = 64;

        let mut lost = Vec::new();
        for (index, slot) in self.slots.iter_mut().enumerate() {
            let (id, kind) = (&slot.info.id, slot.info.kind);
            for _ in 0..MAX_REPORTS_PER_PASS {
                let report = match slot.module.read_raw(Duration::ZERO) {
                    Ok(Some(report)) => report,
                    Ok(None) => break,
                    Err(_) => {
                        lost.push(index);
                        break;
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
                    Ok(decoded) => {
                        events.extend(decoded.into_iter().map(|event| DeckEvent::Input {
                            id: id.clone(),
                            kind,
                            event,
                        }))
                    }
                    Err(error) => events.push(DeckEvent::Undecodable {
                        id: id.clone(),
                        kind,
                        error,
                    }),
                }
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
        /// Plugs a module in and returns the handle that scripts it. As on
        /// Windows, the path depends on the serial only.
        fn plug(&self, kind: ModuleKind, serial: &str, chain: &[u8]) -> Mock {
            let mock = Mock::with_features(&[&[0x33, 0x05, 10, 20, 30, 40, 50]]);
            let path = CString::new(format!("path-{serial}")).unwrap();
            let mut bus = self.0.lock().unwrap();
            bus.listed
                .push(interface(kind, serial, path.clone(), chain));
            bus.transports.retain(|(p, _)| *p != path);
            bus.transports.push((path, mock.clone()));
            mock
        }

        fn unplug(&self, serial: &str) {
            let mut bus = self.0.lock().unwrap();
            bus.listed.retain(|i| i.serial.as_deref() != Some(serial));
        }

        fn deck(&self, mut options: DeckOptions) -> Deck {
            // Rescan on every poll so that tests need no waiting.
            options.rescan = Duration::ZERO;
            Deck::with_backend(Box::new(self.clone()), options)
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
            usb_bus: Some("bus".to_owned()),
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
                DeckEvent::Relocated(i) => format!("~{}", i.id),
                DeckEvent::Input { id, event, .. } => format!("{id}:{event:?}"),
                DeckEvent::RawInput { id, report, .. } => format!("{id}:raw{}", report.len()),
                DeckEvent::Undecodable { id, .. } => format!("{id}:undecodable"),
                DeckEvent::OpenFailed { id, .. } => format!("!{id}"),
            })
            .collect()
    }

    const KNOB_STEP: [u8; 9] = [0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0xff, 0xff, 0x01];

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
        assert_eq!(
            deck.info(&ModuleId::from("K")).map(|i| i.kind),
            Some(ModuleKind::Keys15)
        );
    }

    /// Everything a default deck sends on its own, for every module type.
    #[test]
    fn a_default_session_sends_only_the_preparation_commands() {
        let bus = FakeBackend::default();
        let mocks: Vec<(ModuleKind, Mock)> = ModuleKind::ALL
            .into_iter()
            .enumerate()
            .map(|(i, kind)| (kind, bus.plug(kind, &format!("S{i}"), &[i as u8 + 1])))
            .collect();
        let mut deck = bus.deck(DeckOptions::default());
        for _ in 0..3 {
            poll(&mut deck);
        }
        for (kind, mock) in mocks {
            let sent: Vec<(u8, u8, u8)> = mock
                .sent()
                .iter()
                .map(|s| match s {
                    Sent::Feature(r) => (r[0], r[1], r[2]),
                    Sent::FeatureRead(id) => (0, *id, 0),
                    Sent::Output(r) => panic!("{kind}: unexpected output report {r:02x?}"),
                })
                .collect();
            let expected: &[(u8, u8, u8)] = match kind {
                ModuleKind::Keys15 => &[(0x03, 0x3a, 0x01)],
                ModuleKind::DialDisplay => &[(0x03, 0x49, 0x01)],
                ModuleKind::Faders5 => &[(0, 0x33, 0)],
                _ => &[],
            };
            assert_eq!(sent, expected, "{kind}");
        }
    }

    #[test]
    fn passive_deck_writes_nothing() {
        let bus = FakeBackend::default();
        let keys = bus.plug(ModuleKind::Keys15, "K", &[1]);
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[2]);
        let faders = bus.plug(ModuleKind::Faders5, "F", &[3]);
        let mut deck = bus.deck(DeckOptions::default().software_mode(false));
        poll(&mut deck);
        assert!(keys.sent().is_empty());
        assert!(knobs.sent().is_empty());
        // Reading the fader positions is a query, not a write.
        assert_eq!(faders.sent(), [Sent::FeatureRead(0x33)]);
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
    fn faders_connect_even_if_their_positions_cannot_be_read() {
        let bus = FakeBackend::default();
        let faders = bus.plug(ModuleKind::Faders5, "F", &[1]);
        // A position outside 0..=100 makes the read fail.
        faders.0.lock().unwrap().features = vec![vec![0x33, 0x05, 101, 0, 0, 0, 0]];
        let mut deck = bus.deck(DeckOptions::default());
        assert_eq!(summary(&poll(&mut deck)), ["+F"]);
        // The first movement then reports every fader.
        faders.push_input(&[0x01, 0x2a, 0x07, 0x00, 0x02, 0x05, 1, 2, 3, 4, 5]);
        assert_eq!(poll(&mut deck).len(), 5);
    }

    #[test]
    fn delivers_input_and_optionally_raw_reports() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default().raw_reports(true));
        poll(&mut deck);

        knobs.push_input(&KNOB_STEP);
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
    fn one_poll_drains_a_burst_without_blocking_reads() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.plug(ModuleKind::Rollers2, "R", &[2]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        for _ in 0..10 {
            knobs.push_input(&KNOB_STEP);
        }
        assert_eq!(poll(&mut deck).len(), 10);
        // A blocking read on an idle module costs a timer tick on Windows.
        let timeouts = knobs.0.lock().unwrap().read_timeouts.clone();
        assert!(!timeouts.is_empty());
        assert!(timeouts.iter().all(|t| t.is_zero()));
    }

    #[test]
    fn poll_accepts_an_unbounded_timeout() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        knobs.push_input(&KNOB_STEP);
        // Returns with the first events instead of overflowing the deadline.
        assert!(!deck.poll(Duration::MAX).unwrap().is_empty());
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

        // Same serial and, as on Windows, the same path; only the port
        // changed between two scans.
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
    fn a_base_found_later_places_the_modules_already_open() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[7, 1, 3, 4]);
        let mut deck = bus.deck(DeckOptions::default());
        let events = poll(&mut deck);
        let DeckEvent::Connected(info) = &events[0] else {
            panic!()
        };
        assert_eq!(info.placement, None);

        bus.plug(ModuleKind::Base, "B", &[7, 1, 4, 4]);
        let events = poll(&mut deck);
        assert_eq!(summary(&events), ["~N", "+B"]);
        let DeckEvent::Relocated(info) = &events[0] else {
            panic!()
        };
        assert_eq!(info.placement.map(|p| (p.column, p.row)), Some((5, 3)));
        assert_eq!(
            deck.info(&ModuleId::from("N")).unwrap().placement,
            info.placement
        );
        // The module was not reopened for this.
        assert!(knobs.sent().is_empty());
        assert!(poll(&mut deck).is_empty());
    }

    #[test]
    fn changing_strings_do_not_reconnect_a_module() {
        let bus = FakeBackend::default();
        let keys = bus.plug(ModuleKind::Keys15, "K", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);
        assert_eq!(keys.sent().len(), 1);

        // The OS may fail to read a string descriptor on one enumeration, or
        // to work out the USB location.
        {
            let mut bus = bus.0.lock().unwrap();
            bus.listed[0].product = Some("15xKey w/ IPS, 4x3".to_owned());
            bus.listed[0].port_chain = None;
        }
        assert!(poll(&mut deck).is_empty());
        assert_eq!(keys.sent().len(), 1);
        assert_eq!(deck.modules().count(), 1);
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
        assert!(deck.module(&ModuleId::new("nobody")).is_none());
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
        // The builder refuses to rescan on every pass.
        let options = DeckOptions::default().rescan_interval(Duration::ZERO);
        assert_eq!(options.rescan, DeckOptions::MIN_RESCAN_INTERVAL);
    }

    #[test]
    fn deck_can_move_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<Deck>();
    }
}
