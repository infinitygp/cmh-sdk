# 0009: display brightness, knob order, dial direction, knob LEDs

- Date: 2026-10-12
- Tools: `cmh monitor --raw` and USBPcap running while a person operated the
  device; vendor application 1.0.0.113 running
- Captures (local only): `20261012-130807-user-session-1.pcapng`,
  `20261012-131311-led-knobs.pcapng`,
  `20261012-13*-sdk-brightness-knob-led.pcapng`
- Evidence:
  [fixtures/inputs/clockwise-checks.txt](../../fixtures/inputs/clockwise-checks.txt),
  [fixtures/display/vendor-app-brightness.txt](../../fixtures/display/vendor-app-brightness.txt),
  the 3 Knobs block of [fixtures/leds/vendor-app-lighting.txt](../../fixtures/leds/vendor-app-lighting.txt)
- Layout: keys top-left, dial top-right, knobs bottom-right with the single
  knob of the triangle at the top, rollers bottom-left

## Procedure (performed by a person)

1. Remove the 5 Faders, mount the dial and the knobs.
2. One detent clockwise on: top knob, bottom-left knob, bottom-right knob,
   dial.
3. Top knob (bound to "brightness" in the vendor application): three
   detents down, three up.

## Observations

Hot-plug, as logged by `cmh monitor`:

```text
#   118.793 lost    5 Faders (module disconnected: ... device is not connected)
#   125.114 arrived Dial Display
#   127.219 arrived 3 Knobs
```

The vendor application reacted to the dial arriving with its usual start
sequence (`get 4b`, `set 03 49 01`, `get 48`, images).

One detent clockwise:

| Control | Report payload | Decoded before this experiment |
|---|---|---|
| top knob | `00 03 ff ff 01` | knob 2, +1 |
| bottom-left knob | `00 03 01 ff ff` | knob 0, +1 |
| bottom-right knob | `00 03 ff 01 ff` | knob 1, +1 |
| dial | `03 01 00` | -1 |

Brightness knob (each detent, both display modules at once):

| Time | Keys | Dial |
|---|---|---|
| 160.4 | `03 34 32` | `03 44 32` |
| 160.9 | `03 34 2d` | `03 44 2d` |
| 161.7 | `03 34 28` | `03 44 28` |
| 164.7 | `03 34 2d` | `03 44 2d` |
| 165.3 | `03 34 32` | `03 44 32` |
| 166.2 | `03 34 37` | `03 44 37` |

These are feature writes on report `0x03`. The knob itself only produced its
normal rotation reports; the application did the rest. `03 34 37` /
`03 44 37` was also sent once when the modules had settled after the swap.

Opening the 3xKnob lighting panel of the application:

```text
02 2d 02 <i> 74 3a df      i = 0, 1, 2
02 2e 02 <i> 32
02 2c 02 <i> 01 00
```

SDK check: `cmh led --module knobs` set zones 0, 1, 2 to red, green, blue;
`cmh brightness` set the keys to 15 and the dial to 100. On the wire:
`02 2d/2c/2e 02 <i> ...`, `03 34 0f`, `03 44 64`, all successful. Level 101
and the knobs module were refused without sending anything.

## Conclusions

- Knob index order: 0 = left, 1 = right, 2 = the single knob, seen with the
  single knob pointing away from the operator. CONFIRMED.
- Dial rotation codes are the reverse of the knobs: `00` is clockwise.
  CONFIRMED (this run, and consistent with experiment 0002 where the dial
  was turned clockwise first). The SDK now reports clockwise as `+1` on
  both.
- Display brightness: `03 34 <level>` on the 15 LCD Keys and `03 44 <level>`
  on the Dial Display, level in percent. CONFIRMED for 40..55 as sent by the
  vendor application; the full 0..100 range is LIKELY.
- LED commands of the 3 Knobs use module code `02` with three zones.
  CONFIRMED on the wire.
- An I/O error on a pulled module surfaces as `Error::Disconnected`, and
  `cmh monitor` picks up the replacement modules: CONFIRMED on Windows.

## Open questions

- Visual confirmation of the SDK run: which knob shows which colour, and is
  the key display visibly dimmer than the dial.
- Does brightness 0 switch a display off completely?
