# 0022: the 5 Faders on Linux, at a pad not used before

- Date: 2026-10-12, same host as [experiment 0020](0020-native-linux.md)
- Modules: Base, 15 LCD Keys, 2 Rollers, and the 5 Faders where the Dial
  Display and the 3 Knobs had been
- Tool: `cmh` from this repository; a person at the device

## Procedure

1. `cmh list --descriptors`, `cmh info --module faders`.
2. `cmh led --module faders --index N --mode static --color ...` for the
   zones 0 to 4: red, green, blue, yellow, white.
3. LED brightness of zone 2 to 30 and of zone 4 to 60, `cmh info`, both
   back to 100, `cmh info`.
4. With `cmh monitor --raw` running, the owner moved the red fader to one
   end and back, then the white one.
5. With the same monitor still running, the owner lifted the module off
   the base and put it back.

## Observations

- Enumeration: `5xFader, 4x2`, `2516:0200`, the report descriptor has 211
  bytes and declares the feature IDs listed in
  [protocol/faders5.md](../protocol/faders5.md).
- `usb_port_chain=1.4.3.1.4.1 mount_path=1.4.1
  placement: columns 5-6 rows 1-4 turned 270`.
- The owner: the module lies in the two right columns over the whole
  height of the base, with the 100 end of the faders pointing left. From
  the top of the base down the strips were white, yellow, blue, green,
  red.
- `cmh info`: firmware `01.00.05.00` (build tag `54 e2 61 51`), device
  info `14` + zeros, LED brightness `[100, 100, 100, 100, 100]`, fader
  positions `[100, 100, 41, 55, 56]`. `cmh monitor` started with the same
  five positions as events.
- LED brightness read back `[100, 100, 30, 100, 60]` after step 3 and
  `[100, 100, 100, 100, 100]` after setting it back.
- The owner started both movements towards the right. The monitor showed
  index 0 going from 100 down to 0 and back to 100, then index 4 from 56
  down to 0 and up to 100; 177 reports, most of them 18 to 21 ms apart:

  ```text
  01 2a 07 00 02 05 63 64 29 37 38      AnalogChanged { index: 0, value: 99 }
  01 2a 07 00 02 05 61 64 29 37 38      AnalogChanged { index: 0, value: 97 }
  01 2a 07 00 02 05 64 64 29 37 37      AnalogChanged { index: 4, value: 55 }
  ```

- Lifting and reseating, as logged by the monitor:

  ```text
  #   282.122 left    5 Faders
  #   293.277 arrived 5 Faders     mount_path=1.4.1 placement: columns 5-6 rows 1-4 turned 270
  #   293.277 5 Faders     AnalogChanged { index: 0, value: 100 }
  #   293.277 5 Faders     AnalogChanged { index: 1, value: 100 }
  #   293.277 5 Faders     AnalogChanged { index: 2, value: 41 }
  #   293.277 5 Faders     AnalogChanged { index: 3, value: 55 }
  #   293.277 5 Faders     AnalogChanged { index: 4, value: 100 }
  ```

  `cmh info` afterwards gave the same positions and LED brightness
  `[100, 100, 100, 100, 100]`.

## Conclusions

- Enumeration, firmware and position reads, input, LED colours and
  hot-plug of the 5 Faders through hidraw: CONFIRMED on Linux. A module
  that returns announces its fader positions again.
- LED brightness per zone can be read back from `0x32` on the 5 Faders as
  on the base and the knobs: CONFIRMED.
- LED zone index equals fader index: CONFIRMED again (the red fader is
  index 0, the white one index 4).
- Mount path `1.4.1`, the right-side pad of column 6 in row 1, with the
  module turned 270 degrees: the placement the SDK derives matches what
  the owner sees, including the direction (unturned, index 0 is on the
  left and 100 at the top; here index 0 is at the bottom and 100 on the
  left). CONFIRMED. It is the third place checked for the 5 Faders and
  the first with a module of that contact type on a right-side pad.
