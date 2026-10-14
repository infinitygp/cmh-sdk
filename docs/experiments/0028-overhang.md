# 0028: a module hanging over the edge of the base

- Date: 2026-10-14
- Host: native Linux
- Tools: `cmh list` and `cmh monitor`, which only read
- Layout, photographed by the owner: the 15 LCD Keys top-left, the
  5 Faders below them, the Dial Display to the right of the keys but half a
  place lower, the 3 Knobs below the dial; every module in its normal
  mounting. The modules together cover 6 x 5 cells, one row more than the
  base.

## Reasoning before the reading

With nothing turned, the modules hang over either the top or the bottom
edge. Over the bottom edge the faders would need a bottom pad in row 5,
which does not exist, so the base had to be under rows 2-5 of the photo and
the keys one row past its top edge. The pads under the modules' contacts
would then be B (4, 2) for the keys, B (6, 2) for the dial, B (4, 4) for
the faders and B (6, 4) for the knobs.

## Log

```text
15 LCD Keys  mount_path=1.1.1 placement: ?
5 Faders     mount_path=2.3   placement: columns 1-4 rows 3-4 turned 0
3 Knobs      mount_path=1.3.4 placement: columns 5-6 rows 3-4 turned 0
Dial Display mount_path=1.4.3 placement: columns 5-6 rows 1-2 turned 0
Base         OrientationChanged { upright: false }
```

All four paths are the pads named above. The base lay flat.

## Result

A module stays connected as long as its contact rests on a pad; the rest of
it may hang over the edge. The SDK rejected such placements as impossible
and reported none for the keys. It now returns them, with cells that
continue the base's grid past its edges: the keys cover columns 1-4,
rows 0-2.

After the change:

```text
15 LCD Keys  placement: columns 1-4 rows 0-2 turned 0
```

How far a module can hang over in practice, given the locating pegs and how
well it then holds, is UNKNOWN; only this layout was tried.
