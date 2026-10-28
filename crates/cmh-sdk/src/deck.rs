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

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant};

use crate::devices::ModuleKind;
use crate::error::{Error, Result};
use crate::input::InputEvent;
use crate::manager::{self, Manager};
use crate::module::Module;
use crate::placement::{self, Placement};
use crate::protocol::InputMessage;
use crate::protocol::frame::INPUT_REPORT_LEN;
use crate::transport::{HidInterface, InputReader};
use crate::watch;

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
        /// When the report was read from the module, by the thread that
        /// waits for it, not when [`Deck::poll`] handed it over. The events
        /// of one report share the time. The positions and the orientation
        /// sent right after [`Connected`](DeckEvent::Connected) carry the
        /// time of connecting.
        at: Instant,
    },
    /// A raw input report; only with [`DeckOptions::raw_reports`].
    #[non_exhaustive]
    RawInput {
        id: ModuleId,
        kind: ModuleKind,
        report: Vec<u8>,
        /// When the report was read, as for [`Input`](DeckEvent::Input).
        at: Instant,
    },
    /// Reports of this module did not fit in the queue because
    /// [`Deck::poll`] was not called for a while (see
    /// [`Deck::MAX_PENDING_REPORTS`]), and `dropped` of them were not
    /// delivered one by one.
    ///
    /// The [`Input`](DeckEvent::Input) events that follow at once bring
    /// buttons, faders and the orientation up to the last state the module
    /// reported, so the picture of what is held and where the faders stand
    /// is right again; a press and release that both fell into the gap is
    /// not reported. Encoder steps in the gap are lost. The events of the
    /// module that came before this one are complete and in order. With
    /// [`DeckOptions::raw_reports`], only the replayed reports follow as
    /// [`RawInput`](DeckEvent::RawInput).
    #[non_exhaustive]
    InputLost {
        id: ModuleId,
        kind: ModuleKind,
        dropped: usize,
    },
    /// An input report could not be decoded. The module stays usable.
    #[non_exhaustive]
    Undecodable {
        id: ModuleId,
        kind: ModuleKind,
        error: Error,
        /// When the report was read, as for [`Input`](DeckEvent::Input).
        at: Instant,
    },
    /// A listed module could not be opened or, for a display module, did
    /// not confirm software mode. Reported once; the deck keeps retrying on
    /// every rescan and, with device events, whenever the system says the
    /// device list changed and once a second until the module opens or
    /// leaves the list. On Linux a failure that goes away within two
    /// seconds of the device appearing (udev has not applied the access rule
    /// yet) is not reported.
    #[non_exhaustive]
    OpenFailed {
        id: ModuleId,
        interface: HidInterface,
        error: Error,
    },
}

type Filter = Box<dyn Fn(&HidInterface) -> bool + Send>;

/// How often the device list is looked at when nothing announces changes.
const POLLING_RESCAN: Duration = Duration::from_secs(1);
/// How often it is looked at when the system announces changes: a safety
/// net for an announcement that was missed.
const SAFETY_RESCAN: Duration = Duration::from_secs(60);
/// How long to collect more announcements of one burst (a module that is
/// plugged in appears as several nodes, each changing twice) before the
/// device list is read.
const SETTLE: Duration = Duration::from_millis(25);
/// How long a module whose reader has ended waits before the device list is
/// read again. Bounds the pace of a module that connects and fails at once.
const RECONNECT_PAUSE: Duration = Duration::from_secs(1);
/// With device events a node is announced when it appears, usually before
/// udev has given the user access to it. An open that fails is tried again
/// after this long, for [`OPEN_RETRY_WINDOW`], before it is reported.
const OPEN_RETRY_PAUSE: Duration = Duration::from_millis(250);
const OPEN_RETRY_WINDOW: Duration = Duration::from_secs(2);
/// The longest a reader thread stays in one read. It looks at its stop flag
/// between reads, so this is also how long a thread outlives its deck; a
/// module that goes away ends the read at once.
#[cfg(windows)]
const READ_WAIT: Duration = Duration::from_millis(250);
#[cfg(not(windows))]
const READ_WAIT: Duration = Duration::from_secs(2);

/// Behaviour of a [`Deck`].
pub struct DeckOptions {
    software_mode: bool,
    raw_reports: bool,
    rescan: Option<Duration>,
    device_events: bool,
    filter: Option<Filter>,
}

impl fmt::Debug for DeckOptions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DeckOptions")
            .field("software_mode", &self.software_mode)
            .field("raw_reports", &self.raw_reports)
            .field("rescan", &self.rescan)
            .field("device_events", &self.device_events)
            .field("filter", &self.filter.is_some())
            .finish()
    }
}

impl Default for DeckOptions {
    fn default() -> Self {
        Self {
            software_mode: true,
            raw_reports: false,
            rescan: None,
            // The Windows source is written but not verified on hardware
            // yet (experiment 0029).
            device_events: cfg!(target_os = "linux"),
            filter: None,
        }
    }
}

impl DeckOptions {
    /// Shortest accepted rescan interval; a rescan walks the device list.
    pub const MIN_RESCAN_INTERVAL: Duration = Duration::from_millis(100);

    /// Whether display modules are switched to software mode when they
    /// connect, as the vendor application does before drawing. Default:
    /// `true`. With `false` the deck never writes to a module on its own;
    /// the keys and the dial then send no input, and their firmware version
    /// and backlight cannot be read until
    /// [`Module::set_software_mode`] is called on them.
    pub fn software_mode(mut self, enabled: bool) -> Self {
        self.software_mode = enabled;
        self
    }

    /// Also emit every input report as [`DeckEvent::RawInput`].
    pub fn raw_reports(mut self, enabled: bool) -> Self {
        self.raw_reports = enabled;
        self
    }

    /// How often the device list is read to find arrivals and departures,
    /// whatever else happens; never less than
    /// [`MIN_RESCAN_INTERVAL`](Self::MIN_RESCAN_INTERVAL).
    ///
    /// Without device events (see [`device_events`](Self::device_events))
    /// this is how late an arrival or a departure can be noticed. Default:
    /// once per second. With them it only guards against an announcement
    /// that never came, and the default is once per minute. If the system's
    /// announcements stop, the deck goes back to the timer, and a module that
    /// is listed but does not open is looked at at least once a second.
    pub fn rescan_interval(mut self, interval: Duration) -> Self {
        self.rescan = Some(interval.max(Self::MIN_RESCAN_INTERVAL));
        self
    }

    /// Whether the deck asks the operating system to announce arriving and
    /// leaving devices instead of reading the device list on a timer. The
    /// list is then read once at the start and after an announcement. Default:
    /// `true` on Linux, `false` elsewhere. If the system cannot provide the
    /// announcements, the deck falls back to the timer.
    ///
    /// On Linux the source is an inotify watch of `/dev`. On Windows it is a
    /// device interface notification for the HID class, which has not been
    /// verified on hardware yet (`docs/experiments/0029-idle-cost.md`).
    pub fn device_events(mut self, enabled: bool) -> Self {
        self.device_events = enabled;
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
    /// Opens a module and a second handle that only reads its input.
    fn open(&self, interface: &HidInterface) -> Result<(Module, Box<dyn InputReader>)>;
    /// Starts calling `changed` when the device list may have changed, and
    /// `ended` if the source stops by itself. Dropping the returned guard
    /// stops it. `None`: no such source.
    fn watch(
        &self,
        _changed: Box<dyn Fn() + Send + Sync>,
        _ended: Box<dyn FnOnce() + Send>,
    ) -> Option<Box<dyn Send>> {
        None
    }
}

impl Backend for Manager {
    fn scan(&mut self) -> Result<Vec<HidInterface>> {
        self.refresh()?;
        Ok(self.modules().to_vec())
    }

    fn open(&self, interface: &HidInterface) -> Result<(Module, Box<dyn InputReader>)> {
        self.open_with_reader(interface)
    }

    fn watch(
        &self,
        changed: Box<dyn Fn() + Send + Sync>,
        ended: Box<dyn FnOnce() + Send>,
    ) -> Option<Box<dyn Send>> {
        watch::start(changed, ended).map(|watch| Box::new(watch) as Box<dyn Send>)
    }
}

/// What the reader threads of a deck tell it, in the order it happened.
#[derive(Debug)]
enum Msg {
    /// A reader thread read a report.
    Report {
        slot: u64,
        at: Instant,
        report: Vec<u8>,
    },
    /// A reader thread stopped: its module is gone.
    ReaderEnded { slot: u64 },
}

/// The reports of a slot that did not fit in the queue.
#[derive(Debug)]
struct Lost {
    slot: u64,
    dropped: usize,
    /// The last report of each kind that carries a whole state, oldest
    /// first. Replaying them brings the decoder to the state the module
    /// reported last.
    states: Vec<(WholeState, Instant, Vec<u8>)>,
}

/// Reports that carry the state of every control of their kind, as
/// opposed to encoder steps, which are relative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WholeState {
    Buttons,
    Faders,
    Orientation,
}

