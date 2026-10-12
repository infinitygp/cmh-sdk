# 2 Rollers (M032LE)

- USB `2516:0204`, product string `2xRoller, 4x1`, serial prefix `MHRR02AA41`
- Family: basic
- Firmware seen: `01.00.03.01`
- Controls: 2 detented rollers

## Input: rotation

| | |
|---|---|
| Report | ID `0x01`, tag `0x2a`, payload `01 02 r0 r1` |
| Trigger | one report per detent |
| Values | `00` / `01` one detent in either direction, `ff` roller did not move |
| Confidence | CONFIRMED for the layout (experiment 0002) |

```text
01 2a 04 00 01 02 ff 00      roller 1, one detent, direction code 00
01 2a 04 00 01 02 01 ff      roller 0, one detent, direction code 01
```

SDK: `InputEvent::EncoderDelta { index, delta }` with `-1` for `00` and `+1`
for `01`.

Open points (UNKNOWN):

- Which physical direction (towards / away from the user) produces `00`.
- Which physical roller is index 0.
- Whether the rollers can be pressed. No button report was seen in
  experiment 0002, and the report descriptor offers no hint.

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x2e` | firmware version | `2e 0f 28 35 3e 25 "01.00.03.01"` | CONFIRMED |
| `0x31` | device info | `31 14` + 20 zero bytes | CONFIRMED |

The rollers declare fewer feature reports than the other basic modules
(`0x2d` `0x2e` `0x2f` `0x30` `0x31` `0x36`), without `0x32` and `0x34`.
