# cmh-sdk

A Rust SDK for the Cooler Master MasterHUB modular control deck. It finds
the modules, follows them as they are plugged in and moved, delivers their
input as typed events, shows pictures on the displays, controls the LEDs
and tells where each module sits on the base, without the application
having to deal with the USB HID protocol.

The SDK is independent and not affiliated with or endorsed by Cooler
Master. The protocol was learned from controlled observations of real
hardware, and the SDK only sends commands whose effect was verified on a
device.

## Supported hardware and platforms

| Module | Input | Output |
|---|---|---|
| Base | orientation (flat or upright) | LED effect, colour, brightness |
| 15 LCD Keys | 15 keys | a picture per key, backlight |
| 5 Faders | 5 faders, `0..=100` | LED effect, colour, brightness per fader |
| 3 Knobs | 3 knobs, each turning and pressing | LED effect, colour, brightness per knob |
| 2 Rollers | 2 rollers | - |
| Dial Display | a dial, turning and pressing | a round picture, backlight |

| Platform | Status |
|---|---|
| Windows 11 | The features of the feature matrix checked on hardware. Hot-plug is found by reading the device list once a second; the system's device notifications are implemented but have not been run on hardware and are off by default. |
| Linux | Every module checked on a natively installed Linux and, for some modules, under WSL2 with usbipd-win. Needs a udev rule for access. |
| macOS | Not supported: never run. |

The [feature matrix](docs/feature-matrix.md) lists what was verified, on
which platform and with which firmware, and what is still open. Notable
gaps: two bases at once, or two modules of the same model, were only
exercised with scripted devices; the placement on the base needs the USB
topology, which WSL2 hides.

The minimum supported Rust version is 1.88.

## What you can do with it

- **Discovery and hot-plug.** `Deck` opens every module it finds, reports
  `Connected` and `Disconnected` as modules come and go, and identifies
  each module by its serial number.
- **Input.** `ButtonDown`, `ButtonUp`, `EncoderDelta`, `AnalogChanged` and
  `OrientationChanged`, with the time each report was read. Input waits in
  a bounded queue between polls; loss under overload is reported, never
  hidden.
- **Displays.** Pictures on each of the 15 keys and on the dial, from PNG
  or JPEG files with the `image` feature, and the backlight level.
- **Lighting.** LED effect, colour and brightness on the Base, the 5
  Faders and the 3 Knobs.
- **Layout.** The cells a module covers on the base and its rotation.
- **Tests without hardware.** Simulated modules (feature `test-support`)
  drive a real `Deck`, so application logic can be tested without owning
  every module.

## Installation

```sh
cargo add cmh-sdk
cargo add cmh-sdk --features image   # to show PNG and JPEG files
```

On Linux, building needs a C compiler, `pkg-config` and the libudev
development files, and opening the modules as an ordinary user needs a
udev rule. Both are described in [Linux permissions](docs/linux-permissions.md).

## Reading input

```rust,no_run
use std::time::Duration;

use cmh_sdk::{Deck, DeckEvent, InputEvent};

fn main() -> cmh_sdk::Result<()> {
    let mut deck = Deck::new()?;
    loop {
        for event in deck.poll(Duration::from_millis(100))? {
            match event {
                DeckEvent::Connected(info) => println!("connected {} ({})", info.kind, info.id),
                DeckEvent::Disconnected(info) => println!("disconnected {}", info.id),
                DeckEvent::Input {
                    id,
                    event: InputEvent::ButtonDown { index },
                    ..
                } => println!("{id}: button {index} pressed"),
                DeckEvent::OpenFailed {
                    interface, error, ..
                } => eprintln!("cannot open {}: {error}", interface.kind),
                _ => {}
            }
        }
    }
}
```

`Deck::poll` sleeps until something happens and returns everything that
did; an idle deck with five modules costs about 0.03 % of a CPU core on
Linux. The 15 LCD Keys and the Dial Display start in standalone mode after
power-up, where they send no input; `Deck` switches them to software mode
when they connect.

## Pictures and LEDs

Commands go to a module through `deck.module(&id)` between polls:

```rust,no_run
use cmh_sdk::{Deck, LedMode, ModuleId};

/// `png` is the content of a PNG file.
fn decorate(deck: &Deck, keys: &ModuleId, knobs: &ModuleId, png: &[u8]) -> cmh_sdk::Result<()> {
    if let Some(keys) = deck.module(keys) {
        // Needs the `image` feature; `set_image` takes a ready JPEG without it.
        keys.set_picture(0, png)?;
        keys.set_display_brightness(80)?;
    }
    if let Some(knobs) = deck.module(knobs) {
        knobs.set_led_mode(0, LedMode::Static)?;
        knobs.set_led_color(0, 0, 160, 255)?;
    }
    Ok(())
}
```

The modules do not keep pictures or LED settings made through the SDK
across a power cycle, and the running LED effect cannot be read back: keep
the state in the application and set it again on every `Connected`. While
the Cooler Master application runs, it overwrites LEDs and the dial
picture; close it while your program drives the outputs.

## Documentation

- [Getting started](docs/getting-started.md): a first program, step by step.
- [API guide](docs/api-guide.md): events and their ordering, reconnection,
  threads, errors, images, LEDs, testing without hardware.
- [Linux permissions](docs/linux-permissions.md): udev rules and build
  dependencies.
- [Troubleshooting](docs/troubleshooting.md).
- [Feature matrix](docs/feature-matrix.md): verified features and open
  questions.
- [Protocol reference](docs/protocol/README.md) and
  [experiments](docs/experiments/): what the modules send and accept, and
  how each fact was established.
- The API reference: `cargo doc -p cmh-sdk --all-features --open`.

## Examples and the `cmh` tool

From a checkout of this repository:

```sh
cargo run -p cmh-sdk --example events                                # print every event
cargo run -p cmh-sdk --example leds                                  # colour the LEDs, again after every reconnect
cargo run -p cmh-sdk --features image --example show_picture -- a.png  # a picture on every pressed key
cargo test -p cmh-sdk --features test-support --test simulated_application
```

`cmh` is a diagnostic command-line tool built on the SDK. It is part of
this repository, not of the library:

```sh
cargo run -p cmh-cli -- list                 # modules, mount path, placement
cargo run -p cmh-cli -- info --software-mode # firmware and capabilities
cargo run -p cmh-cli -- monitor --raw        # live events and raw reports; writes nothing
cargo run -p cmh-cli -- replay crates/cmh-sdk/fixtures/inputs/all-controls.txt
cargo run -p cmh-cli -- image --module keys --key 3 picture.png
cargo run -p cmh-cli -- led --module knobs --color ff8000 --mode static
```

## Safety

The SDK sends a closed set of commands, each verified on hardware, and
offers no way to send an arbitrary report. Firmware update and recovery,
factory reset, serial number changes and writes of LED settings to a
module's permanent memory are deliberately absent, even where they were
seen in captures of the vendor application.

## Repository layout

- `crates/cmh-sdk`: the library, its examples and the recorded reports
  (`fixtures/`) its tests use
- `crates/cmh-cli`: the `cmh` tool
- `docs/`: user guides, the protocol reference and the experiment notes
- `tools/capture`: USBPcap helper scripts ([workflow](docs/capture-workflow.md))

## Contributing and security

See [CONTRIBUTING.md](CONTRIBUTING.md) for what a change needs: an
observation behind every protocol fact, fixtures instead of captures, and
the checks to run. Report vulnerabilities privately as described in
[SECURITY.md](SECURITY.md). When reporting problems, mask serial numbers
and do not attach raw USB captures.

## License

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE)
or the [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
