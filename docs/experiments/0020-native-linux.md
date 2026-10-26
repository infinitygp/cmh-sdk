# 0020: the checklist on natively installed Linux

- Date: 2026-10-12
- Host: CachyOS (Arch based) installed natively, kernel 7.2.9-2-cachyos,
  `systemd-detect-virt` prints `none`; rustc 1.99.0, libudev 262, hidapi
  crate 2.6.7 with its hidraw backend
- Modules, in the usual layout: Base, 15 LCD Keys, 3 Knobs, 2 Rollers, Dial
  Display. The 5 Faders were not mounted.
- USB: two hubs sit between the PC's root port and the base (port chain
  `1.4`, then `3` for the base's own hub). The 15 LCD Keys and the Dial
  Display are high-speed devices with a 64 byte control endpoint; Base,
  Knobs and Rollers are full-speed with 8 bytes.
- Tool: only `cmh` from this repository (release build), run as an ordinary
  user; a person at the device. Times are local.
- Other software: OpenDeck (opendeck-bin 2.14.0) was running for another
  device; `fuser` listed no process on the modules' hidraw nodes.
- Evidence: [crates/cmh-sdk/fixtures/inputs/linux-native.txt](../../crates/cmh-sdk/fixtures/inputs/linux-native.txt)

## Procedure

1. Without a udev rule for the modules: `cargo build --release`, `cmh list`,
   `cmh info`.
2. The owner installed the rule from [../linux-udev.md](../linux-udev.md)
   (group `wheel`, mode 0660) and reloaded udev. `cmh list --descriptors`,
   `cmh info`.
3. With `cmh monitor --raw` running: the owner unplugged the base's USB
   cable for about five seconds and plugged it in again. `cmh info`.
4. `cmh software-mode on` for the keys and the dial; the owner operated one
   control per module.
5. `cmh brightness`, `cmh image` for key 0 and for the dial (a 240 x 240
   PNG labelled "LINUX 0020", converted by the tool),
   `cmh led --module base --mode static --color 00ff00`.
6. The owner lifted the Dial Display off the base and put it back; `cmh
   info` for the dial and the keys was then repeated every few seconds.

Steps 3, 4 and 6 grew beyond the checklist because of what step 2 showed.
[Experiment 0021](0021-standalone-mode-reads.md) found the cause: step 2
itself. The cause of the state described below, and what ends it, are
recorded there.

## Observations

### Topology, access, enumeration

- `cmh list` worked before the rule existed and gave the mount paths and
  placements known from Windows:

  | Module | `usb_port_chain` | `mount_path` | `placement` |
  |---|---|---|---|
  | Base | `1.4.3.1.4.4` | `1.4.4` | - |
  | 15 LCD Keys | `1.4.3.2.2` | `2.2` | columns 1-4 rows 1-3 turned 0 |
  | 3 Knobs | `1.4.3.1.3.4` | `1.3.4` | columns 5-6 rows 3-4 turned 0 |
  | 2 Rollers | `1.4.3.1.1.4` | `1.1.4` | columns 1-4 rows 4-4 turned 0 |
  | Dial Display | `1.4.3.1.4.3` | `1.4.3` | columns 5-6 rows 1-2 turned 0 |

- Without the rule the nodes were `crw------- root root` and `cmh info`
  printed, for every module,
  `<cannot open: permission denied opening /dev/hidraw17; on Linux install the udev rule from docs/linux-udev.md>`.
- With the rule the five nodes became `crw-rw---- root wheel`.
- Report descriptor lengths as stored in the modules: Base 211 bytes, Keys
  194, Knobs 194, Rollers 160, Dial 194. The feature IDs declared are the
  ones listed in experiment 0018.
- After the power cycle the keys were `/dev/hidraw14` and the rollers
  `/dev/hidraw9`, the reverse of before.

### Basic modules

- `cmh info`: Base `02.00.04.00`, Knobs `01.00.05.00`, Rollers
  `01.00.03.01`, device info `14` + zeros on all three, every time.
