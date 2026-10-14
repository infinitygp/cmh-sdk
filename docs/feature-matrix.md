# Feature matrix

Legend:

- **done**: implemented in the SDK and verified on hardware
- **open**: not investigated or not implemented
- **n/a**: does not apply to this module

Last update: 2026-10-14. Every "done" below was verified on Windows 11,
except where a row says otherwise.
On Linux (WSL2 through usbipd-win, experiment 0012) the 3 Knobs and the
15 LCD Keys were verified for enumeration, feature reads, input, LEDs,
images, brightness and hot-plug. On a natively installed Linux (experiment
0020) enumeration, mount path, placement, firmware, input and hot-plug
were verified for every module except the 5 Faders, together with LEDs on
the Base and software mode, image upload and display brightness on the
keys and the dial; the 5 Faders followed in experiment 0022 and the LEDs
of the 3 Knobs in experiment 0023. On the keys and the dial the firmware version, the
display brightness, images and input need software mode (experiment 0021).
Firmware: Base
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
| Software mode switch, on and off | n/a | done | n/a | n/a | done | n/a |
| Image upload | n/a | done | n/a | n/a | done | n/a |
| Sustained animation, upload rate measured (0025) | n/a | done | n/a | n/a | done | n/a |
| Display brightness, set and read back | n/a | done | n/a | n/a | done | n/a |
| LED effect / colour / brightness | done | n/a | done | n/a | n/a | done |
| LED brightness read back | done | n/a | done | n/a | n/a | done |
| Hot-plug (polling, identity by serial) | done | done | done | done | done | done |
| Mount path (USB port path below the base) | done | done | done | done | done | done |
| Grid cells and rotation from the mount path | n/a | done | done | done | done | done |

Hot-plug was verified by exchanging modules on a powered base (experiments
0005, 0009), including recovery from a contact bounce while a module was
being seated (experiment 0011).

## Library surface

| Part | Status |
|---|---|
| `Deck`: discovery, hot-plug, one event stream, commands by module ID | done on hardware (Windows; on Linux across a power cycle, experiment 0021); logic covered by tests with a scripted backend. Input is read by a thread per module and `poll` sleeps (experiment 0029): idle cost and ordered, timed heartbeats measured on Linux (open: presses, Windows) |
| `Deck`: discovery by announcement of the system (`DeckOptions::device_events`) | Linux (inotify on `/dev`): the watch is tested, the idle cost measured, an arrival or departure seen by it open (experiment 0029). Windows (device interface notifications): written, compiles, never run, off by default (open) |
| `Manager` / `Module`: listing and single-module access | done |
| `placement`: cells and rotation | done |
| `picture` (cargo feature `image`): PNG / JPEG / pixels to the displays' JPEG | done on the keys; conversion output is checked against the upload rules in tests |
| Closed command set | enforced by the API and by a test listing every report that can be emitted |

## Tools

| Tool | Purpose |
|---|---|
| `cmh list [--descriptors]` | modules, HID interface data, USB port chain, mount path and placement on the base, report descriptors |
| `cmh info [--software-mode]` | capabilities, firmware, device info, fader positions; the option switches the keys and the dial to software mode, without which they give neither firmware nor brightness |
| `cmh monitor [--raw] [--heartbeat]` | live typed events, optional fixture-format hex, follows hot-plug |
| `cmh replay <fixture>` | decode recorded reports without hardware |
| `cmh software-mode on\|off` | software mode of a display module |
| `cmh image [--key N] <file>` | show a PNG or JPEG on a key or on the dial |
| `cmh led [--index N] [--mode M] [--color RRGGBB] [--brightness N]` | LED effect, colour, brightness |
| `cmh brightness <0..100>` | backlight of the keys or the dial display; switches the module to software mode |
| `tools/capture/*.ps1` | USBPcap capture daemon, client and fixture export |

## Open questions

1. Base orientation values other than flat and upright on the short side.
2. Two bases at once, and two modules of the same model: handled in tests
   with scripted devices only; no second unit was available.
3. Position on the base: purpose of the 2 x 6 pin block; whether other base
   units are wired the same as the one examined; how far a module can hang
   over the edge (experiment 0028)
   ([protocol/placement.md](protocol/placement.md)).
4. JPEG tolerance of the display modules: the maximum size (above 19 kB
   on the keys, above 28 kB on the dial). Both take 4:2:2 and 4:2:0; the
   keys take neither progressive nor greyscale, which was not tried on the
   dial (experiment 0019). The SDK accepts only what was seen working.
5. What the vendor's background process (`MHDaemon.exe`) does on its
   own: it keeps running after the application exits and is the likely
   sender of the software mode command the keys receive at that moment
   (experiment 0026).
6. Display modules in standalone mode (experiment 0021): why a firmware
   or brightness read there delays the feature writes that follow, and
   why a brightness level sent right after entering software mode can be
   lost. The keys behave the same on Windows (experiment 0024); there a
   module in that state also reports wrong serial and product strings.
   `Deck` reads the listing again after enabling software mode and
   connects the module under its real serial; `Manager`, and a deck that
   leaves the mode alone, show what the system lists.
7. The declared minimum Rust version (1.88): the workspace builds and its
   tests pass with rustc 1.88.0 on Windows; not tried with that toolchain
   on Linux.
8. Feature reports whose bytes are known but not understood (experiment
   0018). The commands listed in the
   [protocol reference](protocol/README.md#commands-that-must-never-be-sent)
   as destructive were deliberately not touched.
9. The running LED effect and colour cannot be read back; a host has to
   remember what it set.
10. Storing an LED colour in a module, which the vendor application does
    after its colour picker is closed, is understood (experiment 0027) and
    deliberately not offered: it is a write to the module's permanent
    memory, and only the colour is kept.
11. The event-driven deck (experiment 0029) on a real re-plug on Linux, and
    on Windows altogether: the device notifications, a second handle on a
    module opened by the same process, the reader threads, and the end of
    a read when a module is pulled. The Faders and a second unit of a model
    were not on the machine.
