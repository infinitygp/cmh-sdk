# API guide

How the parts of `cmh-sdk` fit together and what they promise. The API
reference on docs.rs has the details of every type and method.

## Two levels

| Use | When |
|---|---|
| `Deck` | Almost always. It finds the modules, opens them, prepares the display modules, follows hot-plug and turns all input into one stream of `DeckEvent`s. Commands go through `deck.module(&id)`. |
| `Manager` and `Module` | Diagnostics and tools that open one module on purpose, read it synchronously and close it again. Nothing happens in the background. |

Both offer the same closed set of commands; neither can send an arbitrary
report to a module.

## The deck

```rust,no_run
use std::time::Duration;

use cmh_sdk::{Deck, DeckOptions};

let mut deck = Deck::with_options(DeckOptions::default())?;
let events = deck.poll(Duration::from_millis(100))?;
# Ok::<(), cmh_sdk::Error>(())
```

`poll(timeout)` sleeps until something happens and returns everything that
happened, oldest first. An empty list means that the timeout elapsed or a
`DeckWaker` was used; it says nothing about the modules.
`Duration::ZERO` checks without waiting and `Duration::MAX` waits as long as
it takes. `poll` fails only when the HID backend itself fails; problems of a
single module are events.

If reading the device list fails, `poll` returns that error, and nothing
that already happened is lost: the events gathered before the failure,
including what a partly done rescan found (modules connected or gone), are
returned first by the next poll, each once. The list is read again within a
second, so keep polling after such an error.

Between polls, `deck.modules()` lists the open modules, `deck.info(&id)`
describes one and `deck.module(&id)` gives a `&Module` for drawing,
lighting and queries. Input stays with the deck: it arrives only through
`poll`.

### Events

| Event | Meaning | Typical reaction |
|---|---|---|
| `Connected(info)` | A module was opened and prepared. | Look at `info.kind`, draw its pictures, set its LEDs. |
| `Disconnected(info)` | A module was pulled, moved or stopped answering. | Forget its state; it may come back with `Connected`. |
| `Relocated(info)` | The place of an open module on the base became known or changed without a reconnect. | Update a layout view. |
| `Input { id, kind, event, at }` | A control was operated. `at` is when the report was read, not when `poll` returned. | Act on it. |
| `InputLost { id, kind, dropped }` | Input of the module did not fit in the queue; see below. | Treat held buttons and encoder positions as uncertain until the events that follow. |
| `RawInput { .. }` | The report behind input, only with `DeckOptions::raw_reports(true)`. | Diagnostics. |
| `Undecodable { .. }` | A report could not be decoded; the module stays usable. | Log it. |
| `OpenFailed { id, interface, error }` | A listed module could not be opened, or a display module did not confirm software mode. Reported once; the deck keeps trying. | Show the error, for example missing permissions. |

`DeckEvent`, `InputEvent`, `Error` and most other public types are
`#[non_exhaustive]`: match them with a `_` arm, because new variants may be
added.

### Ordering and the queue

Every open module has a reader thread that waits in a blocking read and
hands each report to the deck with the time it was read. Reports wait for
the next poll in a queue that holds at most `Deck::MAX_PENDING_REPORTS`
(256) reports per module. A moving fader sends about 60 reports a second,
so a deck that is polled every few seconds or more often never runs out of
room. A reader thread never waits for the application.

The deck guarantees:

- The events of one module come in the order the module sent them.
- Everything a module sent before it left comes before its
  `Disconnected`, and nothing read on one connection is delivered after
  the module connected again.
- When the queue of a module is full, the deck stops queueing its reports
  and counts them instead. The next poll delivers what was queued, then
  `InputLost` with the number of reports that were not delivered, then
  `Input` events that bring the buttons, the faders and the orientation of
  the base to the last state the module reported. A press and release that
  both fell into the gap is not reported, and encoder steps in the gap are
  lost: nothing is made up. After that, input flows as usual.
- Wake-ups and the operating system's device announcements do not pile up:
  any number of them between two polls counts as one.

Events of different modules are delivered in the order they were read,
except that the events following an `InputLost` come after the other
modules' events of the same poll.

### Options

`DeckOptions` is a builder:

| Option | Default | Effect |
|---|---|---|
| `software_mode(bool)` | `true` | Switch the 15 LCD Keys and the Dial Display to software mode when they connect. Without it they send no input and show no pictures, and the deck never writes to a module on its own. |
| `raw_reports(bool)` | `false` | Also deliver every report as `RawInput`. |
| `device_events(bool)` | `true` on Linux, `false` elsewhere | Learn of arriving and leaving modules from the operating system instead of reading the device list on a timer. Linux uses an inotify watch of `/dev`; the Windows notifications are implemented but have not been run on hardware, hence the default. |
| `rescan_interval(Duration)` | 1 s without device events, 60 s with them | How often the device list is read regardless; at least `DeckOptions::MIN_RESCAN_INTERVAL` (100 ms). |
| `filter(Fn(&HidInterface) -> bool)` | everything | Open only the modules the closure accepts. |

