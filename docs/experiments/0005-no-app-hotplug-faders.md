# 0005: display check, input without the vendor application, hot-plug, faders

- Date: 2026-10-12
- Tools: `cmh monitor --raw` running throughout (hidapi only), then one
  USBPcap capture of the vendor application starting
- Capture (local only): `20261012-122713-app-start-with-faders.pcapng` (2070 packets)
- Evidence:
  [fixtures/inputs/without-vendor-app.txt](../../fixtures/inputs/without-vendor-app.txt),
  [fixtures/inputs/faders.txt](../../fixtures/inputs/faders.txt),
  [fixtures/startup/vendor-app-init-faders.txt](../../fixtures/startup/vendor-app-init-faders.txt)
- State at the start: vendor application killed, test images from
  experiment 0004 on the displays

## Part 1: what the displays show (photo taken by a person)

- The 15 keys show the numbers 0..14 upright, row by row from the top-left,
  each with its marker in the top-left corner.
- The dial shows the test picture upright (arrow at the top, `L` left, `R`
  right), cropped to a circle.
- The key lenses hide a few pixels at the image borders (the right edge of
  "10" is cut off, as are the vendor's own icons).

Conclusions:

- `Module::set_image` works on both display modules: CONFIRMED.
- Images are neither rotated nor mirrored, key index order for images equals
  the input order: CONFIRMED.
- Offset 6 of the dial image report may be `00`: CONFIRMED.
- Images stay on the displays after the sending process exits: CONFIRMED.

## Part 2: input without the vendor application

One action per module while only `cmh monitor` was attached:

| Time (s) | Module | Event |
|---|---|---|
| 297.9 | Keys | key 6 down, up |
| 309.0 | Knobs | knob 1, +1 |
| 316.9 | Rollers | roller 0, +1 |
| 320.7 | Dial | -1 |
| 323.5 | Dial | button down, up |

Conclusion: every module reports input with no vendor software running:
CONFIRMED. The keys and the dial had been put into software mode by the SDK
in experiment 0004; whether they also report input outside software mode is
still UNKNOWN.

## Part 3: hot-plug

The dial and the knobs were pulled off the base and the 5 Faders mounted in
their place while `cmh monitor` kept running:

```text
#   330.955 left    Dial Display 2516:0206
#   332.959 left    3 Knobs      2516:0202
#   338.099 arrived 5 Faders     2516:0200
```

Conclusions:

- Modules can be exchanged while the base is powered; the remaining modules
  keep working: CONFIRMED.
- Polling the HID device list once per second detects removal and arrival:
  CONFIRMED on Windows.
- The faders got the USB address the knobs had before, so USB addresses are
  not an identity: CONFIRMED.

## Part 4: faders

All five faders were moved over their full travel, one at a time.

- 406 reports, all `01 2a 07 00 02 05 v0..v4`; every value from 0 to 100
  occurred.
- Exactly one position changed per report; nothing was sent before the
  first movement.
- Fastest spacing 16 ms, median 41 ms.
- At rest, fader 0 kept toggling between 44 and 45.

Vendor application start with the faders mounted (capture):

| Time | Request | Data |
|---|---|---|
| 4.541 | get `0x31` | `31 14` + zeros |
| 4.544 | get `0x33` | `33 05 2c 2d 39 3a 38` |
| 5.148 | get `0x2e` | firmware `01.00.05.00` |
| 5.956, 7.988, then every ~2.03 s | get `0x33` | positions again |

No output report and no feature write went to the faders.

Afterwards `cmh info` read the same positions through the SDK
(`[44, 45, 57, 58, 56]`).

Conclusions:

- Fader input report and position feature report: CONFIRMED.
- The vendor application polls positions in addition to listening for
  input reports: CONFIRMED.

## Part 5: port paths

Hub port paths below the base's first hub, with the module positions on the
6 x 4 grid as seen with the cable at the top-left (columns 1..6 from the
left, rows 1..4 from the top):

| Layout | Module | Cells | Port path |
|---|---|---|---|
| A | 15 LCD Keys | columns 1-4, rows 1-3 | 2.2 |
| A | Dial Display | columns 5-6, rows 1-2 | 1.4.3 |
| A | 3 Knobs | columns 5-6, rows 3-4 | 1.3.4 |
| A | 2 Rollers | columns 1-4, row 4 | 1.1.4 |
| A, B | Base controller | - | 1.4.4 |
| B | 15 LCD Keys | unchanged | 2.2 |
| B | 2 Rollers | unchanged | 1.1.4 |
| B | 5 Faders (rotated) | columns 5-6, rows 1-4 | 1.1.3 |

Modules that did not move kept their path; the faders, covering the area of
two former modules, appeared on a third path. This is consistent with one
fixed port per contact position on the base, but three data points cannot
establish the mapping. UNKNOWN.

## Open questions

- Fader index order and direction relative to the module's own markings.
- Mapping between port path and grid position; position of the contact on
  each module.