impl WholeState {
    fn of(kind: ModuleKind, report: &[u8]) -> Option<Self> {
        match InputMessage::parse(kind, report) {
            Ok(InputMessage::ButtonStates(_)) => Some(Self::Buttons),
            Ok(InputMessage::FaderPositions(_)) => Some(Self::Faders),
            // The decoder ignores other values, so they must not replace
            // the last heartbeat that said something.
            Ok(InputMessage::Heartbeat { status: 0 | 1 }) => Some(Self::Orientation),
            _ => None,
        }
    }
}

/// Everything waiting for the next [`Deck::poll`]. Its size is bounded:
/// reports by [`Deck::MAX_PENDING_REPORTS`] per slot, departures by the
/// number of reader threads, and the announcements and wake-ups are flags
/// that a repetition does not add to.
#[derive(Debug)]
struct Queue {
    msgs: VecDeque<Msg>,
    /// Reports per slot in `msgs`.
    pending: Vec<(u64, usize)>,
    /// Slots that ran over, in the order they did. While a slot is here its
    /// reports are not queued, so that none of them overtakes the loss.
    lost: Vec<Lost>,
    capacity: usize,
    /// The system says the device list may have changed.
    devices_changed: bool,
    /// The source of those announcements stopped: nothing more will come.
    watch_ended: bool,
    /// A [`DeckWaker`] was used.
    woken: bool,
}

impl Queue {
    fn is_empty(&self) -> bool {
        self.msgs.is_empty()
            && self.lost.is_empty()
            && !(self.devices_changed || self.watch_ended || self.woken)
    }

    fn report(&mut self, slot: u64, kind: ModuleKind, at: Instant, report: Vec<u8>) {
        let pending = match self.pending.iter_mut().find(|(s, _)| *s == slot) {
            Some((_, count)) => count,
            None => {
                self.pending.push((slot, 0));
                &mut self.pending.last_mut().expect("just pushed").1
            }
        };
        let over = self.lost.iter().any(|l| l.slot == slot);
        if !over && *pending < self.capacity {
            *pending += 1;
            self.msgs.push_back(Msg::Report { slot, at, report });
            return;
        }
        let lost = match self.lost.iter().position(|l| l.slot == slot) {
            Some(index) => &mut self.lost[index],
            None => {
                self.lost.push(Lost {
                    slot,
                    dropped: 0,
                    states: Vec::new(),
                });
                self.lost.last_mut().expect("just pushed")
            }
        };
        lost.dropped = lost.dropped.saturating_add(1);
        // Encoder steps cannot be summed into one event of the same meaning,
        // so only whole states are kept.
        if let Some(state) = WholeState::of(kind, &report) {
            lost.states.retain(|(kept, ..)| *kept != state);
            lost.states.push((state, at, report));
        }
    }
}

/// What the threads of a deck share with it.
#[derive(Debug)]
struct Shared {
    queue: Mutex<Queue>,
    /// Signalled whenever the queue stops being empty.
    ready: Condvar,
}

impl Shared {
    fn new(capacity: usize) -> Self {
        Self {
            queue: Mutex::new(Queue {
                msgs: VecDeque::new(),
                pending: Vec::new(),
                lost: Vec::new(),
                capacity,
                devices_changed: false,
                watch_ended: false,
                woken: false,
            }),
            ready: Condvar::new(),
        }
    }

    /// The queue; a thread that panicked while holding it cannot have left
    /// it half changed.
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn update(&self, change: impl FnOnce(&mut Queue)) {
        change(&mut self.lock());
        self.ready.notify_one();
    }
}

/// Wakes a [`Deck::poll`] from another thread, so that its caller can look
/// at something else the moment it happens instead of when the poll times
/// out. Obtained from [`Deck::waker`].
#[derive(Debug, Clone)]
pub struct DeckWaker(Weak<Shared>);

impl DeckWaker {
    /// Makes the poll that is waiting, or if none is, the next one, return
    /// at once with whatever it has, which may be nothing. Wake-ups before
    /// that poll count as one. Does nothing once the deck is gone.
    pub fn wake(&self) {
        if let Some(shared) = self.0.upgrade() {
            shared.update(|queue| queue.woken = true);
        }
    }
}

/// Lets a reader thread end when the slot is dropped.
struct StopFlag(Arc<AtomicBool>);

impl Drop for StopFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct Slot {
    /// Tells this slot's reports from those of an earlier one for the same
    /// module.
    id: u64,
    info: ModuleInfo,
    module: Module,
    _reader: StopFlag,
}

/// Reads reports from `reader` until its module is gone or the flag is set,
/// and passes them on with the time they were read. Never waits for the
/// deck: what does not fit is counted as lost.
fn spawn_reader(
    mut reader: Box<dyn InputReader>,
    slot: u64,
    kind: ModuleKind,
    to: Arc<Shared>,
) -> Result<StopFlag> {
    let stop = Arc::new(AtomicBool::new(false));
    let flag = stop.clone();
    std::thread::Builder::new()
        .name("cmh-reader".to_owned())
        .spawn(move || {
            let mut buf = [0u8; INPUT_REPORT_LEN];
            while !flag.load(Ordering::Relaxed) {
                // hidapi's Linux read fails when its poll is interrupted by a
                // signal, which is no reason to give the module up. A second
                // read without waiting fails again if the module is gone.
                let read = reader
                    .read_input(&mut buf, READ_WAIT)
                    .or_else(|_| reader.read_input(&mut buf, Duration::ZERO));
                // The slot is gone; nothing it read belongs anywhere.
                if flag.load(Ordering::Relaxed) {
                    return;
                }
                match read {
                    Ok(0) => {}
                    Ok(len) => {
                        let at = Instant::now();
                        let report = buf[..len.min(INPUT_REPORT_LEN)].to_vec();
                        to.update(|queue| queue.report(slot, kind, at, report));
                    }
                    Err(_) => {
                        to.update(|queue| queue.msgs.push_back(Msg::ReaderEnded { slot }));
                        return;
                    }
                }
            }
        })
        .map_err(|err| Error::Backend(format!("cannot start a reader thread: {err}")))?;
    Ok(StopFlag(stop))
}

/// `now` plus `by`, without overflowing for an interval of "never".
fn after(by: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(by)
        .unwrap_or_else(|| now + Duration::from_secs(365 * 24 * 3600))
}

/// Everything that is connected, opened and polled together.
///
/// A deck is meant to live on one thread: call [`poll`](Self::poll) in a
/// loop and send commands through [`module`](Self::module) between polls.
/// Input is read by a thread per open module, which waits in the read and
/// costs nothing while a module is quiet. Reports wait for the next poll in
/// a queue of bounded size: a module that sends more than
/// [`MAX_PENDING_REPORTS`](Self::MAX_PENDING_REPORTS) reports between two
/// polls loses the excess, which is reported as
/// [`DeckEvent::InputLost`]. A reader thread never waits for the poll, and
/// ends within a few seconds after its module is closed or the deck is
/// dropped.
pub struct Deck {
    backend: Box<dyn Backend>,
    options: DeckOptions,
    slots: Vec<Slot>,
    /// Modules whose open error has been reported and not yet resolved.
    failed: Vec<HidInterface>,
    /// Modules whose open failed, with the time of the first failure, while
    /// the error is held back (device events only).
    retrying: Vec<(HidInterface, Instant)>,
    /// When the device list is read next; `None`: at the next poll.
    next_scan: Option<Instant>,
    /// Set while the system announces device changes.
    watch: Option<Box<dyn Send>>,
    woken: bool,
    next_slot: u64,
    shared: Arc<Shared>,
    /// Events of a poll that failed, for the next one.
    carried: Vec<DeckEvent>,
    /// Passes of the loop in `poll`, for the tests that check it does not spin.
    #[cfg(test)]
    passes: usize,
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
    /// How many input reports of one module may wait for the next
    /// [`poll`](Self::poll). A fader that is moved sends about 60 reports a
    /// second (`fixtures/inputs/faders.txt`), so this covers several seconds
    /// of continuous operation.
    pub const MAX_PENDING_REPORTS: usize = 256;