```rust,no_run
use cmh_sdk::{Deck, DeckOptions, ModuleKind};

// Leave the display modules to another program.
let options = DeckOptions::default()
    .filter(|interface| interface.kind.capabilities().display.is_none());
let deck = Deck::with_options(options)?;
# Ok::<(), cmh_sdk::Error>(())
```

The closure sees each module's `HidInterface`, including its kind and
serial number, before the module is opened.

### Threads and waking

`Deck` is `Send`: create it on one thread or move it there, and poll it in
a loop. Commands through `deck.module(&id)` are synchronous and run
between polls on the same thread; keep slow work, such as preparing many
pictures, off that thread so that input is handed over promptly.

To make a waiting `poll` return early, for example when another part of
the application has work for the deck's thread, use a `DeckWaker`:

```rust,no_run
use std::time::Duration;

use cmh_sdk::Deck;

let mut deck = Deck::new()?;
let waker = deck.waker();
std::thread::spawn(move || {
    // ... when there is something for the deck's thread to do:
    waker.wake();
});
let events = deck.poll(Duration::MAX)?; // returns once woken
# Ok::<(), cmh_sdk::Error>(())
```

In an async application, run the poll loop on a dedicated thread (or a
blocking task) and pass events through a channel of your runtime; send
commands back the same way and use the waker to have them picked up.

Each reader thread ends with its module. After a module is closed or the
deck is dropped, its reader thread finishes the read it is in, which takes
up to 2 seconds on Linux and 250 ms on Windows.

## Identity and placement

`ModuleId` is the module's USB serial number, the same across
reconnects, restarts and places on the base. Every module seen so far has
one; if one had none, its operating system path would stand in, which does
not survive a reconnect. Store serial numbers, never paths.

Two modules of the same model are told apart by their serial numbers.
Two bases at once, or two modules of the same model, have only been
exercised with scripted devices in the tests, not with real units.

`ModuleInfo::mount_path` and `ModuleInfo::placement` describe where a module
sits on the base: the USB port path below the base, and the grid cells and
rotation derived from it. Both are `None` when the base is not connected
or the platform hides the USB topology (as it does under WSL2). A module
that is moved disconnects and connects again with its new place; a base
that is found after a module produces `Relocated`.

## Input

- Indexes start at 0. The verified order for every module is documented on
  `InputEvent`.
- `ButtonDown` and `ButtonUp` come in pairs while the module stays
  connected. After a reconnect the state starts over: do not assume a
  button that was held before is still held.
- `EncoderDelta` is one detent: `+1` clockwise on the knobs and the dial,
  towards the operator on the rollers.
- `AnalogChanged` carries a fader position, `0..=100`. The deck reads the
  positions when the 5 Faders connect and delivers them right after
  `Connected`. A fader resting between two positions may alternate.
- `OrientationChanged` reports the base standing upright on its short side
  or lying flat, from its heartbeat; the first one follows the first
  heartbeat after connecting.

## Displays

The 15 LCD Keys have 15 displays of 72 x 72 pixels, targets `0..=14`; the
Dial Display has one of 240 x 240 pixels, target `0`.

| Method | Input |
|---|---|
| `Module::set_picture(target, file)` (feature `image`) | PNG or JPEG file bytes. Stretched to the display; transparent areas become black; sources above 4096 pixels on a side are refused. |
| `Module::set_image(target, jpeg)` | A JPEG the display accepts as it is: baseline, three components, exactly the display size, 4:4:4, 4:2:2 or 4:2:0, at most 18 kB. Nothing is sent otherwise. |
| `Module::check_image(target, jpeg)` | Checks a JPEG like `set_image`, without sending it. |
| `picture::convert`, `picture::encode_rgba` (feature `image`) | Prepare a JPEG from a file or from RGBA pixels, for example ahead of time. |
| `Module::set_display_brightness(level)`, `display_brightness()` | Backlight `0..=100`, read back until the module reports it. |

Uploads are synchronous. A key takes several hundred small images a
second; the Dial Display about 60. Thirty images a second on the keys
together with 20 on the dial were sustained for five minutes without
losing input. Pace animations yourself.

The display modules need software mode for input, pictures, the backlight
and their firmware version. `Deck` enables it; with `Manager` call
`Module::set_software_mode(true)` yourself. The modules forget pictures
and software mode when they lose power.

## LEDs

| Module | LED zones |
|---|---|
| Base | 1 |
| 5 Faders | 5 |
| 3 Knobs | 3 |

`set_led_mode`, `set_led_color` and `set_led_brightness` set one zone, by
index; `ModuleKind::capabilities().leds` says how many there are. The
effects are those of the vendor application (`LedMode`); the colour applies
to the effects that use one. `led_brightness()` reads the brightness of
every zone back; the running effect and colour cannot be read, so keep
them in your application. The SDK never stores LED settings in a module's
permanent memory, and they are lost on power-up.

