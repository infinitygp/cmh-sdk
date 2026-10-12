# Dial Display (1xDial w/ IPS)

- USB `2516:0206`, product string `1xDial w/ IPS, 2x2`, serial prefix `MHDD01AA22`
- Family: dial (320 byte output report, feature IDs `0x47`..`0x4e`)
- Firmware seen: `01.00.04.00`
- Controls: one detented dial with push, round 240 x 240 display

## Input: rotation

| | |
|---|---|
| Report | ID `0x01`, tag `0x44`, payload `03 01 d` |
| Trigger | one report per detent |
| Values | `00` one detent clockwise, `01` one detent counter-clockwise |
| Confidence | CONFIRMED (experiments 0002, 0009) |

```text
01 44 03 00 03 01 00      clockwise
01 44 03 00 03 01 01      counter-clockwise
```

Note the reversal against the 3 Knobs, where `01` is clockwise.

SDK: `InputEvent::EncoderDelta { index: 0, delta }` with `+1` for clockwise,
the same sign convention as on the knobs.

## Input: push

| | |
|---|---|
| Report | ID `0x01`, tag `0x44`, payload `04 01 p` |
| Values | `01` pressed, `00` released |
| Confidence | CONFIRMED (experiment 0002) |

```text
01 44 03 00 04 01 01      pressed
01 44 03 00 04 01 00      released
```

Touching or swiping the glass produces no report (experiment 0016): the
display is not a touch input.

SDK: `InputEvent::ButtonDown { index: 0 }` / `ButtonUp { index: 0 }`.

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x48` | firmware version | `48 0f 26 a0 d8 0a "01.00.04.00"` | CONFIRMED |
| `0x4b` | device info | `4b 0b 14 00 00 00 f0 00 f0 00 01 f0 f0` | CONFIRMED (bytes), LIKELY (meaning) |

The device info payload has the layout described in
[keys15.md](keys15.md#feature-reads): no key grid, panel 240 x 240, image
240 x 240.

## Feature write: software mode

| | |
|---|---|
| Report | `03 49 01`, 32 bytes, `SET_REPORT` feature |
| Reply | none |
| Confidence | CONFIRMED as sent by the vendor application; effect LIKELY |

Sent by the vendor application right after opening the module and again
before it starts drawing. `03 49 00` has never been observed, so the SDK
refuses to send it. After power-up the dial plays a built-in animation
until software mode is enabled (experiment 0017).

## Feature write: display brightness

| | |
|---|---|
| Report | `03 44 <level>`, 32 bytes, `SET_REPORT` feature |
| Values | percent; the vendor application was seen sending 40..55 in steps of 5 |
| Reply | none |
| Confidence | CONFIRMED on the wire (experiment 0009) and on the device: level 0 turns the display completely dark (experiment 0017). Not remembered across a power cycle. |

```text
03 44 32      brightness 50
```

In the vendor application brightness is not a setting but an action bound
to a control; one detent changes both display modules by 5.

SDK: `Module::set_display_brightness`.

## Output: display image

Same chunk format as the [key image](keys15.md#output-key-image) with
command `0x45`.

```text
02 45 00 00 38 01 30 00 ff d8 ff e0 ...     chunk 0, 312 bytes
02 45 2a 00 a5 00 30 01 ...                 chunk 42, 165 bytes, last
```

- Offset 6 is `30` in almost every report of the vendor application and
  `f0` in one upload, matching stale buffer contents. The SDK sends `00`,
  which displays correctly: the byte is not a required constant. CONFIRMED
  (0005).
- Image format: baseline JPEG, 240 x 240, 4:4:4, not rotated. Sizes seen
  working: 2.2 kB to 13.4 kB.
- The vendor application redraws the whole display once per second (clock
  face), about 5 ms per frame on the wire.

Confidence: CONFIRMED on the wire (experiments 0001, 0003, 0004) and on
the display by photo (0005): upright, cropped to the round panel.

SDK: `Module::set_image(0, jpeg)`.

## Not investigated

Feature reports `0x47` `0x49` `0x4a` `0x4c` `0x4d` `0x4e`, animations.
UNKNOWN.