    /// Creates a deck with default options. Modules show up as
    /// [`DeckEvent::Connected`] from the first [`poll`](Self::poll).
    pub fn new() -> Result<Self> {
        Self::with_options(DeckOptions::default())
    }

    pub fn with_options(options: DeckOptions) -> Result<Self> {
        Ok(Self::with_backend(Box::new(Manager::new()?), options))
    }

    pub(crate) fn with_backend(backend: Box<dyn Backend>, options: DeckOptions) -> Self {
        let shared = Arc::new(Shared::new(Self::MAX_PENDING_REPORTS));
        // Before the first scan, so that nothing between the two is missed.
        let watch = options.device_events.then(|| {
            let (changed, ended) = (shared.clone(), shared.clone());
            backend.watch(
                Box::new(move || changed.update(|queue| queue.devices_changed = true)),
                Box::new(move || ended.update(|queue| queue.watch_ended = true)),
            )
        });
        Self {
            backend,
            options,
            slots: Vec::new(),
            failed: Vec::new(),
            retrying: Vec::new(),
            next_scan: None,
            watch: watch.flatten(),
            woken: false,
            next_slot: 0,
            shared,
            carried: Vec::new(),
            #[cfg(test)]
            passes: 0,
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

    /// A handle that makes [`poll`](Self::poll) return early, for a caller
    /// that also waits for something else and has to be woken by it.
    pub fn waker(&self) -> DeckWaker {
        DeckWaker(Arc::downgrade(&self.shared))
    }

    /// Waits up to `timeout` for something to happen and returns it.
    ///
    /// Returns as soon as there is at least one event; an empty list means
    /// the timeout elapsed or a [`DeckWaker`] was used. `Duration::ZERO`
    /// checks once without waiting, `Duration::MAX` waits for as long as it
    /// takes. Fails only when the HID backend itself does; problems with
    /// single modules are events.
    ///
    /// A failure to read the device list loses nothing: the events gathered
    /// before it, including what a partly done rescan found, are returned
    /// first by the next poll, and the list is read again within a second.
    ///
    /// The thread sleeps until a report, a device change, the next rescan or
    /// the timeout, whichever is first.
    pub fn poll(&mut self, timeout: Duration) -> Result<Vec<DeckEvent>> {
        let deadline = Instant::now().checked_add(timeout);
        let mut events = std::mem::take(&mut self.carried);
        loop {
            #[cfg(test)]
            {
                self.passes += 1;
            }
            // Before the rescan: a module that leaves hands over what it
            // sent last.
            self.drain(&mut events);
            if self.next_scan.is_none_or(|t| t <= Instant::now()) {
                self.next_scan = Some(after(self.rescan_interval()));
                if let Err(error) = self.rescan(&mut events) {
                    // The events were taken from the queue, and a rescan
                    // that stopped half way may have opened or dropped
                    // modules already.
                    self.carried = events;
                    self.scan_within(POLLING_RESCAN);
                    return Err(error);
                }
            }
            if self.woken || !events.is_empty() {
                self.woken = false;
                return Ok(events);
            }

            let now = Instant::now();
            let mut wait = self
                .next_scan
                .map_or(Duration::ZERO, |t| t.saturating_duration_since(now));
            if let Some(deadline) = deadline {
                if now >= deadline {
                    return Ok(events);
                }
                wait = wait.min(deadline - now);
            }
            // Whatever arrived is taken by the drain at the top.
            let queue = self.shared.lock();
            drop(
                self.shared
                    .ready
                    .wait_timeout_while(queue, wait, |queue| queue.is_empty())
                    .unwrap_or_else(PoisonError::into_inner),
            );
        }
    }

    fn rescan_interval(&self) -> Duration {
        self.options.rescan.unwrap_or(if self.watch.is_some() {
            SAFETY_RESCAN
        } else {
            POLLING_RESCAN
        })
    }

    /// Brings the next read of the device list forward to `pause` from now.
    fn scan_within(&mut self, pause: Duration) {
        let due = after(pause);
        self.next_scan = self.next_scan.map(|t| t.min(due));
    }

    /// For a module that is listed but not open. With announcements nothing
    /// says when whatever stopped it is over, and the safety rescan is a
    /// minute away, so it is tried again at the pace the timer would have.
    fn retry_soon(&mut self) {
        if self.watch.is_some() {
            self.scan_within(POLLING_RESCAN);
        }
    }

    /// Takes everything that is waiting. A slot's loss comes after the
    /// reports it queued before it ran over, and before its departure.
    fn drain(&mut self, events: &mut Vec<DeckEvent>) {
        let (msgs, mut lost, changed, ended, woken) = {
            let mut queue = self.shared.lock();
            queue.pending.clear();
            (
                std::mem::take(&mut queue.msgs),
                std::mem::take(&mut queue.lost),
                std::mem::take(&mut queue.devices_changed),
                std::mem::take(&mut queue.watch_ended),
                std::mem::take(&mut queue.woken),
            )
        };
        self.woken |= woken;
        if changed {
            self.scan_within(SETTLE);
        }
        // Nothing announces changes any more, so the timer has to find them,
        // starting with whatever went unseen meanwhile.
        if ended && self.watch.take().is_some() {
            self.scan_within(Duration::ZERO);
        }
        for msg in msgs {
            match msg {
                Msg::Report { slot, at, report } => self.report(slot, at, &report, events),
                Msg::ReaderEnded { slot } => {
                    if let Some(index) = lost.iter().position(|l| l.slot == slot) {
                        self.recover(lost.remove(index), events);
                    }
                    // Already gone if a rescan was first.
                    if let Some(index) = self.slots.iter().position(|s| s.id == slot) {
                        // A module that is still (or again) listed is
                        // reopened by the rescan and announced with a fresh
                        // `Connected`.
                        events.push(DeckEvent::Disconnected(self.slots.remove(index).info));
                        self.scan_within(RECONNECT_PAUSE);
                    }
                }
            }
        }
        for lost in lost {
            self.recover(lost, events);
        }
    }

    /// Reports a loss and replays the states that were kept.
    fn recover(&mut self, lost: Lost, events: &mut Vec<DeckEvent>) {
        let Some(slot) = self.slots.iter().find(|s| s.id == lost.slot) else {
            return;
        };
        events.push(DeckEvent::InputLost {
            id: slot.info.id.clone(),
            kind: slot.info.kind,
            dropped: lost.dropped,
        });
        for (_, at, report) in lost.states {
            self.report(lost.slot, at, &report, events);
        }
    }

    fn report(&mut self, slot: u64, at: Instant, report: &[u8], events: &mut Vec<DeckEvent>) {
        // Nothing to do for a slot that was dropped meanwhile: its reports
        // never reach a later session of the same module.
        let Some(slot) = self.slots.iter_mut().find(|s| s.id == slot) else {
            return;
        };
        let (id, kind) = (&slot.info.id, slot.info.kind);
        if self.options.raw_reports {
            events.push(DeckEvent::RawInput {
                id: id.clone(),
                kind,
                report: report.to_vec(),
                at,
            });
        }
        match slot.module.decode(report) {
            Ok(decoded) => events.extend(decoded.into_iter().map(|event| DeckEvent::Input {
                id: id.clone(),
                kind,
                event,
                at,
            })),
            Err(error) => events.push(DeckEvent::Undecodable {
                id: id.clone(),
                kind,
                error,
                at,
            }),
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
        // What a module sent while the list was read comes before what the
        // list says about the module.
        self.drain(events);
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
        self.retrying.retain(|(r, _)| is_listed(r));

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
            let mut info = Self::describe(&listed, interface);
            match self.open(interface) {
                Ok((module, reader, initial)) => {
                    if self.options.software_mode && info.kind.capabilities().display.is_some() {
                        match self.relisted(interface)? {
                            Some(current) => info = current,
                            // Gone again already, or not wanted any more; the
                            // next rescan sorts it out.
                            None => {
                                self.retry_soon();
                                continue;
                            }
                        }
                    }
                    let id = self.next_slot;
                    match spawn_reader(reader, id, info.kind, self.shared.clone()) {
                        Ok(reader) => {
                            self.next_slot += 1;
                            self.failed.retain(|f| !manager::same_module(f, interface));
                            self.retrying
                                .retain(|(r, _)| !manager::same_module(r, interface));
                            events.push(DeckEvent::Connected(info.clone()));
                            let at = Instant::now();
                            events.extend(initial.into_iter().map(|event| DeckEvent::Input {
                                id: info.id.clone(),
                                kind: info.kind,
                                event,
                                at,
                            }));
                            self.slots.push(Slot {
                                id,
                                info,
                                module,
                                _reader: reader,
                            });
                        }
                        Err(error) => self.open_failed(interface, info.id, error, events),
                    }
                }
                Err(error) => self.open_failed(interface, info.id, error, events),
            }
        }
        Ok(())
    }

    /// Reports a failed open once, unless it may go away by itself.
    fn open_failed(
        &mut self,
        interface: &HidInterface,
        id: ModuleId,
        error: Error,
        events: &mut Vec<DeckEvent>,
    ) {
        self.retry_soon();
        if self
            .failed
            .iter()
            .any(|f| manager::same_module(f, interface))
        {
            return;
        }
        // With device events the open is tried while the node is still new,
        // usually before udev has applied the access rule, so a failure is
        // held back for a short while. Without them the next timer rescan
        // is the retry.
        if self.watch.is_some() {
            let first = match self
                .retrying
                .iter()
                .find(|(r, _)| manager::same_module(r, interface))
            {
                Some((_, first)) => *first,
                None => {
                    let now = Instant::now();
                    self.retrying.push((interface.clone(), now));
                    now
                }
            };
            if first.elapsed() < OPEN_RETRY_WINDOW {
                self.scan_within(OPEN_RETRY_PAUSE);
                return;
            }
            self.retrying
                .retain(|(r, _)| !manager::same_module(r, interface));
        }
        self.failed.push(interface.clone());
        events.push(DeckEvent::OpenFailed {
            id,
            interface: interface.clone(),
            error,
        });
    }

    /// Describes a display module from a fresh listing, taken after software
    /// mode was confirmed.
    ///
    /// A display module that was asked for its firmware in standalone mode
    /// answers string requests out of step until then, and Windows lists it
    /// under a wrong serial for that long (experiment 0024). Its identity
    /// is therefore taken from a listing made afterwards. `None` if the
    /// module is no longer there or no longer wanted.
    fn relisted(&mut self, opened: &HidInterface) -> Result<Option<ModuleInfo>> {
        let listed = self.backend.scan()?;
        // As in `manager::same_module`, but without the serial; a location
        // that is unknown on either side is not held against the match.
        let same_place = |i: &&HidInterface| {
            let located = match (&i.port_chain, &opened.port_chain) {
                (Some(x), Some(y)) => x == y && i.usb_bus == opened.usb_bus,
                _ => true,
            };
            i.path == opened.path && i.product_id == opened.product_id && located
        };
        Ok(listed
            .iter()
            .find(same_place)
            .filter(|i| self.options.filter.as_ref().is_none_or(|keep| keep(i)))
            .map(|current| Self::describe(&listed, current)))
    }

    /// Opens and prepares a module; returns it with the handle its input is
    /// read on and the events that describe its initial state.
    fn open(
        &self,
        interface: &HidInterface,
    ) -> Result<(Module, Box<dyn InputReader>, Vec<InputEvent>)> {
        let (mut module, reader) = self.backend.open(interface)?;
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
        Ok((module, reader, initial))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::CString;
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::testing::{DIAL_FIRMWARE, KEYS_FIRMWARE, Mock, Sent};

    type OnChange = Arc<dyn Fn() + Send + Sync>;

    /// What the fake bus currently lists, and the transport behind each path.
    #[derive(Default)]
    struct Bus {
        listed: Vec<HidInterface>,
        transports: Vec<(CString, Mock)>,
        refuse: Vec<CString>,
        scans: usize,
        /// Serial listed instead of the real one while the module behind
        /// the path is in the state an empty read leaves (experiment 0024).
        garbled: Vec<(CString, String)>,
        /// Numbers of the scans (counting from 1) that fail.
        failing: Vec<usize>,
        /// Whether the bus announces changes, and whom to tell.
        events: bool,
        on_change: Option<OnChange>,
        on_end: Option<Box<dyn FnOnce() + Send>>,
    }

    #[derive(Clone, Default)]
    struct FakeBackend(Arc<Mutex<Bus>>);

    impl Backend for FakeBackend {
        fn scan(&mut self) -> Result<Vec<HidInterface>> {
            let mut bus = self.0.lock().unwrap();
            bus.scans += 1;
            if bus.failing.contains(&bus.scans) {
                return Err(Error::Backend("scripted enumeration failure".to_owned()));
            }
            let mut listed = bus.listed.clone();
            for (path, wrong) in &bus.garbled {
                let lagging = bus
                    .transports
                    .iter()
                    .any(|(p, mock)| p == path && mock.0.lock().unwrap().lagging);
                let entry = listed.iter_mut().find(|i| i.path == *path);
                if let (true, Some(entry)) = (lagging, entry) {
                    entry.serial = Some(wrong.clone());
                }
            }
            Ok(listed)
        }

        fn watch(
            &self,
            changed: Box<dyn Fn() + Send + Sync>,
            ended: Box<dyn FnOnce() + Send>,
        ) -> Option<Box<dyn Send>> {
            let mut bus = self.0.lock().unwrap();
            bus.events.then(|| {
                bus.on_change = Some(changed.into());
                bus.on_end = Some(ended);
                Box::new(()) as Box<dyn Send>
            })
        }

        fn open(&self, interface: &HidInterface) -> Result<(Module, Box<dyn InputReader>)> {
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
            Ok((
                Module::new(
                    interface.kind,
                    interface.serial.clone(),
                    Box::new(mock.clone()),
                ),
                Box::new(mock.reader()),
            ))
        }
    }

    impl FakeBackend {
        /// Plugs a module in and returns the handle that scripts it. As on
        /// Windows, the path depends on the serial only.
        fn plug(&self, kind: ModuleKind, serial: &str, chain: &[u8]) -> Mock {
            let mock = Mock::with_features(&[
                &[0x33, 0x05, 10, 20, 30, 40, 50],
                KEYS_FIRMWARE,
                DIAL_FIRMWARE,
            ]);
            // Display modules start in standalone mode.
            mock.0.lock().unwrap().standalone = kind.capabilities().display.is_some();
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

        /// A deck that rescans on every poll, so that tests need no waiting.
        /// The bus announces nothing, as on a platform without events.
        fn deck(&self, mut options: DeckOptions) -> Deck {
            options.rescan = Some(Duration::ZERO);
            options.device_events = false;
            Deck::with_backend(Box::new(self.clone()), options)
        }

        /// A deck on a bus that announces changes (see [`Self::changed`]).
        fn event_deck(&self, options: DeckOptions) -> Deck {
            self.0.lock().unwrap().events = true;
            Deck::with_backend(Box::new(self.clone()), options.device_events(true))
        }

        /// The system says the device list changed.
        fn changed(&self) {
            let on_change = self.0.lock().unwrap().on_change.clone();
            (on_change.expect("no deck listens"))();
        }

        /// The source of the announcements dies.
        fn watch_ended(&self) {
            let on_end = self.0.lock().unwrap().on_end.take();
            (on_end.expect("no deck listens"))();
        }

        fn scans(&self) -> usize {
            self.0.lock().unwrap().scans
        }

        /// Makes the next `count` scans fail.
        fn fail_next(&self, count: usize) {
            let mut bus = self.0.lock().unwrap();
            let next = bus.scans + 1;
            bus.failing.extend(next..next + count);
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
                DeckEvent::InputLost { id, dropped, .. } => format!("{id}:lost{dropped}"),
            })
            .collect()
    }

    /// Waits until the reader of a pulled module has let go of it, which it
    /// does after telling the deck that it ends.
    fn reader_ended(mock: &Mock) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while mock.0.lock().unwrap().readers > 0 {
            assert!(Instant::now() < deadline, "the reader did not end");
            std::thread::sleep(Duration::from_millis(5));
        }
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
        // Display modules are switched to software mode, which is confirmed
        // by a firmware read; nothing else is sent.
        let mut mode = vec![0u8; 32];
        mode[..3].copy_from_slice(&[0x03, 0x3a, 0x01]);
        assert_eq!(
            keys.sent(),
            [
                Sent::Feature(mode.clone()),
                Sent::Feature(mode),
                Sent::FeatureRead(0x38)
            ]
        );
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
                ModuleKind::Keys15 => &[(0x03, 0x3a, 0x01), (0x03, 0x3a, 0x01), (0, 0x38, 0)],
                ModuleKind::DialDisplay => &[(0x03, 0x49, 0x01), (0x03, 0x49, 0x01), (0, 0x48, 0)],
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
        faders.settle();
        assert_eq!(poll(&mut deck).len(), 5);
    }

    #[test]
    fn delivers_input_and_optionally_raw_reports() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default().raw_reports(true));
        poll(&mut deck);

        knobs.push_input(&KNOB_STEP);
        knobs.settle();
        assert_eq!(
            summary(&poll(&mut deck)),
            ["N:raw64", "N:EncoderDelta { index: 2, delta: 1 }"]
        );
        // A report that cannot be decoded does not cost the module.
        knobs.push_input(&[0x01, 0x2a, 0x05, 0x00, 0x00, 0x03, 0x09, 0xff, 0xff]);
        knobs.settle();
        assert_eq!(summary(&poll(&mut deck)), ["N:raw64", "N:undecodable"]);
        assert_eq!(deck.modules().count(), 1);
        assert!(poll(&mut deck).is_empty());
    }

