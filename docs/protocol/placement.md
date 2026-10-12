# Position of a module on the base

No HID report carries position. A module's place follows from the USB port
it is connected through, and the base wires every contact pad to a fixed
port of its internal hub tree.

Confidence: CONFIRMED for the pad table (every pad was measured, experiments
0011, 0013, 0014). The geometric rule that turns a pad into covered cells is
CONFIRMED for every module type: in many places for the 3 Knobs and the
2 Rollers, and in two places each, one of them predicted beforehand, for the
Dial Display, the 15 LCD Keys and the 5 Faders (experiment 0015).

## Mount path

The base contains nine 4-port hubs: one root hub `A`, four hubs on the ports
of `A.1`, and `A.2`, `A.3`, `A.4`. The *mount path* of a module is its chain
of hub ports below `A`, for example `1.3.4` or `2.2`. The base controller
itself is at `1.4.4`, which the SDK uses to find where the base's hub tree
starts in the port chain reported by the operating system.

SDK: `HidInterface::port_chain`, `Manager::mount_path`.

## Grid and pads

Seen with the USB cable at the top-left, the base is 6 columns by 4 rows of
cells. Each of the 27 pads sits at the bottom (B), left (L) or right (R) side
of one cell:

| Row | B pads (column: path) | L pads | R pads |
|---|---|---|---|
| 1 | - | - | 4: `3.3`, 6: `1.4.1` |
| 2 | 2: `1.2.4`, 4: `1.1.1`, 6: `1.4.3` | 1: `1.2.3`, 3: `3.1`, 5: `3.4` | 4: `3.2`, 6: `1.4.2` |
| 3 | 4: `2.2`, 6: `1.3.2` | - | 4: `2.1`, 6: `1.3.1` |
| 4 | 1: `1.2.1`, 2: `4.4`, 3: `4.3`, 4: `2.3`, 5: `1.1.2`, 6: `1.3.4` | 1: `1.2.2`, 2: `4.1`, 3: `4.2`, 4: `2.4`, 5: `1.1.3` | 4: `1.1.4`, 6: `1.3.3` |

A block of 2 x 6 pins in column 2, row 1 has no port of its own; its purpose
is UNKNOWN.

## From pad to cells

Every module touches one pad with a contact at a fixed spot of its
underside:

| Module | Footprint (normal mounting) | Contact |
|---|---|---|
| 15 LCD Keys | 4 x 3 | bottom side of its bottom-right cell |
| 5 Faders | 4 x 2 | bottom side of its bottom-right cell |
| 3 Knobs, Dial Display | 2 x 2 | bottom side of its bottom-right cell |
| 2 Rollers | 4 x 1 | right side of its rightmost cell |

Turning a module clockwise moves the contact from a bottom side to a left
side (90 degrees), a top side (180 degrees) and a right side (270 degrees).
The base has no pad on a top side, so the modules with a bottom contact
cannot be mounted upside down, while the rollers reach a bottom pad when
turned 90 degrees.

Given the pad and the module type there is exactly one orientation whose
contact side matches, and the pad's cell fixes the rest. Examples:

| Module | Mount path | Covers | Turned |
|---|---|---|---|
| 15 LCD Keys | `2.2` | columns 1-4, rows 1-3 | 0 |
| 3 Knobs | `2.2` | columns 3-4, rows 2-3 | 0 |
| 5 Faders | `1.1.3` | columns 5-6, rows 1-4 | 90 |
| 3 Knobs | `1.1.3` | columns 5-6, rows 3-4 | 90 |
| 2 Rollers | `1.1.4` | columns 1-4, row 4 | 0 |
| 2 Rollers | `1.2.1` | column 1, rows 1-4 | 90 |

2 x 2 modules are not limited to the six places of a 3 x 2 tiling: they
also seat half a place lower in the middle and right columns and half a
place sideways in the bottom row.

SDK: `placement::placement(kind, mount_path)`, `Manager::placement`.

## Limits

- One base unit was examined. That other units are wired identically is an
  assumption.
- The base can also be used standing on its short side ("VERTICAL" marking);
  the cells are then the same, only the view is turned.
- Mount paths need the real USB topology. Windows provides it and so does
  Linux through sysfs, with the same values (experiment 0020); USB/IP (WSL)
  hides it.