## Errors and recovery

All fallible calls return `cmh_sdk::Result<T>` with `cmh_sdk::Error`:

| Error | Meaning | What to do |
|---|---|---|
| `PermissionDenied` | The system refused to open the device. | On Linux, install a udev rule ([Linux permissions](linux-permissions.md)). |
| `Disconnected` | The module went away during the call. | With a deck, carry on: the next poll reports `Disconnected`. |
| `NotInSoftwareMode` | A display module was asked for something that needs software mode, or did not confirm the switch. | Enable software mode; see troubleshooting if it keeps failing. |
| `BrightnessNotApplied` | The display kept reporting another backlight level. | Try again later. |
| `Unsupported` | The module does not have that capability. | Check `ModuleKind::capabilities()` first. |
| `InvalidArgument` | An index, level or picture was out of range or not accepted. | Fix the call; nothing was sent. |
| `MalformedReport` | A report did not have the documented shape. | Not fatal. |
| `WrongDevice` | A path now leads to another device. | The deck reports it as `OpenFailed` and tries again; with `Manager`, refresh and open again. |
| `NotFound` | `Manager::open` found no module with that serial. | |
| `Backend` | The HID library failed. | From `poll`: keep polling; no event is lost and the device list is read again within a second. Report it if it persists. |

A command that fails part-way, such as a picture upload interrupted by
unplugging, may have reached the module in part. Commands are not
transactions: on `Connected`, set everything the module should show.

## Testing without hardware

Two facilities let you test code without the modules.

**Simulated modules** (cargo feature `test-support`). `cmh_sdk::simulator`
provides a simulated bus: plug modules in, operate their controls, pull
them, and drive your application with a real `Deck` that sees them. The
deck runs the same code as on hardware, so your tests see the same events
in the same order, including `Connected`, `Disconnected` and the fader
positions after connecting. Outputs can be checked as decoded LED commands
and as the pictures a module received.

```toml
[dev-dependencies]
cmh-sdk = { version = "0.1", features = ["test-support"] }
```

```rust,ignore
use std::time::Duration;

use cmh_sdk::simulator::Simulator;
use cmh_sdk::{DeckEvent, DeckOptions, InputEvent, ModuleKind};

#[test]
fn a_key_press_reaches_the_application() -> cmh_sdk::Result<()> {
    let bus = Simulator::new();
    let keys = bus.plug(ModuleKind::Keys15, "KEYS-1");
    let mut deck = bus.deck(DeckOptions::default());
    deck.poll(Duration::from_secs(1))?; // Connected

    keys.press(3);
    let events = deck.poll(Duration::from_secs(1))?;
    assert!(matches!(
        &events[..],
        [DeckEvent::Input { event: InputEvent::ButtonDown { index: 3 }, .. }]
    ));
    Ok(())
}
```

An input method returns once the deck has the report, so the next poll has
it. Plugging and pulling is announced to the deck, which looks at the bus
a few milliseconds later: poll with a timeout to see it. The simulation
has no placement on a base, and the reads it does not model fail with
`Error::Backend`. A passing simulated test says nothing about a particular
device, operating system or firmware. A complete example is
`crates/cmh-sdk/tests/simulated_application.rs` in the repository.

**Recorded reports.** `InputDecoder` turns raw input reports into
`InputEvent`s, and `fixture::Fixture` reads the text format that
`cmh monitor --raw` writes. Together they replay a recorded session
through the decoder, which is how the SDK's own tests check decoding
against hardware recordings.

## Manager and Module

```rust,no_run
use std::time::Duration;

use cmh_sdk::Manager;

let mut manager = Manager::new()?;
manager.refresh()?;
for interface in manager.modules() {
    println!("{} {:?} at {:?}", interface.kind, interface.serial, manager.placement(interface));
}
if let Some(serial) = manager.modules().first().and_then(|i| i.serial.clone()) {
    let mut module = manager.open(&serial)?;
    if module.kind().capabilities().display.is_some() {
        // Input from the display modules needs software mode.
        module.set_software_mode(true)?;
    }
    let events = module.read_events(Duration::from_millis(500))?;
    println!("{events:?}");
}
# Ok::<(), cmh_sdk::Error>(())
```

`Manager::refresh()` reads the device list and returns `HotplugEvent::Arrived`
and `HotplugEvent::Left`; nothing is watched in the background. A `Module`
from `open` or `open_interface` owns its handle and decodes its own input
with `read_events`. It does not switch software mode on by itself.

## Hardware safety

The SDK sends only commands whose effect was verified on a device. It has
no way to send an arbitrary report, update or recover firmware, reset a
module to factory settings or change a serial number, and it does not
write LED settings to a module's permanent memory. The test
`emits_only_the_verified_command_set` lists every report the crate can
send. New capabilities are added as typed methods once they are verified;
see [CONTRIBUTING.md](../CONTRIBUTING.md) and the
[protocol reference](protocol/README.md).
