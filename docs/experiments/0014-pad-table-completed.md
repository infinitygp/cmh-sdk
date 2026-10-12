# 0014: every pad measured

- Date: 2026-10-12
- Tool: `cmh monitor`; placements made by a person, with all other modules
  removed
- Follow-up to [0013](0013-contact-pads-and-placement-model.md); same grid
  and rotation conventions

## Placements

3 Knobs, bottom row:

| Place | Orientation | Mount path | Pad |
|---|---|---|---|
| columns 1-2, rows 3-4 | 0 | `4.4` | B (2, 4) |
| columns 1-2, rows 3-4 | 90 | `1.2.2` | L (1, 4) |
| columns 1-2, rows 3-4 | 270 | no contact | R (2, 3) is absent |
| columns 3-4, rows 3-4 | 0 | `2.3` | B (4, 4) |
| columns 3-4, rows 3-4 | 90 | `4.2` | L (3, 4) |
| columns 3-4, rows 3-4 | 270 | `2.1` | R (4, 3) |
| columns 5-6, rows 3-4 | 90 | `1.1.3` | L (5, 4) |
| columns 5-6, rows 3-4 | 270 | `1.3.1` | R (6, 3) |

3 Knobs, places between the regular ones:

| Place | Orientation | Mount path | Pad |
|---|---|---|---|
| columns 3-4, rows 2-3 | 0 | `2.2` | B (4, 3) |
| columns 3-4, rows 2-3 | 270 | `3.2` | R (4, 2) |
| columns 5-6, rows 2-3 | 0 | `1.3.2` | B (6, 3) |
| columns 5-6, rows 2-3 | 270 | `1.4.2` | R (6, 2) |
| columns 2-3, rows 3-4 | 0 | `4.3` | B (3, 4) |
| columns 2-3, rows 3-4 | 90 | `4.1` | L (2, 4) |
| columns 4-5, rows 3-4 | 0 | `1.1.2` | B (5, 4) |
| columns 4-5, rows 3-4 | 90 | `2.4` | L (4, 4) |

2 Rollers:

| Place | Orientation | Mount path | Pad |
|---|---|---|---|
| columns 1-4, row 4 | 0 | `1.1.4` | R (4, 4) |
| columns 3-6, row 4 | 0 | `1.3.3` | R (6, 4) |
| columns 1-4, row 1 | 0 | `3.3` | R (4, 1) |
| column 1, rows 1-4 | 90 (contact end down) | `1.2.1` | B (1, 4) |
| column 1, rows 1-4 | 270 (contact end up) | no contact | - |

Note on the first run of the "between" places: an unplanned extra placement
shifted the order, and the two entries for columns 5-6, rows 2-3 could not
be told apart from it. Both were repeated on their own; the table shows the
repeated values.

## Predictions of 0013

| Prediction | Result |
|---|---|
| knobs on B (4, 3) report the keys' path `2.2` | correct |
| knobs on L (5, 4) report the faders' path `1.1.3` | correct |
| knobs at columns 1-2, rows 3-4, 270 degrees make no contact | correct |
| knobs on B (4, 4) report the rollers' path `1.1.4` | wrong: `2.3` |

The failed one showed that the rollers do not have their contact where the
other modules have it. Three roller placements then located it at the right
side of the module's rightmost cell, and a fourth confirmed the consequence
that the rollers reach a bottom pad when stood upright.

Two more values were predicted from the remaining unassigned ports before
being measured: `1.3.2` for B (6, 3) and `1.2.1` for B (1, 4). Both correct.

## Conclusions

- All 27 pads have a measured mount path, and the 27 paths are exactly the
  ports of the base's hub tree other than the base controller: CONFIRMED.
- Hubs serve neighbouring pads (for example `4.x` covers columns 2-3 of
  row 4, `1.4.x` the top-right corner), which is what one would expect from
  the wiring and makes mix-ups in the table unlikely.
- Contact position: bottom of the bottom-right cell for the 3 Knobs (and,
  from single placements, the Dial Display, the 15 LCD Keys and the
  5 Faders); right end for the 2 Rollers: CONFIRMED / LIKELY as stated in
  [../protocol/placement.md](../protocol/placement.md).
- 2 x 2 modules can be mounted in places offset by one cell from the
  regular 3 x 2 tiling wherever a pad exists: CONFIRMED.
- The block of 2 x 6 pins has no port of its own: its purpose is UNKNOWN.

## SDK check

After the table was built into the SDK, `cmh list` reported for the modules
as they were left on the base:

```text
3 Knobs    mount_path=1.3.2  placement: columns 5-6 rows 2-3 turned 0
2 Rollers  mount_path=1.2.1  placement: columns 1-1 rows 1-4 turned 90
```

which is where they were.
