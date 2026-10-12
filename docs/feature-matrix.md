# Feature matrix

Legend:

- **done**: implemented in the SDK and verified on hardware
- **open**: not investigated or not implemented
- **n/a**: does not apply to this module

Last update: 2026-10-12. Platform: verified on Windows 11; on Linux the
workspace builds and its tests pass, device access is untested. Firmware: Base
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
| Heartbeat decoding | done | n/a | n/a | n/a | n/a | n/a |
| Software mode switch | n/a | done | n/a | n/a | done (on only) | n/a |
| Image upload | n/a | done | n/a | n/a | done | n/a |
| Display brightness | n/a | done | n/a | n/a | done | n/a |
| LED effect / colour / brightness | done | n/a | done | n/a | n/a | done |
| Hot-plug (polling, identity by serial) | done | done | done | done | done | done |
| Mount path (USB port path below the base) | done | done | done | done | done | open |
| Grid position derived from the mount path | open | open | open | open | open | open |

Hot-plug was verified by exchanging modules on a powered base (experiment
0005); the automatic re-open path of `cmh monitor` after an I/O error is
covered by unit tests only.

## Tools

| Tool | Purpose |
|---|---|
| `cmh list [--descriptors]` | modules, HID interface data, USB port chain and mount path, report descriptors |
| `cmh info` | capabilities, firmware, device info, fader positions |
| `cmh monitor [--raw] [--heartbeat]` | live typed events, optional fixture-format hex, follows hot-plug |
| `cmh replay <fixture>` | decode recorded reports without hardware |
| `cmh software-mode on\|off` | software mode of a display module |
| `cmh image [--key N] <file.jpg>` | show a JPEG on a key or on the dial |
| `cmh led [--index N] [--mode M] [--color RRGGBB] [--brightness N]` | LED effect, colour, brightness |
| `cmh brightness <0..100>` | backlight of the keys or the dial display |
| `tools/capture/*.ps1` | USBPcap capture daemon, client and fixture export |

## Open questions

1. Does LED zone index equal knob index (expected)?
2. Does the dial display react to touch?
3. Do the display modules report input when they are not in software mode,
   and what do they show then? What does `03 3a 00` change?
4. What does the heartbeat status byte mean?
5. Display brightness level 0. Do LED and brightness settings survive a
   power cycle?
6. Position on the base: the SDK reports the mount path (experiment 0010);
   the table from mount path to grid cells and orientation is still to be
   built by moving modules around.
7. JPEG tolerance of the display modules: chroma subsampling, progressive
   mode, maximum size. The SDK accepts only what was seen working.
8. Behaviour on a clean exit of the vendor application.
9. Two modules of the same model at once (logic is unit-tested, no second
   unit available).
10. Linux: builds and passes its tests (experiment 0010), but has never
    talked to a module; see [linux-udev.md](linux-udev.md).
11. Threading model for consumers that read input and draw images at the
    same time (currently: one owner thread with short read timeouts).
