//! Simulated modules, for testing an application without the hardware
//! (cargo feature `test-support`).
//!
//! A [`Simulator`] is a USB bus of its own. A [`Deck`] made by
//! [`Simulator::deck`] finds, opens, reads and loses the modules plugged
//! into it through the same code as on real hardware, so the application
//! sees the same [`DeckEvent`](crate::DeckEvent)s in the same order:
//!
//! ```
//! use std::time::Duration;
//! use cmh_sdk::simulator::Simulator;
//! use cmh_sdk::{DeckEvent, DeckOptions, InputEvent, ModuleKind};
//!
//! let bus = Simulator::new();
//! let keys = bus.plug(ModuleKind::Keys15, "KEYS-1");
//! let mut deck = bus.deck(DeckOptions::default());
//! let events = deck.poll(Duration::from_secs(1))?;
//! assert!(matches!(&events[..], [DeckEvent::Connected(info)] if info.id.as_str() == "KEYS-1"));
//!
//! keys.press(3);
//! let events = deck.poll(Duration::from_secs(1))?;
//! assert!(matches!(
//!     &events[..],
//!     [DeckEvent::Input { event: InputEvent::ButtonDown { index: 3 }, .. }]
//! ));
//! # Ok::<(), cmh_sdk::Error>(())
//! ```
//!
//! An input method returns once the deck has the report, so the next poll
//! returns its events at once. A module that is plugged in or pulled is
//! announced to the deck, which looks at the bus a few milliseconds later:
//! poll with a timeout to see that.
//!
//! What a module was told is available as decoded LED commands and as the
//! pictures it received. The simulation knows no placement on a base, and
//! reads it does not model (device info, LED brightness) fail with
//! [`Error::Backend`]. Simulated modules accept what
//! the real ones accept; that a feature works here says nothing about a
//! particular device, operating system or firmware.

use std::ffi::CString;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use crate::deck::{Backend, Deck, DeckOptions};
use crate::devices::{Capabilities, ModuleKind};
use crate::error::{Error, Result};
use crate::module::Module;
use crate::protocol::feature::FADER_POSITIONS_REPORT_ID;
use crate::protocol::image::ImageChunk;
use crate::protocol::input::{self, InputMessage, STEP_CODE_0, STEP_CODE_1, STEP_NONE};
use crate::protocol::led::{LedReport, parse_led_report};
use crate::testing::{DIAL_FIRMWARE, KEYS_FIRMWARE, Mock, Sent};
use crate::transport::{HidInterface, InputReader};

type OnChange = Box<dyn Fn() + Send + Sync>;

#[derive(Default)]
struct Bus {
    modules: Vec<(HidInterface, Mock)>,
    /// Decks to tell when a module is plugged in or pulled.
    listeners: Vec<OnChange>,
    next_port: u8,
}

/// A simulated USB bus with MasterHUB modules on it. Clones share the bus.
#[derive(Clone, Default)]
pub struct Simulator(Arc<Mutex<Bus>>);

