# Feature matrix

Legend:

- **done**: implemented in the SDK and verified on hardware
- **done (wire)**: implemented and verified on the USB wire; the visible
  result still needs a person to confirm
- **open**: not investigated or not implemented
- **n/a**: does not apply to this module

Last update: 2026-10-12. Platform: Windows 11 only. Firmware: Base
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
| Display brightness | n/a | open | n/a | n/a | open | n/a |
| LED effect / colour / brightness | done (wire) | n/a | open | n/a | n/a | done (wire) |
| Hot-plug (polling, identity by serial) | done | done | done | done | done | done |
| Physical position on the base | open | open | open | open | open | open |

Hot-plug was verified by exchanging modules on a powered base (experiment
0005); the automatic re-open path of `cmh monitor` after an I/O error is
covered by unit tests only.

## Tools

| Tool | Purpose |
|---|---|
| `cmh list [--descriptors]` | modules, HID interface data, report descriptors |
| `cmh info` | capabilities, firmware, device info, fader positions |
| `cmh monitor [--raw] [--heartbeat]` | live typed events, optional fixture-format hex, follows hot-plug |
| `cmh replay <fixture>` | decode recorded reports without hardware |
| `cmh software-mode on\|off` | software mode of a display module |
| `cmh image [--key N] <file.jpg>` | show a JPEG on a key or on the dial |
| `cmh led [--index N] [--mode M] [--color RRGGBB] [--brightness N]` | LED effect, colour, brightness |
| `tools/capture/*.ps1` | USBPcap capture daemon, client and fixture export |

## Open questions

1. Physical meaning of indexes and directions: knob order, roller order and
   direction, dial direction, fader order and direction.
2. Does the dial display react to touch? Can the rollers be pressed?
3. Do the display modules report input when they are not in software mode,
   and what do they show then? What does `03 3a 00` change?
4. What does the heartbeat status byte mean?
5. Display brightness of the keys and the dial: no control found in the
   vendor application's UI. LED rings of the 3 Knobs: module code not
   observed yet. Do LED settings sent without the persist flag survive a
   power cycle?
6. Does the hub port path of a module map to a fixed position on the base?
7. JPEG tolerance of the display modules: chroma subsampling, progressive
   mode, maximum size. The SDK accepts only what was seen working.
8. Behaviour on a clean exit of the vendor application.
9. Two modules of the same model at once (logic is unit-tested, no second
   unit available).
10. Linux: nothing has been built or run there yet; see
    [linux-udev.md](linux-udev.md).
11. Threading model for consumers that read input and draw images at the
    same time (currently: one owner thread with short read timeouts).
