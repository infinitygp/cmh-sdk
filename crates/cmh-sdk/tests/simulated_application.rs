//! How an application tests its own logic without the hardware, through the
//! public API only (cargo feature `test-support`).
//!
//! ```sh
//! cargo test -p cmh-sdk --features test-support --test simulated_application
//! ```

use std::collections::HashMap;
use std::time::Duration;

use cmh_sdk::simulator::Simulator;
use cmh_sdk::{Deck, DeckEvent, DeckOptions, InputEvent, LedMode, ModuleId, ModuleKind};

/// The application under test: a mixer whose faders set channel volumes,
/// whose keys mute channels and whose knob LEDs show red while anything is
/// muted. It only ever sees a `Deck`.
#[derive(Default)]
struct Mixer {
    volumes: HashMap<u8, u8>,
    muted: [bool; 15],
    knobs: Option<ModuleId>,
}

impl Mixer {
    fn handle(&mut self, deck: &Deck, event: DeckEvent) -> cmh_sdk::Result<()> {
        match event {
            DeckEvent::Connected(info) if info.kind == ModuleKind::Knobs3 => {
                self.knobs = Some(info.id);
                self.show(deck)?;
            }
            DeckEvent::Disconnected(info) if self.knobs.as_ref() == Some(&info.id) => {
                self.knobs = None;
            }
            DeckEvent::Input {
                event: InputEvent::AnalogChanged { index, value },
                ..
            } => {
                self.volumes.insert(index, value);
            }
            DeckEvent::Input {
                kind: ModuleKind::Keys15,
                event: InputEvent::ButtonDown { index },
                ..
            } => {
                self.muted[usize::from(index)] ^= true;
                self.show(deck)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn show(&self, deck: &Deck) -> cmh_sdk::Result<()> {
        let Some(knobs) = self.knobs.as_ref().and_then(|id| deck.module(id)) else {
            return Ok(());
        };
        let (r, g) = if self.muted.contains(&true) {
            (255, 0)
        } else {
            (0, 255)
        };
        knobs.set_led_mode(0, LedMode::Static)?;
        knobs.set_led_color(0, r, g, 0)
    }

    /// Polls until nothing more happens for a moment.
    fn run(&mut self, deck: &mut Deck) -> cmh_sdk::Result<()> {
        loop {
            let events = deck.poll(Duration::from_millis(200))?;
            if events.is_empty() {
                return Ok(());
            }
            for event in events {
                self.handle(deck, event)?;
            }
        }
    }
}

#[test]
fn faders_set_volumes_and_keys_mute() -> cmh_sdk::Result<()> {
    let bus = Simulator::new();
    let faders = bus.plug(ModuleKind::Faders5, "FADERS-1");
    let keys = bus.plug(ModuleKind::Keys15, "KEYS-1");
    let knobs = bus.plug(ModuleKind::Knobs3, "KNOBS-1");
    faders.move_fader(0, 70);

    let mut deck = bus.deck(DeckOptions::default());
    let mut mixer = Mixer::default();
    mixer.run(&mut deck)?;
    // The position read when the faders connected counts as well.
    assert_eq!(mixer.volumes.get(&0), Some(&70));

    faders.move_fader(1, 25);
    keys.press(4);
    keys.release(4);
    mixer.run(&mut deck)?;
    assert_eq!(mixer.volumes.get(&1), Some(&25));
    assert!(mixer.muted[4]);
    let last = knobs.led_commands().last().map(|report| report.command);
    assert_eq!(
        last,
        Some(cmh_sdk::protocol::led::LedCommand::Color { r: 255, g: 0, b: 0 })
    );
    Ok(())
}

#[test]
fn the_mixer_forgets_knobs_that_are_pulled() -> cmh_sdk::Result<()> {
    let bus = Simulator::new();
    bus.plug(ModuleKind::Knobs3, "KNOBS-1");
    let keys = bus.plug(ModuleKind::Keys15, "KEYS-1");
    let mut deck = bus.deck(DeckOptions::default());
    let mut mixer = Mixer::default();
    mixer.run(&mut deck)?;
    assert!(mixer.knobs.is_some());

    bus.unplug("KNOBS-1");
    mixer.run(&mut deck)?;
    assert!(mixer.knobs.is_none());
    // Muting without the knobs still works.
    keys.press(0);
    mixer.run(&mut deck)?;
    assert!(mixer.muted[0]);
    Ok(())
}
