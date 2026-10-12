# 0011: mount paths of one module in several places

- Date: 2026-10-12
- Tool: `cmh monitor` (prints the mount path of every arriving module)
- Grid convention: the base seen with the USB cable at the top-left is 6
  columns wide and 4 rows high; columns count from the left, rows from the
  top. A 2x2 module fits three times across the width.

## Procedure (performed by a person)

1. Remove the 15 LCD Keys (columns 1-4, rows 1-3) and photograph the base.
2. Mount the 3 Knobs in the top-left corner.
3. Move them one module width to the right.
4. Mount them in the top-left corner again, turned 90 degrees clockwise.
5. Restore the original layout.

## Observations

| 3 Knobs placement | Orientation | Mount path |
|---|---|---|
| columns 1-2, rows 1-2 | as in the normal layout | `1.2.4` |
| columns 3-4, rows 1-2 | as in the normal layout | `1.1.1` |
| columns 1-2, rows 1-2 | turned 90 degrees clockwise | `1.2.3` |
| columns 5-6, rows 3-4 (normal layout) | as in the normal layout | `1.3.4` |

All known placements so far, including experiments 0005 and 0010:

| Module | Cells | Orientation | Mount path |
|---|---|---|---|
| Base controller | - | - | `1.4.4` |
| 15 LCD Keys | columns 1-4, rows 1-3 | logo bar at the bottom | `2.2` |
| 2 Rollers | columns 1-4, row 4 | along the bottom edge | `1.1.4` |
| Dial Display | columns 5-6, rows 1-2 | upright | `1.4.3` |
| 3 Knobs | columns 5-6, rows 3-4 | single knob at the top | `1.3.4` |
| 3 Knobs | columns 1-2, rows 1-2 | single knob at the top | `1.2.4` |
| 3 Knobs | columns 3-4, rows 1-2 | single knob at the top | `1.1.1` |
| 3 Knobs | columns 1-2, rows 1-2 | turned 90 degrees clockwise | `1.2.3` |
| 5 Faders | columns 5-6, rows 1-4 | turned 90 degrees | `1.1.3` |

The photo of the uncovered area (columns 1-4, rows 1-3) shows spring
contact pads, each a 2 x 2 group of pins, plus one block of 2 x 6 pins.
Approximate pad centres in cell units from the top-left corner of the base
(read off a photo, so only good to a quarter of a cell):

| Pad | x | y |
|---|---|---|
| block of three (2 x 6 pins) | 1.5 .. 2.0 | 0.8 |
| a | 3.7 | 0.4 |
| b | 0.1 | 1.4 |
| c | 2.1 | 1.4 |
| d | 3.7 | 1.4 |
| e | 1.4 | 1.7 |
| f | 3.4 | 1.7 |
| g | 3.7 | 2.4 |
| h | 3.4 | 2.7 |

Re-seating the keys produced a contact bounce (arrived, lost, arrived
within three seconds); `cmh monitor` followed it and ended with the module
open.

## Conclusions

- Turning a module in place changes its mount path: the path encodes
  position and orientation together. CONFIRMED.
- Pads are not laid out one per grid cell. Pads b / e and c / f sit at the
  same offsets inside two neighbouring 2 x 2 areas, which fits a 2 x 2
  module having its contact off-centre and reaching a different pad in each
  orientation. LIKELY.
- A lookup from (module type, mount path) to (cells, orientation) is
  therefore possible, but it is a table of every allowed placement, not a
  formula over the path. Nine entries are known.

## How to finish the table

Cheapest route found so far: photograph the whole empty base and the
underside of each module once, to get pad and contact positions, then label
the pads with a handful of placements instead of trying every combination
by hand.
