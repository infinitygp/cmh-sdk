# cmh-sdk

A Rust SDK for the Cooler Master MasterHUB modular control deck: module
discovery and hot-plug, typed input events, pictures on the 15 LCD Keys
and the Dial Display, LED control on the Base, 5 Faders and 3 Knobs, and
the position of each module on the base. Independent; not affiliated with
Cooler Master.

- **Platforms:** Windows 11 and Linux, both checked on hardware. macOS is
  not supported.
- **Minimum Rust version:** 1.88.
- **License:** MIT OR Apache-2.0.

## Example

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

`Deck::poll` sleeps until something happens. Commands go to a module
through `deck.module(&id)`: `set_picture` / `set_image`,
`set_display_brightness`, `set_led_mode`, `set_led_color`,
`set_led_brightness` and a few queries. `Deck` switches the display
modules to software mode when they connect; they send no input and show
no pictures without it. Input waits in a bounded queue between polls, and
input that does not fit is reported as `DeckEvent::InputLost`.

## Cargo features

| Feature | Default | Adds |
|---|---|---|
| `image` | no | `Module::set_picture` and `cmh_sdk::picture`: PNG and JPEG files, or RGBA pixels, converted to the JPEG form the displays accept. Pulls in the `image` crate. |
| `test-support` | no | `cmh_sdk::simulator`: simulated modules that drive a real `Deck`, for testing an application without the hardware. Meant for `[dev-dependencies]`. |

## Linux

Building needs a C compiler, `pkg-config` and the libudev development
files (`libudev-dev` on Debian and Ubuntu, `systemd-devel` on Fedora,
`systemd-libs` on Arch Linux).

The modules' hidraw nodes belong to root. To open them as the user logged
in at the desktop, install this rule as
`/etc/udev/rules.d/70-masterhub.rules`; it matches the six MasterHUB
product IDs only:

```udev
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2516", ATTRS{idProduct}=="01f6|01fe|0200|0202|0204|0206", TAG+="uaccess"
```

For a service or a system without a desktop session, replace
`TAG+="uaccess"` with `GROUP="masterhub", MODE="0660"` and add the
service's account to that group. Then run
`sudo udevadm control --reload` and
`sudo udevadm trigger --action=change --subsystem-match=hidraw`, or
replug the modules. Earlier hardware checks used a vendor-wide rule; this
narrower form has not itself been tested on hardware.

## Safety

The SDK sends only commands whose effect was verified on a device and
offers no way to send an arbitrary report. It cannot update or recover
firmware, reset a module, change a serial number or write LED settings to
a module's permanent memory.

## Documentation

The API reference is on [docs.rs](https://docs.rs/cmh-sdk). The
[repository](https://github.com/infinitygp/cmh-sdk) has the getting-started
guide, the API guide (event ordering, reconnection, threads, errors,
testing without hardware), troubleshooting, the feature matrix with the
verification status of every feature, the protocol reference and examples.

## License

Licensed under either of the Apache License, Version 2.0 (`LICENSE-APACHE`)
or the MIT license (`LICENSE-MIT`), at your option.
