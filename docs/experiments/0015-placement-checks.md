# 0015: placement checks with the remaining modules

- Date: 2026-10-12
- Tool: `cmh monitor`, which now prints the placement the SDK works out for
  every arriving module
- Purpose: the rule of [protocol/placement.md](../protocol/placement.md) had
  been measured in many places for the 3 Knobs and the 2 Rollers, but only
  once each for the Dial Display, the 5 Faders and the 15 LCD Keys

## Predictions, written before the placements

| Module | Placement | Expected mount path |
|---|---|---|
| Dial Display | columns 1-2, rows 1-2, turned 90 degrees | `1.2.3` |
| 5 Faders | columns 1-4, rows 1-2, normal | `1.1.1` |
| 15 LCD Keys | columns 3-6, rows 1-3, normal | `1.3.2` |

## Log

```text
arrived Dial Display mount_path=1.2.3 placement: columns 1-2 rows 1-2 turned 90
arrived 5 Faders     mount_path=1.1.1 placement: columns 1-4 rows 1-2 turned 0
arrived 15 LCD Keys  mount_path=1.3.2 placement: columns 3-6 rows 1-3 turned 0
```

Then the normal layout was restored:

```text
arrived 15 LCD Keys  mount_path=2.2   placement: columns 1-4 rows 1-3 turned 0
arrived Dial Display mount_path=1.4.3 placement: columns 5-6 rows 1-2 turned 0
arrived 3 Knobs      mount_path=1.3.4 placement: columns 5-6 rows 3-4 turned 0
arrived 2 Rollers    mount_path=1.1.4 placement: columns 1-4 rows 4-4 turned 0
```

## Conclusions

- All three predictions held, and the placements printed by the SDK match
  where the modules were put: the placement rule is CONFIRMED for every
  module type, each in at least two placements.
- The 15 LCD Keys are not tied to the left side of the base and do not need
  the 2 x 6 pin block, whose purpose stays UNKNOWN.