- LED brightness read `[50]` (Base) and `[50, 50, 50]` (Knobs) as found,
  `[100]` and `[100, 100, 100]` after the power cycle.
- After `cmh led --module base --mode static --color 00ff00` the owner
  reported the base lit steady green.

### Display modules do not answer after power-up

As found (the kernel log has the modules enumerating at 15:31, before the
session), on the keys and, with its own device info, on the dial:

```text
firmware: <malformed report: feature report too short: 0 bytes>
device info: 14 03 05 0f e0 01 10 01 01 48 48
display brightness: <malformed report: feature report too short: 0 bytes>
```

The two reads came back empty in every one of 17 `cmh info` runs between
15:47 and 15:53 (10 on the keys, 7 on the dial), also while `cmh monitor`
held the modules open; the device info read worked each time.
`cmh software-mode` (keys: on, off, on; dial: on) returned without error
and changed nothing in the reads. The owner reported the keys showing their
built-in animation throughout.

Base power cycle, as logged by `cmh monitor` (serial numbers left out):

```text
#    74.299 left    2 Rollers
#    74.399 left    3 Knobs
#    74.463 left    Dial Display
#    74.496 left    Base
#    74.590 left    15 LCD Keys
#    83.083 arrived 15 LCD Keys  mount_path=? placement: ?
#    83.083 arrived 3 Knobs      mount_path=? placement: ?
#    83.083 arrived 2 Rollers    mount_path=? placement: ?
cannot open Dial Display: permission denied opening /dev/hidraw16; ... (will retry)
#    84.086 arrived Base         mount_path=1.4.4 placement: ?
#    84.086 placed  15 LCD Keys  mount_path=2.2 placement: columns 1-4 rows 1-3 turned 0
#    84.086 placed  3 Knobs      mount_path=1.3.4 placement: columns 5-6 rows 3-4 turned 0
#    84.086 placed  2 Rollers    mount_path=1.1.4 placement: columns 1-4 rows 4-4 turned 0
#    84.086 arrived Dial Display mount_path=1.4.3 placement: columns 5-6 rows 1-2 turned 0
```

Timeline after the modules came back at 15:54:27:

| Time | Action | Result |
|---|---|---|
| 15:54:3x | `cmh info` | keys and dial: both reads empty, as before |
| 15:54:55 | `cmh software-mode on`, keys and dial | no error; the owner reported both still showing the animation |
| 15:56:25 | owner: a knob turned (by mistake), then dial one detent, dial pressed, top-left key pressed | two reports from the knobs; nothing from the dial or the keys |
| 15:57:31 | `cmh brightness --module keys 5` | no error |
| 15:57:31 | `cmh image --module dial` | `sent 14847 bytes to image 0` |
| 15:57:31 | `cmh info`, keys, then dial | firmware `02.00.01.00` and `01.00.04.00`, display brightness `100` on both |

From then on both modules behaved as on Windows; see the next section. The
brightness of 5 sent at 15:57:31 was not what the keys reported afterwards.

Dial lifted off the base and put back, with the base powered:

```text
#   269.176 left    Dial Display
#   279.231 arrived Dial Display mount_path=1.4.3 placement: columns 5-6 rows 1-2 turned 0
```

The dial arrived at 16:03:05. From 16:03:08 to 16:20:24 every firmware and
brightness read on it came back empty, more than 300 `cmh info` runs,
while the keys, which had not been moved, answered all 60 reads made
alongside until 16:10. In that time:

- 16:03 to 16:08: nothing sent, nothing touched, `cmh monitor` running. No
  change.
- 16:08:12 to 16:08:17: `cmh software-mode on`, `cmh brightness 60`,
  `cmh image`, `cmh brightness 30` to the dial. All returned without error.
  No change; the owner reported the dial still showing the animation at
  16:17, not the picture.
- 16:17:4x: the owner pressed the dial once. No input report, no change on
  the display.
