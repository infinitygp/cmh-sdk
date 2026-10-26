# 0002: every input control once

- Date: 2026-10-12
- Tool: passive hidapi read loop on all five modules (now `cmh monitor --raw`), nothing written
- State: vendor application running
- Evidence: [crates/cmh-sdk/fixtures/inputs/all-controls.txt](../../crates/cmh-sdk/fixtures/inputs/all-controls.txt)
  (complete session; base heartbeats thinned out)
- Layout during the test: keys top-left with the logo bar at the bottom, dial
  top-right, knobs bottom-right, rollers bottom-left

## Procedure (performed by a person)

1. Knobs, one after another: 3 detents counter-clockwise, 3 clockwise, then
   press and release.
2. Rollers, one after another: a few detents one way, then the other way.
3. Keys: each key once, row by row from the top-left; finally the first two
   keys held together.
4. Dial: a few detents each way, one push, then touching the display.

## Observations

| Time (s) | Module | Reports | Reading |
|---|---|---|---|
| 119..124 | Knobs | `00 03 00 ff ff` x3, `00 03 01 ff ff` x3 | knob 0: `00` = counter-clockwise, `01` = clockwise |
| 131..138 | Knobs | `03 03 01 00 00` / `03 03 00 00 00` x3 | knob 0 pressed / released |
| 142..153 | Knobs | same pattern in the third byte | second knob operated is index 2 |
| 157..164 | Knobs | same pattern in the second byte | third knob operated is index 1 |
| 173..175 | Rollers | `01 02 ff 00` x4, `01 02 ff 01` x2 | first roller operated is index 1 |
| 177..180 | Rollers | `01 02 00 ff` x4, `01 02 01 ff` x4 | second roller operated is index 0 |
| 187..193 | Keys | `02 0f` + one `01` moving from position 0 to 14, each followed by all zero | key index = row-major position |
| 195..198 | Keys | `.. 01`, `.. 01 01`, all zero | full state per report; simultaneous keys work |
| 204..208 | Dial | `03 01 00` x2, `03 01 01` x3 | rotation, one byte |
| 214 | Dial | `04 01 01`, `04 01 00` | push and release |
| 222 | Dial | three quick `04 01 01` / `04 01 00` pairs | either three pushes or a reaction to touching the display |

No roller produced a button report.

## Conclusions

- Channel-0 payload is `<type> <count> <one byte per control>`: CONFIRMED on
  four modules.
- Type codes are module specific (`03` is "buttons" on the knobs and
  "rotation" on the dial): CONFIRMED.
- Knob direction codes: CONFIRMED (the operator turned counter-clockwise
  first).
- Key index order: CONFIRMED.

## Open questions

- Physical position of knob indexes 0, 1, 2.
- Physical roller for index 0 and the direction that yields `00`.
- Dial direction for `00`.
- Whether the burst at 222 s came from touching the display. (Answered in
  experiment 0016: it did not; the display reports no touch.)