    /// A report that moves knob `index` one step: the other two do not.
    fn knob_step(index: usize) -> [u8; 9] {
        let mut report = KNOB_STEP;
        report[6..9].copy_from_slice(&[0xff; 3]);
        report[6 + index] = 0x01;
        report
    }

    #[test]
    fn one_poll_hands_over_a_burst_in_the_order_it_was_sent() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.plug(ModuleKind::Rollers2, "R", &[2]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        let order = [0, 1, 2, 2, 1, 0, 1, 2, 0, 0];
        for index in order {
            knobs.push_input(&knob_step(index));
        }
        knobs.settle();
        let events = poll(&mut deck);
        let seen: Vec<usize> = events
            .iter()
            .map(|e| match e {
                DeckEvent::Input {
                    event: InputEvent::EncoderDelta { index, .. },
                    ..
                } => *index as usize,
                other => panic!("unexpected {other:?}"),
            })
            .collect();
        assert_eq!(seen, order);
        // Each read happens before the next one.
        let times: Vec<Instant> = events
            .iter()
            .map(|e| match e {
                DeckEvent::Input { at, .. } => *at,
                _ => unreachable!(),
            })
            .collect();
        assert!(times.windows(2).all(|pair| pair[0] <= pair[1]));
    }

    #[test]
    fn a_report_carries_the_time_it_was_read_not_the_time_of_the_poll() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default().raw_reports(true));
        poll(&mut deck);

        let before = Instant::now();
        knobs.push_input(&KNOB_STEP);
        knobs.settle();
        let settled = Instant::now();
        std::thread::sleep(Duration::from_millis(60));
        let events = poll(&mut deck);
        assert_eq!(events.len(), 2);
        for event in &events {
            let (DeckEvent::Input { at, .. } | DeckEvent::RawInput { at, .. }) = event else {
                panic!("unexpected {event:?}");
            };
            assert!(*at >= before && *at <= settled, "{:?}", at.elapsed());
        }
    }

    #[test]
    fn readers_wait_in_blocking_reads() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);
        knobs.settle();
        // A read that returns at once on an idle module would be polling.
        let timeouts = knobs.0.lock().unwrap().read_timeouts.clone();
        assert_eq!(timeouts.len(), 1);
        assert!(timeouts[0] >= Duration::from_millis(100));
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

        // The handle dies first; the bus still lists the module. The reader
        // ends and reports it, and the rescan that is due on this poll tries
        // to open the module again.
        knobs.unplug();
        reader_ended(&knobs);
        assert_eq!(summary(&poll(&mut deck)), ["-N", "!N"]);
        // Still unusable: reported once, then retried silently.
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
        assert_eq!(keys.sent().len(), 3);

        // The OS may fail to read a string descriptor on one enumeration, or
        // to work out the USB location.
        {
            let mut bus = bus.0.lock().unwrap();
            bus.listed[0].product = Some("15xKey w/ IPS, 4x3".to_owned());
            bus.listed[0].port_chain = None;
        }
        assert!(poll(&mut deck).is_empty());
        assert_eq!(keys.sent().len(), 3);
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
    fn a_display_listed_under_a_wrong_serial_connects_under_its_own() {
        let bus = FakeBackend::default();
        let keys = bus.plug(ModuleKind::Keys15, "K", &[1]);
        // Other software read the firmware in standalone mode.
        keys.0.lock().unwrap().lagging = true;
        bus.0.lock().unwrap().garbled =
            vec![(CString::new("path-K").unwrap(), "Key w/ IPS".to_owned())];
        let mut deck = bus.deck(DeckOptions::default());

        assert_eq!(summary(&poll(&mut deck)), ["+K"]);
        assert!(poll(&mut deck).is_empty());
        assert_eq!(deck.modules().count(), 1);
        assert_eq!(keys.sent().len(), 3);

        // A deck that leaves the mode alone cannot repair the module and
        // reports what the system lists.
        let other = FakeBackend::default();
        let dial = other.plug(ModuleKind::DialDisplay, "D", &[1]);
        dial.0.lock().unwrap().lagging = true;
        other.0.lock().unwrap().garbled =
            vec![(CString::new("path-D").unwrap(), "junk".to_owned())];
        let mut deck = other.deck(DeckOptions::default().software_mode(false));
        assert_eq!(summary(&poll(&mut deck)), ["+junk"]);
    }

    #[test]
    fn a_display_that_is_not_relisted_is_connected_later() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Keys15, "K", &[1]);
        // Turned down on the second look only, the one after opening.
        let looks = AtomicUsize::new(0);
        let options =
            DeckOptions::default().filter(move |_| looks.fetch_add(1, Ordering::Relaxed) != 1);
        let mut deck = bus.deck(options);

        assert!(poll(&mut deck).is_empty());
        assert_eq!(deck.modules().count(), 0);
        assert_eq!(summary(&poll(&mut deck)), ["+K"]);
    }

    #[test]
    fn a_display_that_stays_in_standalone_mode_is_not_connected() {
        let bus = FakeBackend::default();
        let dial = bus.plug(ModuleKind::DialDisplay, "D", &[1]);
        dial.0.lock().unwrap().stuck = true;
        let mut deck = bus.deck(DeckOptions::default());

        assert_eq!(summary(&poll(&mut deck)), ["!D"]);
        assert!(poll(&mut deck).is_empty());
        assert!(deck.module(&ModuleId::from("D")).is_none());
        dial.0.lock().unwrap().stuck = false;
        assert_eq!(summary(&poll(&mut deck)), ["+D"]);
        let dial = deck.module(&ModuleId::from("D")).unwrap();
        assert_eq!(dial.firmware().unwrap().version, "01.00.04.00");
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
            DeckOptions::default()
                .device_events(false)
                .rescan_interval(Duration::from_secs(3600)),
        );
        for _ in 0..5 {
            deck.poll(Duration::ZERO).unwrap();
        }
        assert_eq!(bus.scans(), 1);
        // The builder refuses to rescan on every pass.
        let options = DeckOptions::default().rescan_interval(Duration::ZERO);
        assert_eq!(options.rescan, Some(DeckOptions::MIN_RESCAN_INTERVAL));
    }

    const SECOND: Duration = Duration::from_secs(1);

    #[test]
    fn the_default_rescan_depends_on_whether_the_system_announces_changes() {
        let bus = FakeBackend::default();
        let polling = Deck::with_backend(Box::new(bus.clone()), DeckOptions::default());
        // No source on this bus, so the timer has to do the work.
        assert_eq!(polling.rescan_interval(), POLLING_RESCAN);
        let events = bus.event_deck(DeckOptions::default());
        assert_eq!(events.rescan_interval(), SAFETY_RESCAN);
        // What was asked for wins in both cases.
        let asked = DeckOptions::default().rescan_interval(Duration::from_secs(7));
        assert_eq!(
            bus.event_deck(asked).rescan_interval(),
            Duration::from_secs(7)
        );
        // Events can be turned off.
        let bus = FakeBackend::default();
        bus.0.lock().unwrap().events = true;
        let off = Deck::with_backend(
            Box::new(bus.clone()),
            DeckOptions::default().device_events(false),
        );
        assert!(bus.0.lock().unwrap().on_change.is_none());
        assert_eq!(off.rescan_interval(), POLLING_RESCAN);
    }

    #[test]
    fn modules_are_found_on_an_announcement_and_not_on_a_timer() {
        let bus = FakeBackend::default();
        let mut deck = bus.event_deck(DeckOptions::default());
        // The list is read once at the start.
        assert!(poll(&mut deck).is_empty());
        assert_eq!(bus.scans(), 1);

        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        // Nothing announced it: a quiet wait finds nothing and reads nothing.
        assert!(deck.poll(Duration::from_millis(300)).unwrap().is_empty());
        assert_eq!(bus.scans(), 1);

        // A burst of announcements is one read, soon after the first.
        let started = Instant::now();
        for _ in 0..10 {
            bus.changed();
        }
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["+N"]);
        assert!(started.elapsed() < SECOND, "{:?}", started.elapsed());
        assert_eq!(bus.scans(), 2);
        assert!(deck.poll(Duration::from_millis(100)).unwrap().is_empty());
        assert_eq!(bus.scans(), 2);

        bus.unplug("N");
        bus.changed();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["-N"]);
        assert_eq!(bus.scans(), 3);
    }

    #[test]
    fn the_safety_rescan_finds_what_no_announcement_told_of() {
        let bus = FakeBackend::default();
        let options = DeckOptions::default().rescan_interval(Duration::from_millis(200));
        let mut deck = bus.event_deck(options);
        assert!(poll(&mut deck).is_empty());

        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let started = Instant::now();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["+N"]);
        let waited = started.elapsed();
        assert!(waited >= Duration::from_millis(150), "{waited:?}");
        assert!(waited < 2 * SECOND, "{waited:?}");
    }

    #[test]
    fn an_idle_deck_sleeps_instead_of_polling() {
        let bus = FakeBackend::default();
        let mocks: Vec<Mock> = [
            (ModuleKind::Base, "B"),
            (ModuleKind::Keys15, "K"),
            (ModuleKind::Knobs3, "N"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (kind, serial))| bus.plug(kind, serial, &[i as u8 + 1]))
        .collect();
        let mut deck = bus.event_deck(DeckOptions::default());
        assert_eq!(poll(&mut deck).len(), 3);
        for mock in &mocks {
            mock.settle();
        }
        let (passes, scans) = (deck.passes, bus.scans());

        assert!(deck.poll(Duration::from_millis(500)).unwrap().is_empty());
        // One pass to start, one when the time is up.
        assert!(deck.passes - passes <= 2, "{} passes", deck.passes - passes);
        assert_eq!(bus.scans(), scans);
        for mock in &mocks {
            // Each reader sits in the one read it started with.
            assert_eq!(mock.0.lock().unwrap().read_timeouts.len(), 1);
        }
    }

    #[test]
    fn input_wakes_a_waiting_poll_at_once() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.event_deck(DeckOptions::default());
        poll(&mut deck);
        knobs.settle();

        let pusher = {
            let knobs = knobs.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                knobs.push_input(&KNOB_STEP);
            })
        };
        let started = Instant::now();
        let events = deck.poll(10 * SECOND).unwrap();
        pusher.join().unwrap();
        assert_eq!(summary(&events), ["N:EncoderDelta { index: 2, delta: 1 }"]);
        assert!(started.elapsed() < 2 * SECOND, "{:?}", started.elapsed());
    }

    #[test]
    fn a_waker_ends_a_wait_and_is_not_forgotten() {
        let bus = FakeBackend::default();
        let mut deck = bus.event_deck(DeckOptions::default());
        poll(&mut deck);
        let waker = deck.waker();

        let thread = {
            let waker = waker.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(100));
                waker.wake();
            })
        };
        let started = Instant::now();
        assert!(deck.poll(10 * SECOND).unwrap().is_empty());
        thread.join().unwrap();
        assert!(started.elapsed() < 2 * SECOND, "{:?}", started.elapsed());

        // A wake between two polls ends the next one at once, and only that.
        waker.wake();
        let started = Instant::now();
        assert!(deck.poll(10 * SECOND).unwrap().is_empty());
        assert!(started.elapsed() < SECOND);
        assert!(deck.poll(Duration::from_millis(50)).unwrap().is_empty());
        assert!(started.elapsed() >= Duration::from_millis(50));
    }

    #[test]
    fn a_reader_ends_with_its_module_and_the_module_comes_back() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.event_deck(DeckOptions::default());
        assert_eq!(summary(&poll(&mut deck)), ["+N"]);
        let scans = bus.scans();

        // No announcement: the failed read alone says the module is gone.
        knobs.unplug();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["-N"]);
        assert_eq!(deck.modules().count(), 0);
        assert_eq!(bus.scans(), scans);

        // The bus still lists it, so the rescan that follows after a pause
        // opens the replacement handle.
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let started = Instant::now();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["+N"]);
        assert!(started.elapsed() >= RECONNECT_PAUSE / 2);
    }

    #[test]
    fn what_a_module_sent_before_it_left_is_delivered_before_its_departure() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.event_deck(DeckOptions::default());
        poll(&mut deck);

        knobs.push_input(&KNOB_STEP);
        knobs.settle();
        bus.unplug("N");
        bus.changed();
        let mut seen = Vec::new();
        while seen.len() < 2 {
            seen.extend(summary(&deck.poll(5 * SECOND).unwrap()));
        }
        assert_eq!(seen, ["N:EncoderDelta { index: 2, delta: 1 }", "-N"]);
    }

    #[test]
    fn an_open_that_fails_while_the_node_is_new_is_not_reported() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let path = bus.0.lock().unwrap().listed[0].path.clone();
        bus.0.lock().unwrap().refuse.push(path);
        let mut deck = bus.event_deck(DeckOptions::default());

        // Udev has not given access yet: the open is tried again.
        assert!(deck.poll(Duration::from_millis(700)).unwrap().is_empty());
        assert!(bus.scans() >= 3, "{} scans", bus.scans());
        // Access arrives with the next announcement.
        bus.0.lock().unwrap().refuse.clear();
        bus.changed();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["+N"]);
    }

    #[test]
    fn an_open_that_keeps_failing_is_reported_once_after_the_retries() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let path = bus.0.lock().unwrap().listed[0].path.clone();
        bus.0.lock().unwrap().refuse.push(path);
        let mut deck = bus.event_deck(DeckOptions::default());

        let started = Instant::now();
        assert_eq!(summary(&deck.poll(10 * SECOND).unwrap()), ["!N"]);
        assert!(started.elapsed() >= OPEN_RETRY_WINDOW);
        // From here on it is tried once a second, silently.
        let scans = bus.scans();
        assert!(deck.poll(Duration::from_millis(500)).unwrap().is_empty());
        assert_eq!(bus.scans(), scans);
        assert!(deck.poll(Duration::from_millis(2500)).unwrap().is_empty());
        assert!(bus.scans() >= scans + 2, "{} scans", bus.scans() - scans);
        // An announcement does not wait for the timer.
        let scans = bus.scans();
        bus.changed();
        assert!(deck.poll(Duration::from_millis(100)).unwrap().is_empty());
        assert_eq!(bus.scans(), scans + 1);
    }

    #[test]
    fn a_module_that_would_not_open_is_tried_again_without_an_announcement() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let path = bus.0.lock().unwrap().listed[0].path.clone();
        bus.0.lock().unwrap().refuse.push(path);
        let mut deck = bus.event_deck(DeckOptions::default());
        assert_eq!(summary(&deck.poll(10 * SECOND).unwrap()), ["!N"]);

        // Nothing tells the deck that access was given.
        bus.0.lock().unwrap().refuse.clear();
        let started = Instant::now();
        assert_eq!(summary(&deck.poll(10 * SECOND).unwrap()), ["+N"]);
        assert!(started.elapsed() < 3 * SECOND, "{:?}", started.elapsed());

        // Once it is open the deck goes back to sleeping.
        let scans = bus.scans();
        assert!(deck.poll(Duration::from_millis(2500)).unwrap().is_empty());
        assert_eq!(bus.scans(), scans);
    }

    #[test]
    fn a_display_that_is_not_relisted_is_tried_again_without_an_announcement() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Keys15, "K", &[1]);
        // Turned down on the second look only, the one after opening.
        let looks = AtomicUsize::new(0);
        let options =
            DeckOptions::default().filter(move |_| looks.fetch_add(1, Ordering::Relaxed) != 1);
        let mut deck = bus.event_deck(options);
        assert!(poll(&mut deck).is_empty());

        let started = Instant::now();
        assert_eq!(summary(&deck.poll(10 * SECOND).unwrap()), ["+K"]);
        assert!(started.elapsed() < 3 * SECOND, "{:?}", started.elapsed());
        let scans = bus.scans();
        assert!(deck.poll(Duration::from_millis(2500)).unwrap().is_empty());
        assert_eq!(bus.scans(), scans);
    }

    #[test]
    fn a_watch_that_ends_hands_the_discovery_back_to_the_timer() {
        let bus = FakeBackend::default();
        let mut deck = bus.event_deck(DeckOptions::default());
        assert!(poll(&mut deck).is_empty());
        assert_eq!(deck.rescan_interval(), SAFETY_RESCAN);

        // Missed while the watch was dying: it is looked for at once.
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let started = Instant::now();
        bus.watch_ended();
        assert_eq!(summary(&deck.poll(10 * SECOND).unwrap()), ["+N"]);
        assert!(started.elapsed() < SECOND, "{:?}", started.elapsed());
        assert_eq!(deck.rescan_interval(), POLLING_RESCAN);

        // And from here on the timer finds what comes.
        bus.plug(ModuleKind::Rollers2, "R", &[2]);
        assert_eq!(summary(&deck.poll(10 * SECOND).unwrap()), ["+R"]);
        assert!(started.elapsed() < 4 * SECOND, "{:?}", started.elapsed());
    }

    #[test]
    fn a_read_that_fails_once_does_not_end_the_reader() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.event_deck(DeckOptions::default());
        assert_eq!(summary(&poll(&mut deck)), ["+N"]);
        knobs.settle();

        // As hidapi's read does when a signal interrupts its poll.
        knobs.glitch();
        knobs.push_input(&KNOB_STEP);
        assert_eq!(
            summary(&deck.poll(5 * SECOND).unwrap()),
            ["N:EncoderDelta { index: 2, delta: 1 }"]
        );
        // The failure was followed by a read that does not wait.
        assert!(
            knobs
                .0
                .lock()
                .unwrap()
                .read_timeouts
                .contains(&Duration::ZERO)
        );
        // The module was not given up, and its reader still reads.
        knobs.settle();
        knobs.push_input(&KNOB_STEP);
        assert_eq!(
            summary(&deck.poll(5 * SECOND).unwrap()),
            ["N:EncoderDelta { index: 2, delta: 1 }"]
        );
        assert_eq!(deck.modules().count(), 1);
    }

    /// The 15 LCD Keys report with the keys in `pressed` held.
    fn keys_report(pressed: &[usize]) -> Vec<u8> {
        let mut report = vec![0x01, 0x34, 17, 0x00, 0x02, 15];
        report.extend((0..15).map(|key| u8::from(pressed.contains(&key))));
        report
    }

    fn faders_report(positions: [u8; 5]) -> Vec<u8> {
        let mut report = vec![0x01, 0x2a, 0x07, 0x00, 0x02, 0x05];
        report.extend(positions);
        report
    }

    fn turn(id: &str, index: usize) -> String {
        format!("{id}:EncoderDelta {{ index: {index}, delta: 1 }}")
    }

    /// Lets each reader of `deck` queue at most `capacity` reports.
    fn queue_of(deck: &Deck, capacity: usize) {
        deck.shared.lock().capacity = capacity;
    }

    #[test]
    fn a_slow_consumer_loses_nothing_within_the_queue_size() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        let fits = Deck::MAX_PENDING_REPORTS;
        for i in 0..fits {
            knobs.push_input(&knob_step(i % 3));
        }
        knobs.settle();
        std::thread::sleep(Duration::from_millis(50));
        let events = summary(&poll(&mut deck));
        let expected: Vec<String> = (0..fits).map(|i| turn("N", i % 3)).collect();
        assert_eq!(events, expected);

        // One more than fits: the last one is reported as lost.
        for i in 0..=fits {
            knobs.push_input(&knob_step(i % 3));
        }
        knobs.settle();
        let events = summary(&poll(&mut deck));
        assert_eq!(events.len(), fits + 1);
        assert_eq!(events[..fits], expected);
        assert_eq!(events[fits], "N:lost1");
    }

    #[test]
    fn a_stalled_consumer_keeps_memory_bounded_and_learns_what_was_lost() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 4);
        poll(&mut deck);

        for i in 0..10_000 {
            knobs.push_input(&knob_step(i % 3));
        }
        // The reader took everything without waiting for the deck.
        knobs.settle();
        {
            let queue = deck.shared.lock();
            assert_eq!(queue.msgs.len(), 4);
            assert_eq!(queue.lost.len(), 1);
            // Encoder steps leave no state behind to replay.
            assert!(queue.lost[0].states.is_empty());
        }
        let events = summary(&poll(&mut deck));
        assert_eq!(
            events,
            [
                turn("N", 0),
                turn("N", 1),
                turn("N", 2),
                turn("N", 0),
                "N:lost9996".to_owned()
            ]
        );

        // Recovered: what comes next is delivered as usual, without a loss.
        knobs.push_input(&knob_step(1));
        knobs.settle();
        assert_eq!(summary(&poll(&mut deck)), [turn("N", 1)]);
        assert!(poll(&mut deck).is_empty());
    }

    #[test]
    fn held_buttons_are_right_again_after_a_loss() {
        let bus = FakeBackend::default();
        let keys = bus.plug(ModuleKind::Keys15, "K", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 2);
        poll(&mut deck);

        // Press and release 0, press 1, press 2, release 1: key 2 is held.
        for pressed in [&[0][..], &[], &[1], &[1, 2], &[2]] {
            keys.push_input(&keys_report(pressed));
        }
        keys.settle();
        assert_eq!(
            summary(&poll(&mut deck)),
            [
                "K:ButtonDown { index: 0 }",
                "K:ButtonUp { index: 0 }",
                "K:lost3",
                // Key 1 went down and up in the gap and is not made up.
                "K:ButtonDown { index: 2 }",
            ]
        );
        keys.push_input(&keys_report(&[]));
        keys.settle();
        assert_eq!(summary(&poll(&mut deck)), ["K:ButtonUp { index: 2 }"]);
    }

    #[test]
    fn faders_end_up_at_their_last_position_after_a_loss() {
        let bus = FakeBackend::default();
        let faders = bus.plug(ModuleKind::Faders5, "F", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 1);
        // Positions 10, 20, 30, 40, 50 are read when the module connects.
        assert_eq!(poll(&mut deck).len(), 6);

        for first in [11, 12, 13, 14] {
            let last = if first == 14 { 51 } else { 50 };
            faders.push_input(&faders_report([first, 20, 30, 40, last]));
        }
        faders.settle();
        assert_eq!(
            summary(&poll(&mut deck)),
            [
                "F:AnalogChanged { index: 0, value: 11 }",
                "F:lost3",
                "F:AnalogChanged { index: 0, value: 14 }",
                "F:AnalogChanged { index: 4, value: 51 }",
            ]
        );
    }

    #[test]
    fn the_orientation_is_replayed_after_a_loss() {
        let bus = FakeBackend::default();
        let base = bus.plug(ModuleKind::Base, "B", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 1);
        poll(&mut deck);

        let heartbeat = |status: u8| [0x01, 0x2b, 0x01, 0x00, status];
        for status in [1, 1, 0, 1, 0] {
            base.push_input(&heartbeat(status));
        }
        base.settle();
        assert_eq!(
            summary(&poll(&mut deck)),
            [
                "B:OrientationChanged { upright: false }",
                "B:lost4",
                "B:OrientationChanged { upright: true }",
            ]
        );
    }

    #[test]
    fn a_busy_module_does_not_cost_a_quiet_one_its_input() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let rollers = bus.plug(ModuleKind::Rollers2, "R", &[2]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 3);
        poll(&mut deck);

        for i in 0..100 {
            knobs.push_input(&knob_step(i % 3));
        }
        knobs.settle();
        rollers.push_input(&[0x01, 0x2a, 0x04, 0x00, 0x01, 0x02, 0x01, 0xff]);
        rollers.settle();
        let events = summary(&poll(&mut deck));
        assert_eq!(
            events,
            [
                turn("N", 0),
                turn("N", 1),
                turn("N", 2),
                "R:EncoderDelta { index: 0, delta: 1 }".to_owned(),
                "N:lost97".to_owned(),
            ]
        );
    }

    #[test]
    fn a_loss_is_reported_before_the_module_leaves() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 2);
        poll(&mut deck);

        for i in 0..5 {
            knobs.push_input(&knob_step(i % 3));
        }
        knobs.settle();
        knobs.unplug();
        reader_ended(&knobs);
        assert_eq!(
            summary(&poll(&mut deck)),
            [
                turn("N", 0),
                turn("N", 1),
                "N:lost3".to_owned(),
                "-N".to_owned(),
                "!N".to_owned()
            ]
        );
    }

    #[test]
    fn reports_of_an_earlier_session_never_reach_a_later_one() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 1);
        poll(&mut deck);
        let first = deck.slots[0].id;

        bus.unplug("N");
        assert_eq!(summary(&poll(&mut deck)), ["-N"]);
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        assert_eq!(summary(&poll(&mut deck)), ["+N"]);
        assert_ne!(deck.slots[0].id, first);

        // A reader of the first session that was still in a read hands in
        // what it got: one report queued, one lost.
        deck.shared.update(|queue| {
            for _ in 0..2 {
                let report = keys_report(&[]);
                queue.report(first, ModuleKind::Knobs3, Instant::now(), report);
            }
            queue.msgs.push_back(Msg::ReaderEnded { slot: first });
        });
        assert!(poll(&mut deck).is_empty());
        assert_eq!(deck.modules().count(), 1);
    }

    #[test]
    fn wake_ups_and_announcements_do_not_pile_up() {
        let bus = FakeBackend::default();
        let mut deck = bus.event_deck(DeckOptions::default());
        poll(&mut deck);
        let scans = bus.scans();

        let waker = deck.waker();
        for _ in 0..100_000 {
            waker.wake();
            bus.changed();
        }
        assert!(deck.shared.lock().msgs.is_empty());
        let started = Instant::now();
        assert!(deck.poll(10 * SECOND).unwrap().is_empty());
        assert!(started.elapsed() < SECOND, "{:?}", started.elapsed());
        // All of it was one wake-up and one read of the device list.
        assert!(deck.poll(Duration::from_millis(200)).unwrap().is_empty());
        assert_eq!(bus.scans(), scans + 1);
        assert!(started.elapsed() >= Duration::from_millis(200));

        drop(deck);
        waker.wake();
    }

    #[test]
    fn readers_end_after_the_deck_is_dropped() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);
        knobs.settle();
        assert_eq!(knobs.0.lock().unwrap().readers, 1);
        // Reports that nobody will take any more.
        queue_of(&deck, 1);
        for i in 0..10 {
            knobs.push_input(&knob_step(i % 3));
        }
        knobs.settle();
        let shared = Arc::downgrade(&deck.shared);

        drop(deck);
        // The reader leaves at the end of the read it is in.
        let deadline = Instant::now() + READ_WAIT + 2 * SECOND;
        while knobs.0.lock().unwrap().readers > 0 || shared.upgrade().is_some() {
            assert!(Instant::now() < deadline, "the reader did not end");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn failed(deck: &mut Deck) {
        assert!(matches!(
            deck.poll(Duration::ZERO),
            Err(Error::Backend(message)) if message.contains("scripted")
        ));
    }

    #[test]
    fn input_taken_before_a_failed_rescan_comes_with_the_next_poll() {
        for count in [1, 3] {
            let bus = FakeBackend::default();
            let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
            let mut deck = bus.deck(DeckOptions::default());
            poll(&mut deck);

            for i in 0..count {
                knobs.push_input(&knob_step(i));
            }
            knobs.settle();
            bus.fail_next(1);
            failed(&mut deck);
            let expected: Vec<String> = (0..count).map(|i| turn("N", i)).collect();
            assert_eq!(summary(&poll(&mut deck)), expected);
            // Once only.
            assert!(poll(&mut deck).is_empty());
        }
    }

    #[test]
    fn a_loss_taken_before_a_failed_rescan_is_still_reported() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        queue_of(&deck, 2);
        poll(&mut deck);

        for i in 0..5 {
            knobs.push_input(&knob_step(i % 3));
        }
        knobs.settle();
        bus.fail_next(1);
        failed(&mut deck);
        assert_eq!(
            summary(&poll(&mut deck)),
            [turn("N", 0), turn("N", 1), "N:lost3".to_owned()]
        );
        assert!(poll(&mut deck).is_empty());
        // The queue has room again.
        knobs.push_input(&knob_step(2));
        knobs.settle();
        assert_eq!(summary(&poll(&mut deck)), [turn("N", 2)]);
    }

    #[test]
    fn a_departure_taken_before_a_failed_rescan_comes_with_the_next_poll() {
        let bus = FakeBackend::default();
        let knobs = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        knobs.unplug();
        reader_ended(&knobs);
        bus.fail_next(1);
        failed(&mut deck);
        assert_eq!(deck.modules().count(), 0);
        // Still listed and still dead: the rescan of this poll cannot open it.
        assert_eq!(summary(&poll(&mut deck)), ["-N", "!N"]);
        assert!(poll(&mut deck).is_empty());
    }

    #[test]
    fn what_a_rescan_did_before_it_failed_is_reported() {
        let bus = FakeBackend::default();
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.plug(ModuleKind::Rollers2, "R", &[2]);
        let mut deck = bus.deck(DeckOptions::default());
        assert_eq!(summary(&poll(&mut deck)), ["+N", "+R"]);

        // The rescan drops the rollers and opens the keys, then fails on the
        // second look at the list that every display module gets.
        bus.unplug("R");
        bus.plug(ModuleKind::Keys15, "K", &[3]);
        let second = bus.scans() + 2;
        bus.0.lock().unwrap().failing.push(second);
        failed(&mut deck);
        // The keys were closed again; the rollers are gone.
        let open: Vec<&str> = deck.modules().map(|m| m.id.as_str()).collect();
        assert_eq!(open, ["N"]);
        assert_eq!(summary(&poll(&mut deck)), ["-R", "+K"]);
        assert!(poll(&mut deck).is_empty());
        assert_eq!(deck.modules().count(), 2);
    }

    #[test]
    fn a_failed_rescan_does_not_mix_up_two_connections() {
        let bus = FakeBackend::default();
        let old = bus.plug(ModuleKind::Knobs3, "N", &[1]);
        let mut deck = bus.deck(DeckOptions::default());
        poll(&mut deck);

        old.push_input(&knob_step(0));
        old.settle();
        // Moved to another place: a new connection of the same module.
        bus.unplug("N");
        let new = bus.plug(ModuleKind::Knobs3, "N", &[2]);
        bus.fail_next(1);
        failed(&mut deck);
        assert_eq!(
            summary(&poll(&mut deck)),
            [turn("N", 0), "-N".to_owned(), "+N".to_owned()]
        );

        // The first connection's reader hands in nothing more.
        old.push_input(&knob_step(1));
        old.settle();
        new.push_input(&knob_step(2));
        new.settle();
        assert_eq!(summary(&poll(&mut deck)), [turn("N", 2)]);
    }

    #[test]
    fn an_announced_module_is_found_after_a_failed_rescan() {
        let bus = FakeBackend::default();
        let mut deck = bus.event_deck(DeckOptions::default());
        assert!(poll(&mut deck).is_empty());

        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.fail_next(1);
        bus.changed();
        assert!(deck.poll(5 * SECOND).is_err());
        // Not a minute later, when the safety rescan would be due.
        let started = Instant::now();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["+N"]);
        let waited = started.elapsed();
        assert!(waited >= SECOND / 2 && waited < 3 * SECOND, "{waited:?}");
    }

    #[test]
    fn a_list_that_keeps_failing_is_read_once_a_second() {
        let bus = FakeBackend::default();
        let mut deck = bus.event_deck(DeckOptions::default());
        assert!(poll(&mut deck).is_empty());
        bus.plug(ModuleKind::Knobs3, "N", &[1]);
        bus.fail_next(1000);
        bus.changed();
        let (scans, passes) = (bus.scans(), deck.passes);

        let started = Instant::now();
        let mut errors = 0;
        while started.elapsed() < Duration::from_millis(3500) {
            match deck.poll(Duration::from_millis(3500).saturating_sub(started.elapsed())) {
                Ok(events) => assert!(events.is_empty()),
                Err(_) => errors += 1,
            }
        }
        assert!((3..=5).contains(&errors), "{errors} errors");
        assert_eq!(bus.scans() - scans, errors);
        assert!(deck.passes - passes < 30, "{} passes", deck.passes - passes);

        bus.0.lock().unwrap().failing.clear();
        assert_eq!(summary(&deck.poll(5 * SECOND).unwrap()), ["+N"]);
    }

    #[test]
    fn deck_can_move_to_another_thread() {
        fn assert_send<T: Send>() {}
        assert_send::<Deck>();
    }
}
