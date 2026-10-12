//! Shows a PNG or JPEG file on every key that is pressed.
//!
//! ```sh
//! cargo run -p cmh-sdk --features image --example show_picture -- picture.png
//! ```

use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent, InputEvent, ModuleKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("usage: show_picture <file>")?;
    let file = std::fs::read(path)?;

    let mut deck = Deck::new()?;
    println!("press a key on the 15 LCD Keys module");
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            if let DeckEvent::Input {
                id,
                kind: ModuleKind::Keys15,
                event: InputEvent::ButtonDown { index },
                ..
            } = event
                && let Some(keys) = deck.module(&id)
            {
                keys.set_picture(index, &file)?;
            }
        }
    }
}
