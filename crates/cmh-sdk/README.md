# cmh-sdk

Independent, cross-platform Rust SDK for the Cooler Master MasterHUB modular
control deck (Windows and Linux, HID via `hidapi`). The protocol was derived
from controlled USB observations of real hardware; every feature the SDK
offers was verified on a device.

Not affiliated with Cooler Master. Firmware update, recovery, factory reset
and serial rewrite commands are intentionally out of scope, and the public
API cannot send a raw report.

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

The cargo feature `image` converts PNG / JPEG files and raw pixels into the
JPEG flavour the displays accept. On Linux the `hidapi` backend needs a C
compiler, `pkg-config` and the libudev headers, and access to the devices
needs a udev rule. The protocol notes, the feature matrix and the experiments
behind every fact are in the [repository](https://github.com/infinitygp/cmh-sdk).

## License

Licensed under either of the Apache License, Version 2.0 (`LICENSE-APACHE`)
or the MIT license (`LICENSE-MIT`), at your option.
