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

The mapping of indexes 0..2 to the three physical knobs has not been pinned
down yet (the knobs form a triangle). UNKNOWN, open question in experiment
0002.

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x2e` | firmware version | `2e 0f a7 df 5b 3f "01.00.05.00"` | CONFIRMED |
| `0x31` | device info | `31 14` + 20 zero bytes | CONFIRMED |

## Not investigated

Output reports and the LED rings, feature reports `0x2d` `0x2f` `0x30`
`0x32` `0x34` `0x36`. UNKNOWN.
