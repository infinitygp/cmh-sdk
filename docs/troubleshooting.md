# Troubleshooting

## No module appears

1. Check that the base or the module has power and is connected by USB.
2. List what the system sees. From a checkout of the repository,
   `cargo run -p cmh-cli -- list` prints every MasterHUB module with its
   serial number, mount path and placement. On Linux,
   `grep -l ':00002516:' /sys/class/hidraw/hidraw*/device/uevent` lists the
   hidraw nodes of the vendor.
3. The SDK recognises the six module models in the
   [feature matrix](feature-matrix.md) by their USB product IDs. Other
   Cooler Master devices are ignored.
4. A `DeckOptions::filter` that turns the module down hides it as well.

Under WSL2 every module has to be attached with usbipd-win; see
[linux-udev.md](linux-udev.md#testing-from-wsl2).

## `PermissionDenied` on Linux

The module is listed but the current user may not open its hidraw node.
Install a udev rule as described in [Linux permissions](linux-permissions.md)
and check the node with `ls -l` or `getfacl`. Do not run the application as
root instead.

Right after a module is plugged in, the deck tries a failed open again for
two seconds before it reports `OpenFailed`, because udev applies the rule a
moment after the node appears. An `OpenFailed` that remains means the rule
does not match or has not been loaded.

## The build fails in `hidapi` on Linux

The `hidapi` crate compiles C code against libudev. Install a C compiler,
`pkg-config` and the libudev development files: `libudev-dev` on Debian and
Ubuntu, `systemd-devel` on Fedora, `systemd-libs` on Arch Linux.

## The keys or the dial send no input and show no pictures

The 15 LCD Keys and the Dial Display start in standalone mode after
power-up: they play a built-in animation and ignore the host. `Deck`
switches them to software mode when they connect, unless
`DeckOptions::software_mode(false)` was set. With `Manager`, call
`Module::set_software_mode(true)` yourself.

Other software can switch them back. When the Cooler Master application
exits, it hands the Dial Display back to standalone mode, where it plays
its animation, and switches the keys off and on again, which leaves them
black (experiment [0026](experiments/0026-vendor-app-exit-and-gif.md)).
Call `set_software_mode(true)` on the dial again through `deck.module(&id)`,
or reconnect it, and draw your pictures again. A call that answers
`Error::NotInSoftwareMode` is a sign of this.

If a display module reports `OpenFailed` with `NotInSoftwareMode`, it did
not confirm the switch. The deck keeps trying once a second, each time with
the sequence that brought such a module back in the hardware checks
(experiment [0021](experiments/0021-standalone-mode-reads.md)). Close other
software that talks to the module while it does.

## Pictures or LEDs change back

The Cooler Master application, while it runs, re-sends its LED settings
every minute and redraws the dial every second. Close it, including the
tray icon (closing the window leaves it running), while your program
drives the outputs.

The modules forget pictures and LED settings made through the SDK when
they lose power, which includes being taken off the base. Set your outputs
again on every `Connected`.

## A picture is refused

- `set_picture` (feature `image`) accepts PNG and JPEG files of up to 4096
  pixels on a side and converts them.
- `set_image` accepts only JPEGs in the form the displays take: baseline,
  three components, exactly 72 x 72 pixels for a key or 240 x 240 for the
  dial, 4:4:4, 4:2:2 or 4:2:0, at most 18 kB. Use `check_image` to test a
  JPEG without sending it, or `picture::convert` to produce one.
- Targets are `0..=14` on the 15 LCD Keys and `0` on the Dial Display.
- A very detailed picture may not fit in 18 kB even at the lowest quality
  the converter tries; simplify it.

## Hot-plug is slow

On Linux the deck learns of arriving and leaving modules from the system
as they happen. On Windows it reads the device list once a second by
default; `DeckOptions::device_events(true)` uses the system's notifications
instead, which have not yet been run on hardware.

`Manager` watches nothing: call `refresh()` to see changes.

## `InputLost` events

The application did not call `poll` often enough and more than
`Deck::MAX_PENDING_REPORTS` input reports of one module were waiting. The
events after `InputLost` restore the state of buttons, faders and
orientation; encoder steps in between are gone. Poll more often, and move
slow work such as picture preparation or network requests out of the
thread that polls. See the [API guide](api-guide.md#ordering-and-the-queue).

## Unexpected `Disconnected` or `Undecodable`

A module that is moved on the base disconnects and connects again with its
new place; a module whose contacts bounce while it is being seated can do
so several times. After `Disconnected`, keep polling: the deck reopens the
module and reports `Connected`. `Undecodable` reports a single report that
did not have the documented shape; the module stays usable. To look at the
raw reports, use `DeckOptions::raw_reports(true)` or `cmh monitor --raw`.

## Several bases, or two modules of the same model

Modules are identified by serial number, so two modules of the same model
are kept apart. This and two bases at once have only been exercised with
scripted devices in the tests, not with real units. Reports from such
setups are welcome.

## Reporting a problem

Include the `cmh-sdk` version, the operating system, the module models and
firmware versions (`cmh info --software-mode` prints them), what you did and
the exact error. Mask the last digits of serial numbers, and do not attach
raw USB captures: they can contain traffic of other devices, such as
keystrokes. Report security problems privately as described in
[SECURITY.md](../SECURITY.md).
