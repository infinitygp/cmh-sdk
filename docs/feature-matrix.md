# Feature matrix

Legend: **done** = implemented and verified on hardware, **decoded** =
protocol confirmed but not implemented, **open** = not yet investigated,
**n/a** = module not available.

Last update: 2026-10-12.

| Feature | Base | 15 LCD Keys | 3 Knobs | 2 Rollers | Dial Display | 5 Faders |
|---|---|---|---|---|---|---|
| Enumeration by VID/PID + serial | done | done | done | done | done | n/a |
| HID report descriptor | done | done | done | done | done | n/a |
| Raw input sniffing | done | done | done | done | done | n/a |
| Input framing | done | open | open | open | open | n/a |
| Typed input events | open | open | open | open | open | n/a |
| Serial / firmware via feature reports | open | open | open | open | open | n/a |
| Software mode / init sequence | open | open | open | open | open | n/a |
| Images | - | open | - | - | open | - |
| Brightness / LEDs | open | open | open | open | open | n/a |
| Hot-plug | open | open | open | open | open | n/a |
| Physical position | open | open | open | open | open | n/a |

## Open questions

- Is the Base heartbeat spontaneous, and what does its payload byte mean?
- Do modules need an init / software-mode command before they report input?
- Does the hub port path map to a fixed grid cell on the base?
- Everything about the 5 Faders module (no unit connected so far).
