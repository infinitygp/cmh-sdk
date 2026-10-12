# 0023: LED rings of the 3 Knobs on natively installed Linux

- Date: 2026-10-12, same host as [experiment 0020](0020-native-linux.md),
  right after [experiment 0022](0022-faders-linux.md)
- Tool: `cmh` from this repository; a person at the device
- Closes the item "LED output on the 3 Knobs" left open in 0020

## Procedure

1. The owner took the 5 Faders off the base and put the 3 Knobs and the
   Dial Display back in their usual places, with the `cmh monitor` of 0022
   still running.
2. `cmh led --module knobs --index N --mode static --color ...` for the
   zones 0, 1, 2: red, green, blue. Brightness of zone 1 to 20.
3. All three zones white; brightness of zone 1 to 5, the others at 100.
4. All three zones back to the rainbow effect at brightness 100.

## Observations

- The swap, as logged by the monitor:

  ```text
  #   386.067 left    5 Faders
  #   392.371 arrived 3 Knobs      mount_path=1.3.4 placement: columns 5-6 rows 3-4 turned 0
  #   392.371 arrived Dial Display mount_path=1.4.3 placement: columns 5-6 rows 1-2 turned 0
  ```

- `cmh info`: firmware `01.00.05.00`, LED brightness `[100, 100, 100]`
  after the module arrived, `[100, 20, 100]` after step 2,
  `[100, 5, 100]` after step 3 and `[100, 100, 100]` after step 4.
- After step 2 the owner saw the left ring red, the right one green and
  the single knob's ring blue. Asked whether the green ring was clearly
  dimmer than the other two, the owner first said yes and then "rather
  not".
- After step 3 the owner saw the right ring clearly dimmer than the other
  two.

## Conclusions

- LED effect, colour and brightness on the 3 Knobs through hidraw, with
  zone 0 on the left knob, 1 on the right knob and 2 on the single knob:
  CONFIRMED on natively installed Linux.
- LED brightness read back follows what is set: CONFIRMED on Linux.
- A brightness of 5 next to 100 is plain to see in the same colour. A
  green ring at 20 next to a red and a blue one at 100 was not judged
  reliably; rings of different colours are a poor comparison.
- One module type taken off and two others put on a powered base are
  followed with their mount paths and placements: CONFIRMED on Linux.