- 16:18:01: `cmh brightness 60`, `cmh image`, `cmh brightness 60` again. No
  change in the reads up to 16:20:24, when the experiment was closed.

The dial was left in that state. The kernel log shows the enumerations and
nothing else: no error, reset or timeout.

### Display modules once they answer

Between 15:57:31 and the lifting of the dial:

- Display brightness followed what was set: 5 and 40, later 60, on the
  keys; 40 and 60 on the dial.
- `cmh image --module keys --key 0` sent 3714 bytes, `cmh image --module
  dial` 14847 bytes. The owner reported "LINUX 0020" on the top-left key
  and on the dial.
- Input, the left roller turned one detent up, the dial one detent
  clockwise and pressed, the top-left key pressed:

  ```text
  01 2a 04 00 01 02 ff              EncoderDelta { index: 1, delta: -1 }   rollers
  01 44 03 00 03 01                 EncoderDelta { index: 0, delta: 1 }    dial
  01 44 03 00 04 01 01              ButtonDown { index: 0 }
  01 44 03 00 04 01                 ButtonUp { index: 0 }
  01 34 11 00 02 0f 01              ButtonDown { index: 0 }                keys
  01 34 11 00 02 0f                 ButtonUp { index: 0 }
  ```

  Two `cmh monitor` processes were running at that moment and both printed
  every one of these reports.

## Conclusions

- Mount path and placement on Linux equal the ones on Windows for the Base,
  15 LCD Keys, 3 Knobs, 2 Rollers and Dial Display: CONFIRMED. Enumeration
  and topology need no access to the hidraw nodes.
- The udev rule grants access on a natively installed system: CONFIRMED.
  Without it the SDK reports `Error::PermissionDenied` with the hint to the
  rule: CONFIRMED. A module that arrives before udev has applied the rule
  is picked up by the next retry of `cmh monitor`: CONFIRMED (once, the
  dial).
- Hot-plug of the whole base and of a single module, each returning with
  its mount path and placement: CONFIRMED.
- Base, 3 Knobs, 2 Rollers: firmware and device info reads, LED brightness
  read, input from the knobs and the rollers, LED colour on the base:
  CONFIRMED on Linux.
- 15 LCD Keys and Dial Display, once they answer: firmware read, display
  brightness set and read back, software mode, image upload, input:
  CONFIRMED on Linux.
- The 15 LCD Keys and the Dial Display can be in a state in which they
  answer the device info read but return no data for the firmware and
  brightness reads, and in which writes complete without error and do
  nothing that could be seen: software mode left the animation running on
  both, the picture did not appear on the dial, a brightness of 5 was not
  what the keys reported next. CONFIRMED, seen in each of the three cases
  observed: as found at the start (both modules), after the power cycle of
  the base (both), after reseating the dial (dial only).
- What ends that state is UNKNOWN. It ended once: both modules were found
  answering three minutes after the base power cycle. Waiting (17 minutes),
  holding the module open, the writes named above and a press of the dial
  did not end it on the reseated dial.
- Whether this happens on Windows is UNKNOWN. It was not noticed there; in
  experiment 0017 software mode took effect on the keys after a power
  cycle. No comparison was made on the same day and the same ports.
- A host cannot tell the state from an error code. An empty firmware read
  on a display module is the only sign found.

## Changes in the SDK

None. `Module::firmware` and `Module::display_brightness` return
`Error::MalformedReport` in that state, which is what the module sent.

## Not covered

- The 5 Faders on Linux.
- The `TAG+="uaccess"` variant of the udev rule.
- LED output on the 3 Knobs (covered on Linux by experiment 0012).
- `Deck` with software mode enabled, as an application would use it; `cmh
  monitor` leaves the mode alone.
- The cause of the unanswered reads. A passive `usbmon` capture of a
  power-up would show what the modules return to `GET_REPORT` and whether
  the writes reach them; it needs root and was not made.
