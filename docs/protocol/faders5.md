# 5 Faders (M005S)

- USB `2516:0200`, product string `5xFader, 4x2`, serial prefix `MHFD05AA42`
- Family: basic (64 byte output report)
- Feature IDs declared: `0x2d` `0x2e` `0x2f` `0x30` `0x31` `0x32` `0x33` `0x34` `0x36`
- Firmware seen: `01.00.05.00`
- Controls: 5 faders, each with an LED strip that follows the knob

## Input: positions

| | |
|---|---|
| Report | ID `0x01`, tag `0x2a`, payload `02 05 v0 v1 v2 v3 v4` |
| Trigger | whenever any fader position changes; carries all five positions |
| Values | `00`..`64` (0..100), all 101 steps occur |
| Rate | about one report per 40 ms while moving, 16 ms at the fastest |
| Initialisation | none needed; works without the vendor application |
| Confidence | CONFIRMED (experiment 0005) |

```text
01 2a 07 00 02 05 64 64 64 64 61      faders 0..3 at 100, fader 4 at 97
01 2a 07 00 02 05 2c 2d 39 3a 38      44, 45, 57, 58, 56
```

Notes:

- Nothing is sent on connect; a module that is not touched stays silent, so
  the position has to be read (see below).
- A fader resting on the boundary between two steps keeps alternating
  between them (seen: 44 / 45 every few seconds). Consumers that react to
  every change should apply their own hysteresis.
- The same type code `02` means "key states" on the 15 LCD Keys.

SDK: `InputEvent::AnalogChanged { index, value }` for every fader whose
position differs from the last known one.

## Feature read: current positions

| | |
|---|---|
| Report | `GET_REPORT` feature `0x33` |
| Response | `33 05 v0 v1 v2 v3 v4` |
| Use | the vendor application reads it on start and then every ~2 s |
| Confidence | CONFIRMED (experiment 0005; values equal the last input report) |

```text
33 05 2c 2d 39 3a 38
```

SDK: `Module::fader_positions()`; it also primes the event decoder so that
only later movement produces events.

## Other feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x2e` | firmware version | `2e 0f 54 e2 61 51 "01.00.05.00"` | CONFIRMED |
| `0x31` | device info | `31 14` + 20 zero bytes | CONFIRMED |

## Index order and direction

LIKELY, from one photo with the module mounted rotated by 90 degrees
(faders horizontal, stacked vertically): index 0 was the topmost fader and
larger values were further to the right. To be pinned down with a dedicated
single-fader test.

## Not investigated

LED colour, mode and brightness (the strips were lit orange by default),
output reports, feature reports `0x2d` `0x2f` `0x30` `0x32` `0x34` `0x36`.
UNKNOWN.
