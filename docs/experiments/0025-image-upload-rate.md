# 0025: how fast the display modules take images

- Date: 2026-10-12
- Host: Windows 11, the modules in their usual layout, vendor application
  closed
- Tool: a throwaway program outside this repository that calls
  `Module::set_image` in a loop and reads the firmware version after every
  step; `cmh monitor` (passive) in a second process for the input part; a
  person at the device
- Test images: baseline 4:4:4 JPEGs made with Pillow, a red "A" and a blue
  "B" sent in turn, a green "OK" at the end

## Procedure

1. Per module and image size: five seconds each at 5, 10, 20, 40 and 80
   images per second and then as fast as the calls return, the firmware
   version read after each step.
2. Five minutes with both modules at once: the keys at 30 images per second
   spread over all 15 keys (every key changes twice a second), the dial at
   20 per second. Meanwhile the owner pressed the top-left key five times
   and turned the dial five detents clockwise.
3. Three seconds at full speed on each module, then "OK" on every key and
   on the dial. The owner reported what was shown.

## Results

Step 1, at full speed:

| Module | Image size | Images per second | Time per `set_image` |
|---|---|---|---|
| 15 LCD Keys, one key | 1.2 kB | 589 | 1.7 ms mean, 13 ms worst |
| 15 LCD Keys, all keys in turn | 1.2 kB | 591 | 1.7 ms mean, 15 ms worst |
| 15 LCD Keys, all keys in turn | 12.7 kB | 190 | 5.3 ms |
| Dial Display | 4.0 kB | 67 | 15 ms mean, 42 ms worst |
| Dial Display | 13.7 kB | 62 | 16 ms |

Below the limit a call took 0.7 ms (keys, 1.2 kB), 5.4 ms (keys, 12.7 kB),
1.8 ms (dial, 4 kB) and 5.7 ms (dial, 13.7 kB), independent of the rate.
Every requested rate up to 80 per second was met on the keys; on the dial
80 per second came out as 68 and 62. The firmware read succeeded after
every step.

Step 2: 9000 images to the keys and 6000 to the dial in 300 s, both at
exactly the requested rate, worst call 1.2 ms and 2.7 ms. The monitor
recorded all five key presses (down and up) and all five dial detents. The
owner described the animation as smooth on all keys and on the dial.

Step 3: 1776 images to the keys (592 per second) and 207 to the dial (68
per second). The owner saw the green "OK" on all 15 keys and on the dial.

## Conclusions

- The dial limits the sender to about 60 to 68 images per second by
  delaying the transfer: CONFIRMED. The keys take data as fast as the host
  sends it, several hundred images per second: CONFIRMED.
- Sustained animation, 30 images per second on the keys together with 20
  per second on the dial for five minutes, leaves both modules responsive
  and looks smooth: CONFIRMED.
- Input is not lost while images are being uploaded, also when another
  process reads the same module: CONFIRMED (five presses, five detents).
- The image sent last is the one shown after a burst at full speed:
  CONFIRMED on both modules.
- Whether the keys display every image of a burst at several hundred per
  second, or skip some, is UNKNOWN; it cannot be seen by eye and does not
  matter for the final state.

## Changes in the SDK

None. `Module::set_image` blocks for the time given above; a host that
animates should budget about 1 ms per small key image and 2 to 6 ms per
dial frame.
