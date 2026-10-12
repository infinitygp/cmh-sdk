# 3 Knobs (M003K)

- USB `2516:0202`, product string `3xKnob, 2x2`, serial prefix `MHKB03AA22`
- Family: basic
- Firmware seen: `01.00.05.00`
- Controls: 3 detented rotary encoders, each with a push button

## Input: rotation

| | |
|---|---|
| Report | ID `0x01`, tag `0x2a`, payload `00 03 d0 d1 d2` |
| Trigger | one report per detent |
| Values | `00` counter-clockwise, `01` clockwise, `ff` knob did not move |
| Confidence | CONFIRMED (experiment 0002) |

```text
01 2a 05 00 00 03 00 ff ff      knob 0, one detent counter-clockwise
01 2a 05 00 00 03 ff ff 01      knob 2, one detent clockwise
```

SDK: `InputEvent::EncoderDelta { index, delta }` with `-1` for `00` and `+1`
for `01`.

## Input: push buttons

| | |
|---|---|
| Report | ID `0x01`, tag `0x2a`, payload `03 03 p0 p1 p2` |
| Trigger | on every press and release; carries the state of all three |
| Values | `00` released, `01` pressed |
| Confidence | CONFIRMED (experiment 0002) |

```text
01 2a 05 00 03 03 00 00 01      knob 2 pressed
01 2a 05 00 03 03 00 00 00      all released
```

SDK: `InputEvent::ButtonDown { index }` / `ButtonUp { index }`.

## Index order

The knobs form a triangle. With its single knob pointing away from the
operator, index 0 is the left knob, index 1 the right knob and index 2 the
single knob. CONFIRMED (experiment 0009).

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x2e` | firmware version | `2e 0f a7 df 5b 3f "01.00.05.00"` | CONFIRMED |
| `0x31` | device info | `31 14` + 20 zero bytes | CONFIRMED |

## Output: LEDs

The LED rings use the commands described for the
[base](base.md#output-leds) with module code `02` and one zone per knob,
`<zone>` = `00`..`02`.

```text
02 2d 02 00 74 3a df 00      zone 0 colour 74 3a df
02 2c 02 02 01 00            zone 2 static
02 2e 02 01 32               zone 1 brightness 50
```

Confidence: CONFIRMED on the wire and on the device (experiment 0009). That
zone index equals knob index is LIKELY; the operator saw the colours change
but the assignment was not noted.

SDK: `Module::set_led_mode`, `set_led_color`, `set_led_brightness`.

## Not investigated

Feature reports `0x2d` `0x2f` `0x30` `0x32` `0x34` `0x36`, other output
commands. UNKNOWN.
