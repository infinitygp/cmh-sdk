# 0026: the vendor application exiting, a GIF on a key, the dial handed back

- Date: 2026-10-12
- Host: Windows 11, modules in their usual layout
- Tools: USBPcap through `tools/capture`, the vendor application operated
  by its owner, `cmh` for the last part
- Captures (local, not committed): `app-window-close`, `app-graceful-close`,
  `app-gif-key`, `app-quit`

## Procedure

1. With a capture running, the application's window was closed with its
   close button; in a second capture the process was asked to close without
   force (`taskkill` without `/F`).
2. With a capture running, the owner set a GIF as the picture of the
   top-left key in the application and confirmed.
3. With a capture running, the owner chose "Exit" in the menu of the
   application's tray icon.
4. Through the SDK, on the dial: software mode on, an image, software mode
   off; then the checks of experiment 0024.

## Observations

### Closing the window is not an exit

After both attempts of step 1 the five application processes were still
there and the traffic went on as before: a dial frame every second and the
LED settings of the base every 60 s.

### A GIF is a stream of ordinary key images

- 0.22 s before the first frame, one feature write to the keys that had not
  been seen before: `03 37 00 00 00 00`.
- Then 309 JPEGs of 1.5 to 2.2 kB to key 0 in 28 s, with the image command
  of [protocol/keys15.md](../protocol/keys15.md) (`02 36`), alternately
  60 ms and 120 ms apart, about 11 per second. Nothing else was sent to the
  keys. The owner saw the key animate.

### Exit

Within 0.13 s, right after the last GIF frame:

```text
 9.408 01fe set 03 3a 00        keys: software mode off
 9.409 0206 set 03 49 00        dial: software mode off
 9.484 01fe get 3b 0b 14 03 05 0f e0 01 10 01 01 48 48
 9.484 01fe set 03 3a 01        keys: software mode on
 9.506 0204 get 31 14 ...       rollers: device info
 9.529 0202 get 31 14 ...       knobs: device info
 9.537 0206 get 4b 0b 14 00 00 00 f0 00 f0 00 01 f0 f0
```

No LED command and no image followed. Afterwards the owner saw a black
screen on the keys and the built-in animation on the dial. Three
`MHDaemon.exe` processes of the vendor were still running; the application
processes were gone.

### `03 49 00` through the SDK

| Step | Result |
|---|---|
| `cmh info --module dial --software-mode` | firmware `01.00.04.00`, backlight 100 |
| `cmh image --module dial`, `cmh software-mode --module dial off` | no error |
| `cmh info --module dial` of the build before `31bab05` | firmware and backlight reads return 0 bytes |
| `cmh list` | dial listed as `serial=ial w/ IPS, 2x2<U+032A>MHDD`, product and manufacturer wrong as well |
| `cmh info --module dial --software-mode` | firmware `01.00.04.00`, backlight 100 |
| `cmh list` | the real serial and strings |

## Conclusions

- The application's close button and a close request leave it running in
  the tray. CONFIRMED.
- On "Exit" the application sends software mode off to both display
  modules: `03 3a 00` to the keys and `03 49 00` to the dial. CONFIRMED.
  This is the first sighting of `03 49 00`.
- The read of the device infos and the `03 3a 01` to the keys that follow
  0.08 s later look like another program opening the modules; the vendor's
  background process is the candidate. LIKELY. They are why the keys end
  up black, in software mode with no image, instead of showing their
  animation.
- `03 49 00` returns the Dial Display to standalone mode: the owner saw
  the animation after the application's exit, and after the same command
  from the SDK the firmware and backlight reads come back empty.
  CONFIRMED.
- The Dial Display behaves like the keys in experiments 0021 and 0024:
  empty reads in standalone mode, wrong strings on Windows afterwards, and
  the SDK's enabling sequence brings it back. CONFIRMED (one run). This
  closes the item left open in 0024.
- An animated key is nothing but repeated image uploads; the application
  sends about 11 frames per second. CONFIRMED. The module has no animation
  command that the application uses.
- `03 37 00 00 00 00` is sent to the keys once before the animation
  starts. It fits a claim published elsewhere that
  `03 37 <key> <r> <g> <b>` fills one key with a colour, here key 0 with
  black. LIKELY for this one use; other values were not sent or seen.

## Changes in the SDK

- `Module::set_software_mode(false)` now works on the Dial Display.
- `03 37` is not offered: a single-colour key is an ordinary image upload,
  which is verified, and the command was seen with one set of values only.