impl std::fmt::Debug for Simulator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bus = lock(&self.0);
        let serials: Vec<_> = bus.modules.iter().map(|(i, _)| &i.serial).collect();
        f.debug_struct("Simulator")
            .field("modules", &serials)
            .finish()
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Simulator {
    /// An empty bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// A deck that uses this bus instead of the hardware. The deck always
    /// learns of changes to the bus by announcement, as on Linux; the other
    /// options apply as given.
    pub fn deck(&self, options: DeckOptions) -> Deck {
        Deck::with_backend(Box::new(self.clone()), options.device_events(true))
    }

    /// Plugs in a module with this serial number, which becomes its
    /// [`ModuleId`](crate::ModuleId). A module that is already plugged in
    /// under the serial is pulled first. The faders start at 0; the display
    /// modules start in standalone mode, as after power-up.
    pub fn plug(&self, kind: ModuleKind, serial: &str) -> SimulatedModule {
        let mock = Mock::with_features(&[
            &[FADER_POSITIONS_REPORT_ID, 0x05, 0, 0, 0, 0, 0],
            KEYS_FIRMWARE,
            DIAL_FIRMWARE,
        ]);
        lock(&mock.0).standalone = kind.capabilities().display.is_some();
        {
            let mut bus = lock(&self.0);
            bus.next_port = bus.next_port.wrapping_add(1);
            let interface = HidInterface {
                kind,
                path: CString::new(format!("simulated-{serial}"))
                    .unwrap_or_else(|_| c"simulated".to_owned()),
                product_id: kind.product_id(),
                serial: Some(serial.to_owned()),
                manufacturer: None,
                product: None,
                release_number: 0,
                usage_page: 0,
                usage: 0,
                interface_number: 0,
                port_chain: Some(vec![bus.next_port]),
                usb_bus: Some("simulated".to_owned()),
            };
            if let Some(index) = bus
                .modules
                .iter()
                .position(|(i, _)| i.serial == interface.serial)
            {
                bus.modules.remove(index).1.unplug();
            }
            bus.modules.push((interface, mock.clone()));
        }
        self.announce();
        SimulatedModule {
            kind,
            mock,
            state: Mutex::new(Controls::default()),
        }
    }

    /// Pulls the module with this serial number, if one is plugged in.
    pub fn unplug(&self, serial: &str) {
        let pulled = {
            let mut bus = lock(&self.0);
            let index = bus
                .modules
                .iter()
                .position(|(i, _)| i.serial.as_deref() == Some(serial));
            index.map(|index| bus.modules.remove(index).1)
        };
        if let Some(mock) = pulled {
            mock.unplug();
            self.announce();
        }
    }

    fn announce(&self) {
        for listener in &lock(&self.0).listeners {
            listener();
        }
    }
}

impl Backend for Simulator {
    fn scan(&mut self) -> Result<Vec<HidInterface>> {
        Ok(lock(&self.0)
            .modules
            .iter()
            .map(|(i, _)| i.clone())
            .collect())
    }

    fn open(&self, interface: &HidInterface) -> Result<(Module, Box<dyn InputReader>)> {
        let bus = lock(&self.0);
        let mock = bus
            .modules
            .iter()
            .find(|(i, _)| i.path == interface.path)
            .map(|(_, mock)| mock)
            .filter(|mock| !lock(&mock.0).unplugged)
            .ok_or_else(|| Error::Disconnected("simulated module pulled".to_owned()))?;
        Ok((
            Module::new(
                interface.kind,
                interface.serial.clone(),
                Box::new(mock.clone()),
            ),
            Box::new(mock.reader()),
        ))
    }

    fn watch(
        &self,
        changed: Box<dyn Fn() + Send + Sync>,
        _ended: Box<dyn FnOnce() + Send>,
    ) -> Option<Box<dyn Send>> {
        lock(&self.0).listeners.push(changed);
        Some(Box::new(()))
    }
}

#[derive(Default)]
struct Controls {
    buttons: [u8; 15],
    faders: [u8; 5],
}

/// A module plugged into a [`Simulator`]. Its input methods act like a hand
/// on the module and panic when the module has no such control.
pub struct SimulatedModule {
    kind: ModuleKind,
    mock: Mock,
    state: Mutex<Controls>,
}

