# cmh-sdk

Independent, cross-platform Rust SDK for the Cooler Master MasterHUB modular
control deck (Windows and Linux, HID via `hidapi`). The protocol was derived
from controlled USB observations of real hardware; every feature the SDK
offers was verified on a device.

Not affiliated with Cooler Master. Firmware update, recovery, factory reset
and serial rewrite commands are intentionally out of scope, and the public
API cannot send a raw report.

## What it does

- Finds modules, follows hot-plug, identifies each module by serial number
- Typed input: `ButtonDown`, `ButtonUp`, `EncoderDelta`, `AnalogChanged`
- Pictures on the 15 LCD Keys and the Dial Display, display brightness
- LED effect, colour and brightness on the Base, 5 Faders and 3 Knobs
- Firmware version, fader positions
- Where every module sits on the base (cells and rotation)

Status per module and the list of open questions:
[docs/feature-matrix.md](docs/feature-matrix.md).

## Use

```rust,no_run
use std::time::Duration;
use cmh_sdk::{Deck, DeckEvent};

let mut deck = Deck::new()?;
loop {
    for event in deck.poll(Duration::from_millis(50))? {
        match event {
            DeckEvent::Connected(info) => println!("+ {} at {:?}", info.kind, info.placement),
            DeckEvent::Disconnected(info) => println!("- {}", info.kind),
            DeckEvent::Input { id, event, .. } => println!("{id}: {event:?}"),
            _ => {}
        }
    }
}
# Ok::<(), cmh_sdk::Error>(())
```

`Deck::module(&id)` gives access to drawing, lighting and queries. The cargo
feature `image` adds conversion of PNG / JPEG files and raw pixels to the
exact JPEG flavour the displays accept. A `Deck` belongs to one thread;
between polls it is free to send commands.

Examples: `crates/cmh-sdk/examples`.

## Layout

- `crates/cmh-sdk`: the library
  - `deck`: all modules as one event stream
  - `manager`, `module`: listing and single-module access
  - `protocol`: packet encoding and decoding, no I/O
  - `placement`: position on the base
  - `picture` (feature `image`): picture conversion
- `crates/cmh-cli`: `cmh`, a diagnostic tool
- `docs/protocol`: [protocol reference](docs/protocol/README.md), one page per module
- `docs/experiments`: what was done to learn each fact
- `fixtures/`: reports recorded from hardware, used by the tests
- `tools/capture`: USBPcap helper scripts ([workflow](docs/capture-workflow.md))

## The `cmh` tool

```sh
cargo run -p cmh-cli -- list                 # modules, mount path, placement
cargo run -p cmh-cli -- info                 # firmware, capabilities
cargo run -p cmh-cli -- monitor --raw        # live events, never writes
cargo run -p cmh-cli -- replay fixtures/inputs/all-controls.txt
cargo run -p cmh-cli -- image --module keys --key 3 picture.png
cargo run -p cmh-cli -- brightness --module keys 60
cargo run -p cmh-cli -- led --module knobs --color ff8000 --mode static
```

## Building

Rust 1.88 or newer. On Linux the `hidapi` backend needs a C compiler,
`pkg-config` and the libudev headers; access to the devices needs a udev
rule, see [docs/linux-udev.md](docs/linux-udev.md).

Verified on Windows 11. On a natively installed Linux every module except
the 5 Faders has been exercised and mount paths match Windows; the 15 LCD
Keys and the Dial Display did not respond for a while after power-up there,
see [docs/linux-udev.md](docs/linux-udev.md#display-modules-after-power-up).

The vendor application and this SDK can read the same modules side by side,
but the application re-sends its own LED settings every minute and redraws
the dial every second, so close it before driving outputs.

## License

Licensed under either of the [Apache License, Version 2.0](LICENSE-APACHE)
or the [MIT license](LICENSE-MIT), at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.
