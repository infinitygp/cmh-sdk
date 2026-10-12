# 0013: contact pads of the base and a placement model

- Date: 2026-10-12
- Inputs: a photo of the empty base taken from above, and a sweep of the
  3 Knobs through the three top 2x2 places in every orientation that makes
  contact, logged by `cmh monitor`
- Grid convention as before: USB cable at the top-left, columns 1..6 from
  the left, rows 1..4 from the top; orientation is given as clockwise
  rotation from the normal mounting (for the knobs: single knob at the top)

## Sweep of the top row

| Place | 0 deg | 90 deg | 180 deg | 270 deg |
|---|---|---|---|---|
| columns 1-2, rows 1-2 | `1.2.4` | `1.2.3` | no contact | no contact |
| columns 3-4, rows 1-2 | `1.1.1` | `3.1` | no contact | `3.3` |
| columns 5-6, rows 1-2 | `1.4.3` | `3.4` | no contact | `1.4.1` |

"No contact" was reported by the operator and matches the log (two, three
and three arrivals). The knobs at columns 5-6, rows 1-2 give the path the
Dial Display has there, so both 2x2 modules carry their contact in the same
spot.

## Pads on the photo

The base has 30 pads of 2 x 2 spring pins. Positions were read off the photo
and converted to cells; each pad sits at one of three spots inside a cell:

- **B**: bottom centre of the cell (about 0.55, 0.80)
- **L**: left centre (about 0.17, 0.48)
- **R**: right centre (about 0.90, 0.47)

| Row | B pads in columns | L pads in columns | R pads in columns |
|---|---|---|---|
| 1 | - | - | 4, 6 |
| 2 | 2, 4, 6 | 1, 3, 5 | 4, 6 |
| 3 | 4, 6 | - | 4, 6 |
| 4 | 1, 2, 3, 4, 5, 6 | 1, 2, 3, 4, 5 | 4, 6 |

That is 11 + 8 + 8 = 27 pads. The remaining three form one block of 2 x 6
pins in column 2, row 1, near its bottom-right corner.

The base also has twelve locating pegs at the centres of every second cell
in a checkerboard pattern, and the marking "VERTICAL / HORIZONTAL" in the
bottom-left corner.

The hub tree offers 27 ports besides the base controller, three fewer than
there are pads; which pads share or lack a port is UNKNOWN.

## Model

A module carries its contact at the B spot of its own bottom-right cell.
Turning the module moves that spot:

| Orientation | Contact ends up at | Module of w x h cells occupies |
|---|---|---|
| 0 deg | B pad of cell (c, r) | columns c-w+1..c, rows r-h+1..r |
| 90 deg | L pad of cell (c, r) | columns c..c+h-1, rows r-w+1..r |
| 270 deg | R pad of cell (c, r) | columns c-h+1..c, rows r..r+w-1 |
| 180 deg | top centre of a cell | never: the base has no such pad |

Check against every placement measured so far:

| Module | Placement | Pad by the model | Mount path |
|---|---|---|---|
| 3 Knobs | columns 1-2, rows 1-2, 0 deg | B (2, 2) | `1.2.4` |
| 3 Knobs | columns 1-2, rows 1-2, 90 deg | L (1, 2) | `1.2.3` |
| 3 Knobs | columns 1-2, rows 1-2, 270 deg | R (2, 1): absent | no contact |
| 3 Knobs | columns 3-4, rows 1-2, 0 deg | B (4, 2) | `1.1.1` |
| 3 Knobs | columns 3-4, rows 1-2, 90 deg | L (3, 2) | `3.1` |
| 3 Knobs | columns 3-4, rows 1-2, 270 deg | R (4, 1) | `3.3` |
| 3 Knobs, Dial Display | columns 5-6, rows 1-2, 0 deg | B (6, 2) | `1.4.3` |
| 3 Knobs | columns 5-6, rows 1-2, 90 deg | L (5, 2) | `3.4` |
| 3 Knobs | columns 5-6, rows 1-2, 270 deg | R (6, 1) | `1.4.1` |
| 3 Knobs | columns 5-6, rows 3-4, 0 deg | B (6, 4) | `1.3.4` |
| 5 Faders (4x2) | columns 5-6, rows 1-4, 90 deg | L (5, 4) | `1.1.3` |
| 2 Rollers (4x1) | columns 1-4, row 4, 0 deg | B (4, 4) | `1.1.4` |
| 15 LCD Keys (4x3) | columns 1-4, rows 1-3, 0 deg | B (4, 3) | `2.2` |

The model reproduces all thirteen, including the three orientations that
make no contact. The last three rows assign a pad to a path only through
the model: no 2x2 module has been put on those pads yet.

Status at the time of writing: LIKELY. It explains everything observed with
one rule, but the pad positions come from a photo and three assignments are
untested. [Experiment 0014](0014-pad-table-completed.md) tested them: two
held, and the rollers turned out to carry their contact elsewhere.

## Predictions to test

Written down before measuring:

| 3 Knobs placement | Pad | Expected |
|---|---|---|
| columns 3-4, rows 3-4, 0 deg | B (4, 4) | `1.1.4` (the rollers' path) |
| columns 5-6, rows 3-4, 90 deg | L (5, 4) | `1.1.3` (the faders' path) |
| columns 3-4, rows 2-3, 0 deg | B (4, 3) | `2.2` (the keys' path) |
| columns 1-2, rows 3-4, 270 deg | R (2, 3): absent | no contact |

The model also says a 2x2 module can sit half a place lower (rows 2-3) in
the middle and right columns, and half a place sideways (columns 2-3 and
4-5) in the bottom row, provided the locating pegs allow it.

## Pads still without a path

B (6, 3), B (1, 4), B (2, 4), B (3, 4), B (5, 4); L (1, 4), L (2, 4),
L (3, 4), L (4, 4); R (4, 2), R (6, 2), R (4, 3), R (6, 3), R (4, 4),
R (6, 4); the block of three.
