//! Prints everything that happens on the deck.
//!
//! ```sh
//! cargo run -p cmh-sdk --example events
//! ```

use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent};

fn main() -> cmh_sdk::Result<()> {
    let mut deck = Deck::new()?;
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            match event {
                DeckEvent::Connected(info) => {
                    println!("connected    {} ({})", info.kind, info.id);
                    if let Some(p) = info.placement {
                        println!(
                            "             columns {}-{}, rows {}-{}, turned {} degrees",
                            p.column,
                            p.column + p.columns - 1,
                            p.row,
                            p.row + p.rows - 1,
                            p.rotation.degrees()
                        );
                    }
                }
                DeckEvent::Disconnected(info) => {
                    println!("disconnected {} ({})", info.kind, info.id)
                }
                DeckEvent::Input { kind, event, .. } => println!("{kind}: {event:?}"),
                DeckEvent::OpenFailed { interface, error } => {
                    eprintln!("cannot open {}: {error}", interface.kind)
                }
                _ => {}
            }
        }
    }
}
