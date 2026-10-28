# Getting started

This guide takes you from an empty Rust project to a program that reads
input from MasterHUB modules and draws on their displays. It needs no
knowledge of the USB protocol.

## Prerequisites

- Rust 1.88 or newer.
- Windows 11 or Linux. macOS is not supported: the SDK has not been run
  there.
- At least one MasterHUB module connected by USB. The
  [feature matrix](feature-matrix.md) lists what works on which module.
- On Linux: a C compiler, `pkg-config` and the libudev development files
  to build, and a udev rule to open the modules as an ordinary user. Both
  are described in [Linux permissions](linux-permissions.md).

Close the Cooler Master application while you try things out. It can run
alongside the SDK, but it re-sends its own LED settings every minute and
redraws the Dial Display every second, so it overwrites what your program
shows.

## Add the dependency

```sh
cargo new masterhub-hello
cd masterhub-hello
cargo add cmh-sdk
```

To show PNG or JPEG files on the displays, enable the `image` feature:

```sh
cargo add cmh-sdk --features image
```

## Read input and follow hot-plug

Replace `src/main.rs` with:

```rust,no_run
use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent, InputEvent};

fn main() -> cmh_sdk::Result<()> {
    let mut deck = Deck::new()?;
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            match event {
                DeckEvent::Connected(info) => {
                    println!("connected {} ({})", info.kind, info.id);
                }
                DeckEvent::Disconnected(info) => {
                    println!("disconnected {} ({})", info.kind, info.id);
                }
                DeckEvent::Input { id, event, .. } => match event {
                    InputEvent::ButtonDown { index } => println!("{id}: button {index} down"),
                    InputEvent::ButtonUp { index } => println!("{id}: button {index} up"),
                    InputEvent::EncoderDelta { index, delta } => {
                        println!("{id}: encoder {index} turned {delta:+}")
                    }
                    InputEvent::AnalogChanged { index, value } => {
                        println!("{id}: fader {index} at {value}")
                    }
                    other => println!("{id}: {other:?}"),
                },
                DeckEvent::OpenFailed {
                    interface, error, ..
                } => {
                    eprintln!("cannot open {}: {error}", interface.kind);
                }
                _ => {}
            }
        }
    }
}
```

`cargo run` prints a `connected` line for every module from the first
poll. Press a key, turn a knob or move a fader to see input; pull a module
off the base and put it back to see it disconnect and connect again.

A few things to know from the start:

- `Deck::new()` switches the 15 LCD Keys and the Dial Display to software
  mode when they connect. They start in standalone mode after power-up,
  where they play a built-in animation and send no input.
- `info.id` is the module's serial number. It stays the same across
  reconnects, restarts and places on the base, so it is the key for
  per-module settings. Operating system paths such as `/dev/hidraw3` are
  not.
- Control indexes start at 0. The order on each module, as verified on
  the hardware, is documented on `InputEvent`.
- Call `poll` regularly. Input waits in a bounded queue between polls;
  a module that sends more than `Deck::MAX_PENDING_REPORTS` reports in that
  time loses the excess, and the deck reports that as
  `DeckEvent::InputLost`.

## Show a picture on a key

With the `image` feature, this program shows a picture on every key that
is pressed:

```rust,no_run
use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent, InputEvent, ModuleKind};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let picture = std::fs::read("picture.png")?;
    let mut deck = Deck::new()?;
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            if let DeckEvent::Input {
                id,
                kind: ModuleKind::Keys15,
                event: InputEvent::ButtonDown { index },
                ..
            } = event
            {
                if let Some(keys) = deck.module(&id) {
                    keys.set_picture(index, &picture)?;
                }
            }
        }
    }
}
```

`set_picture` converts a PNG or JPEG file to the JPEG format the displays
accept and stretches it to the display (72 x 72 pixels per key, 240 x 240
on the Dial Display). The modules do not keep pictures across a power
cycle or a reconnect, so draw again on `Connected`.

## Set the LEDs

The Base, the 5 Faders and the 3 Knobs have LEDs:

```rust,no_run
use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent, LedMode};

fn main() -> cmh_sdk::Result<()> {
    let mut deck = Deck::new()?;
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            if let DeckEvent::Connected(info) = event {
                let Some(module) = deck.module(&info.id) else {
                    continue;
                };
                for zone in 0..info.kind.capabilities().leds {
                    module.set_led_mode(zone, LedMode::Static)?;
                    module.set_led_color(zone, 255, 96, 0)?;
                    module.set_led_brightness(zone, 60)?;
                }
            }
        }
    }
}
```

Settings made through the SDK are not stored in the module, and the
running effect and colour cannot be read back. Keep the state you want in
your application and set it again on every `Connected`.

## Next steps

- [API guide](api-guide.md): the event model, reconnection, threading,
  errors, images, LEDs and testing without hardware.
- [Troubleshooting](troubleshooting.md): permissions, standalone mode,
  the vendor application and hot-plug.
- [Examples](../crates/cmh-sdk/examples): complete programs, runnable from
  a checkout with `cargo run -p cmh-sdk --example <name>`.
- [Protocol reference](protocol/README.md): what the modules send and
  accept, for diagnosis and for contributors.
