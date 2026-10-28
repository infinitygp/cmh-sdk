//! Lights the LEDs of the Base, the 5 Faders and the 3 Knobs, lights them
//! again whenever a module comes back, and lets each knob dim its own ring.
//!
//! ```sh
//! cargo run -p cmh-sdk --example leds
//! ```

use std::collections::HashMap;
use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent, DeckOptions, InputEvent, LedMode, Module, ModuleId};

/// Brightness of every LED zone, by module and zone. The application keeps
/// it: the modules forget it when they lose power and cannot report their
/// effect or colour.
type Levels = HashMap<(ModuleId, u8), u8>;

fn light(module: &Module, id: &ModuleId, levels: &mut Levels) -> cmh_sdk::Result<()> {
    for zone in 0..module.kind().capabilities().leds {
        let level = *levels.entry((id.clone(), zone)).or_insert(50);
        module.set_led_mode(zone, LedMode::Static)?;
        module.set_led_color(zone, 0, 160, 255)?;
        module.set_led_brightness(zone, level)?;
    }
    Ok(())
}

fn main() -> cmh_sdk::Result<()> {
    // Only the modules with LEDs; the displays stay with whoever has them.
    let options = DeckOptions::default().filter(|i| i.kind.capabilities().leds > 0);
    let mut deck = Deck::with_options(options)?;
    let mut levels = Levels::new();
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            match event {
                DeckEvent::Connected(info) => {
                    let Some(module) = deck.module(&info.id) else {
                        continue;
                    };
                    // A module pulled meanwhile fails here and is reported
                    // as disconnected by the next poll.
                    match light(module, &info.id, &mut levels) {
                        Ok(()) => println!("lit {} ({})", info.kind, info.id),
                        Err(error) => eprintln!("{}: {error}", info.id),
                    }
                }
                DeckEvent::Input {
                    id,
                    event: InputEvent::EncoderDelta { index, delta },
                    ..
                } => {
                    // Zone `index` is the ring of knob `index`.
                    let level = levels.entry((id.clone(), index)).or_insert(50);
                    *level = level.saturating_add_signed(delta * 5).min(100);
                    if let Some(module) = deck.module(&id)
                        && let Err(error) = module.set_led_brightness(index, *level)
                    {
                        eprintln!("{id}: {error}");
                    }
                }
                DeckEvent::OpenFailed {
                    interface, error, ..
                } => eprintln!("cannot open {}: {error}", interface.kind),
                _ => {}
            }
        }
    }
}
