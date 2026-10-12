# Feature matrix

Legend:

- **done**: implemented in the SDK and verified on hardware
- **open**: not investigated or not implemented
- **n/a**: does not apply to this module

Last update: 2026-10-12. Every "done" below was verified on Windows 11.
On Linux (WSL2 through usbipd-win, experiment 0012) the 3 Knobs and the
15 LCD Keys were verified for enumeration, feature reads, input, LEDs,
images, brightness and hot-plug; the other modules and the mount path were
not. Firmware: Base
`02.00.04.00`, Keys `02.00.01.00`, Knobs `01.00.05.00`, Rollers
`01.00.03.01`, Dial `01.00.04.00`, Faders `01.00.05.00`.

| Feature | Base | 15 LCD Keys | 3 Knobs | 2 Rollers | Dial Display | 5 Faders |
|---|---|---|---|---|---|---|
| Enumeration by VID/PID + serial | done | done | done | done | done | done |
| Firmware version | done | done | done | done | done | done |
| Device info | done | done | done | done | done | done |
| Raw input monitoring | done | done | done | done | done | done |
| `ButtonDown` / `ButtonUp` | n/a | done | done | n/a | done | n/a |
| `EncoderDelta` | n/a | n/a | done | done | done | n/a |
| `AnalogChanged` | n/a | n/a | n/a | n/a | n/a | done |
| Initial analog positions | n/a | n/a | n/a | n/a | n/a | done |
| Base orientation (flat / upright) | done | n/a | n/a | n/a | n/a | n/a |
| Software mode switch | n/a | done | n/a | n/a | done (on only) | n/a |
| Image upload | n/a | done | n/a | n/a | done | n/a |
| Display brightness | n/a | done | n/a | n/a | done | n/a |
| LED effect / colour / brightness | done | n/a | done | n/a | n/a | done |
| Hot-plug (polling, identity by serial) | done | done | done | done | done | done |
| Mount path (USB port path below the base) | done | done | done | done | done | open |
| Grid cells and rotation from the mount path | n/a | done | done | done | done | done |

Hot-plug was verified by exchanging modules on a powered base (experiments
0005, 0009), including recovery from a contact bounce while a module was
being seated (experiment 0011).

## Library surface

| Part | Status |
|---|---|
| `Deck`: discovery, hot-plug, one event stream, commands by module ID | done on hardware (Windows), poll timing measured (experiment 0016); logic covered by tests with a scripted backend |
| `Manager` / `Module`: listing and single-module access | done |
| `placement`: cells and rotation | done |
| `picture` (cargo feature `image`): PNG / JPEG / pixels to the displays' JPEG | done on the keys; conversion output is checked against the upload rules in tests |
| Closed command set | enforced by the API and by a test listing every report that can be emitted |

## Tools

| Tool | Purpose |
|---|---|
| `cmh list [--descriptors]` | modules, HID interface data, USB port chain, mount path and placement on the base, report descriptors |
| `cmh info` | capabilities, firmware, device info, fader positions |
| `cmh monitor [--raw] [--heartbeat]` | live typed events, optional fixture-format hex, follows hot-plug |
| `cmh replay <fixture>` | decode recorded reports without hardware |
| `cmh software-mode on\|off` | software mode of a display module |
| `cmh image [--key N] <file>` | show a PNG or JPEG on a key or on the dial |
| `cmh led [--index N] [--mode M] [--color RRGGBB] [--brightness N]` | LED effect, colour, brightness |
| `cmh brightness <0..100>` | backlight of the keys or the dial display |
| `tools/capture/*.ps1` | USBPcap capture daemon, client and fixture export |

## Open questions

1. Base orientation values other than flat and upright on the short side.
2. Two bases at once, and two modules of the same model: handled in tests
   with scripted devices only; no second unit was available.
3. Position on the base: purpose of the 2 x 6 pin block; whether other base
   units are wired the same as the one examined
   ([protocol/placement.md](protocol/placement.md)).
4. JPEG tolerance of the display modules: chroma subsampling, progressive
   mode, maximum size. The SDK accepts only what was seen working.
5. Behaviour on a clean exit of the vendor application (only a kill was
   observed).
6. Linux: mount path on a natively installed Linux (USB/IP hides the
   topology); Base, Rollers, Dial and Faders have not been tried there;
   see [linux-udev.md](linux-udev.md).
7. The declared minimum Rust version (1.88) follows from the language
   features used; it has not been built with exactly that toolchain.
8. Feature reports the modules declare but the vendor application never
   reads (probably LED settings and serial read-back), and the commands
   listed in the
   [protocol reference](protocol/README.md#commands-that-must-never-be-sent)
   as destructive: deliberately not touched.