impl std::fmt::Debug for SimulatedModule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SimulatedModule")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl SimulatedModule {
    pub fn kind(&self) -> ModuleKind {
        self.kind
    }

    fn capabilities(&self) -> Capabilities {
        self.kind.capabilities()
    }

    /// Hands a report to the deck that has the module open, and returns once
    /// the deck has it. Without such a deck the report goes nowhere, as
    /// input does on a module nobody listens to.
    fn send(&self, message: InputMessage<'_>) {
        let report = input::encode(self.kind, message).expect("a report this module sends");
        if lock(&self.mock.0).readers == 0 {
            return;
        }
        self.mock.push_input(&report);
        self.mock.settle();
    }

    fn set_button(&self, index: u8, pressed: bool) {
        let buttons = self.capabilities().buttons;
        assert!(
            index < buttons,
            "{} has no button {index}",
            self.kind.name()
        );
        let mut state = lock(&self.state);
        state.buttons[usize::from(index)] = u8::from(pressed);
        self.send(InputMessage::ButtonStates(
            &state.buttons[..usize::from(buttons)],
        ));
    }

    /// Presses button `index` (counted as in [`InputEvent`](crate::InputEvent)).
    pub fn press(&self, index: u8) {
        self.set_button(index, true);
    }

    /// Releases button `index`.
    pub fn release(&self, index: u8) {
        self.set_button(index, false);
    }

    /// Turns encoder `index` by `steps` detents, one report each; positive
    /// is the direction of a positive
    /// [`EncoderDelta`](crate::InputEvent::EncoderDelta).
    pub fn turn(&self, index: u8, steps: i8) {
        let encoders = self.capabilities().encoders;
        assert!(
            index < encoders,
            "{} has no encoder {index}",
            self.kind.name()
        );
        let code = [STEP_CODE_0, STEP_CODE_1]
            .into_iter()
            .find(|code| input::step_delta(self.kind, *code) == Some(steps.signum()))
            .unwrap_or(STEP_NONE);
        let mut codes = vec![STEP_NONE; usize::from(encoders)];
        codes[usize::from(index)] = code;
        for _ in 0..steps.unsigned_abs() {
            self.send(InputMessage::EncoderSteps(&codes));
        }
    }

    /// Moves fader `index` to `value` (`0..=100`). A deck that opens the
    /// module later reads this position.
    pub fn move_fader(&self, index: u8, value: u8) {
        let faders = self.capabilities().faders;
        assert!(index < faders, "{} has no fader {index}", self.kind.name());
        assert!(
            value <= input::FADER_MAX,
            "fader position {value} above 100"
        );
        let mut state = lock(&self.state);
        state.faders[usize::from(index)] = value;
        {
            let mut mock = lock(&self.mock.0);
            mock.features.retain(|f| f[0] != FADER_POSITIONS_REPORT_ID);
            mock.features
                .push([&[FADER_POSITIONS_REPORT_ID, 0x05][..], &state.faders].concat());
        }
        self.send(InputMessage::FaderPositions(&state.faders));
    }

    /// Stands the base upright on its short side (`true`) or lays it flat,
    /// as reported by its heartbeat.
    pub fn set_upright(&self, upright: bool) {
        assert!(
            self.kind == ModuleKind::Base,
            "{} has no orientation",
            self.kind.name()
        );
        self.send(InputMessage::Heartbeat {
            status: u8::from(!upright),
        });
    }

    /// Every LED command the module received, oldest first.
    pub fn led_commands(&self) -> Vec<LedReport> {
        self.outputs()
            .iter()
            .filter_map(|report| parse_led_report(report).ok())
            .collect()
    }

    /// Every picture the module received, oldest first, with the key or
    /// display it was meant for: the JPEG as it was sent.
    pub fn pictures(&self) -> Vec<(u8, Vec<u8>)> {
        let mut pictures = Vec::new();
        let mut current = Vec::new();
        for report in self.outputs() {
            let Ok(chunk) = ImageChunk::parse(&report) else {
                continue;
            };
            if chunk.index == 0 {
                current.clear();
            }
            current.extend_from_slice(chunk.data);
            if chunk.last {
                pictures.push((chunk.target, std::mem::take(&mut current)));
            }
        }
        pictures
    }

    /// Whether a deck switched the display module to software mode and it is
    /// still there.
    pub fn in_software_mode(&self) -> bool {
        self.capabilities().display.is_some() && !lock(&self.mock.0).standalone
    }

    fn outputs(&self) -> Vec<Vec<u8>> {
        self.mock
            .sent()
            .into_iter()
            .filter_map(|sent| match sent {
                Sent::Output(report) => Some(report),
                _ => None,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::{DeckEvent, InputEvent, LedMode, protocol::led::LedCommand};

    const WAIT: Duration = Duration::from_secs(5);

    fn inputs(events: &[DeckEvent]) -> Vec<InputEvent> {
        events
            .iter()
            .filter_map(|e| match e {
                DeckEvent::Input { event, .. } => Some(*event),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_control_reaches_the_deck() {
        let bus = Simulator::new();
        let keys = bus.plug(ModuleKind::Keys15, "K");
        let knobs = bus.plug(ModuleKind::Knobs3, "N");
        let dial = bus.plug(ModuleKind::DialDisplay, "D");
        let rollers = bus.plug(ModuleKind::Rollers2, "R");
        let base = bus.plug(ModuleKind::Base, "B");
        let faders = bus.plug(ModuleKind::Faders5, "F");
        faders.move_fader(2, 40);
        let mut deck = bus.deck(DeckOptions::default());

        let events = deck.poll(WAIT).unwrap();
        let connected = events
            .iter()
            .filter(|e| matches!(e, DeckEvent::Connected(_)))
            .count();
        assert_eq!(connected, 6);
        // The faders announce where they stand, including the one moved
        // before the deck was there.
        assert_eq!(
            inputs(&events)[2],
            InputEvent::AnalogChanged {
                index: 2,
                value: 40
            }
        );
        assert!(keys.in_software_mode() && dial.in_software_mode());

        keys.press(14);
        keys.release(14);
        knobs.turn(1, -2);
        knobs.press(2);
        dial.turn(0, 1);
        rollers.turn(0, 1);
        faders.move_fader(4, 100);
        base.set_upright(true);
        use InputEvent::*;
        assert_eq!(
            inputs(&deck.poll(Duration::ZERO).unwrap()),
            [
                ButtonDown { index: 14 },
                ButtonUp { index: 14 },
                EncoderDelta {
                    index: 1,
                    delta: -1
                },
                EncoderDelta {
                    index: 1,
                    delta: -1
                },
                ButtonDown { index: 2 },
                EncoderDelta { index: 0, delta: 1 },
                EncoderDelta { index: 0, delta: 1 },
                AnalogChanged {
                    index: 4,
                    value: 100
                },
                OrientationChanged { upright: true },
            ]
        );
    }

    #[test]
    fn modules_come_and_go() {
        let bus = Simulator::new();
        let mut deck = bus.deck(DeckOptions::default());
        assert!(deck.poll(Duration::ZERO).unwrap().is_empty());

        let knobs = bus.plug(ModuleKind::Knobs3, "N");
        let events = deck.poll(WAIT).unwrap();
        assert!(matches!(&events[..], [DeckEvent::Connected(i)] if i.id.as_str() == "N"));

        bus.unplug("N");
        let events = deck.poll(WAIT).unwrap();
        assert!(matches!(&events[..], [DeckEvent::Disconnected(i)] if i.id.as_str() == "N"));
        // Input on a module nobody has open goes nowhere.
        knobs.press(0);
        assert!(deck.poll(Duration::from_millis(100)).unwrap().is_empty());

        bus.plug(ModuleKind::Knobs3, "N");
        let events = deck.poll(WAIT).unwrap();
        assert!(matches!(&events[..], [DeckEvent::Connected(_)]));
    }

    #[test]
    fn outputs_are_recorded() {
        let bus = Simulator::new();
        let knobs = bus.plug(ModuleKind::Knobs3, "N");
        let dial = bus.plug(ModuleKind::DialDisplay, "D");
        let mut deck = bus.deck(DeckOptions::default());
        deck.poll(WAIT).unwrap();

        let module = deck.module(&"N".into()).unwrap();
        module.set_led_mode(1, LedMode::Static).unwrap();
        module.set_led_color(1, 255, 128, 0).unwrap();
        let commands: Vec<_> = knobs
            .led_commands()
            .into_iter()
            .map(|r| (r.index, r.command))
            .collect();
        assert_eq!(
            commands,
            [
                (1, LedCommand::Mode(LedMode::Static)),
                (
                    1,
                    LedCommand::Color {
                        r: 255,
                        g: 128,
                        b: 0
                    }
                )
            ]
        );

        let jpeg = crate::protocol::image::tests::fake_jpeg(240, 240, 1000);
        let dial_module = deck.module(&"D".into()).unwrap();
        dial_module.set_image(0, &jpeg).unwrap();
        dial_module.set_display_brightness(30).unwrap();
        assert_eq!(dial_module.display_brightness().unwrap(), 30);
        assert_eq!(dial.pictures(), [(0, jpeg)]);
        // A read the simulation does not model fails instead of panicking.
        assert!(module.led_brightness().is_err());
    }

    #[test]
    #[should_panic(expected = "has no button 3")]
    fn a_control_that_does_not_exist_is_a_mistake_in_the_test() {
        Simulator::new().plug(ModuleKind::Knobs3, "N").press(3);
    }
}
