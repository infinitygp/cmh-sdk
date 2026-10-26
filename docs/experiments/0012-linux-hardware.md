# 0012: the SDK on Linux with real modules

- Date: 2026-10-12
- Host: Arch Linux under WSL2 (kernel 6.18.33.2-microsoft-standard-WSL2,
  systemd with udev running), rustc 1.98.0, libudev 261
- USB passthrough: usbipd-win 5.3.0; the 3 Knobs and the 15 LCD Keys were
  bound on Windows and attached to WSL one USB device each
- Access: udev rule from [../linux-udev.md](../linux-udev.md) (group `wheel`,
  mode 0660); every command below ran as an ordinary user
- The vendor application could not see the attached modules, so nothing
  competed with the SDK
- Evidence: [crates/cmh-sdk/fixtures/inputs/linux-keys-knobs.txt](../../crates/cmh-sdk/fixtures/inputs/linux-keys-knobs.txt)

## Procedure

1. `usbipd attach --wsl --busid <knobs>`; `cmh list --descriptors`,
   `cmh info`, `cmh led` in WSL.
2. Start `cmh monitor --raw` in WSL, attach the keys, run `cmh info`,
   `cmh image` (keys 0 and 14), `cmh brightness 20` then `55`, and an
   oversized image.
3. Detach the knobs from WSL, attach them again.
4. Set the knob LED zones 0, 1, 2 to red, green, blue; a person then pressed
   the top-left key, turned the top knob one detent clockwise and pressed
   it.

## Observations

- `/dev/hidraw0` appeared as `crw-rw---- root wheel`; the modules
  `vhci_hcd`, `usbhid` and `hid_generic` were loaded automatically.
- Enumeration returned product ID, serial number, manufacturer and product
  strings, usage page `000c` and usage `0001`, as on Windows.
- Report descriptor as stored in the modules (Linux returns the original,
  194 bytes on both):

  ```text
  05 0c 09 01 a1 01
  0a 00 ff 15 00 26 ff 00 75 08 96 3f 00 85 02 91 02      output  0x02, 63 bytes (keys: 96 3f 01, 319 bytes)
  0a 00 ff 15 00 26 ff 00 75 08 96 3f 00 85 01 81 02      input   0x01, 63 bytes
  0a 00 ff 15 00 26 ff 00 75 08 96 1f 00 85 03 b1 04      feature 0x03, 31 bytes
  (the same item sequence for the module's other feature IDs)
  c0
  ```

- Feature reads: knobs firmware `01.00.05.00`; keys firmware `02.00.01.00`
  and device info `14 03 05 0f e0 01 10 01 01 48 48`, identical to Windows.
- Writes completed without error: LED reports, software mode, two key
  images, brightness. The 240 x 240 image was refused before anything was
  sent.
- Hot-plug as logged by `cmh monitor`:

  ```text
  #     0.001 arrived 3 Knobs
  #     6.028 arrived 15 LCD Keys
  #    12.269 lost    3 Knobs (module disconnected: hidapi error: hid_read_timeout: unexpected poll error (device disconnected))
  #    19.096 arrived 3 Knobs
  ```

- Input:

  ```text
  01 34 11 00 02 0f 01              ButtonDown { index: 0 }      keys
  01 34 11 00 02 0f                 ButtonUp { index: 0 }
  01 2a 05 00 00 03 ff ff 01        EncoderDelta { index: 2, delta: 1 }   knobs
  01 2a 05 00 03 03 00 00 01        ButtonDown { index: 2 }
  01 2a 05 00 03 03                 ButtonUp { index: 2 }
  ```

- Seen by the person at the device: key 0 (top-left) showed "0", key 14
  (bottom-right) showed "14"; the left knob ring was red, the right one
  green, the single knob blue.
- USB topology: the hidraw node resolves to
  `/sys/devices/platform/vhci_hcd.0/usb1/1-1/...`, so the port chain is `1`
  and no mount path is available.

## Conclusions

- Enumeration, feature reads, output reports, feature writes, input reports
  and hot-plug through the hidraw backend: CONFIRMED on Linux for the 3 Knobs
  and the 15 LCD Keys.
- The udev rule grants access without root: CONFIRMED.
- The report descriptors reconstructed by Windows match the originals in
  report IDs and sizes: CONFIRMED.
- LED zone index equals knob index (zone 0 left, 1 right, 2 single knob):
  CONFIRMED.
- Mount path on Linux: still UNKNOWN. USB/IP hides the hub tree of the
  base; a machine running Linux natively is needed.

## Not covered

Base, 2 Rollers, Dial Display and 5 Faders on Linux; the
`Error::PermissionDenied` path (the rule was already installed).
