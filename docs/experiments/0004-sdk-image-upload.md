# 0004: first writes from the SDK

- Date: 2026-10-12
- Capture (local only): `20261012-121532-sdk-image-upload.pcapng` (1546 packets)
- Evidence: [fixtures/images/key-00-sdk-upload.txt](../../fixtures/images/key-00-sdk-upload.txt)
- Firmware: Keys `02.00.01.00`, Dial `01.00.04.00`

## Procedure (automated)

1. `capture.ps1 start sdk-image-upload`.
2. Kill the vendor application so that it cannot redraw anything.
3. `cmh info` beforehand had read firmware and device info of all modules.
4. `cmh image --module keys --key N key-NN.jpg` for N = 0..14 (72 x 72
   baseline 4:4:4 JPEGs showing the key number and a top-left marker).
5. `cmh image --module dial dial.jpg` (240 x 240, 11.4 kB, arrow pointing up).
6. Stop the capture.

`cmh image` sends the software mode feature write before the image.

## Observations

- Feature reads through the SDK returned exactly the values seen in
  experiment 0003 for all five modules.
- On the wire each upload is `03 3a 01` (keys) or `03 49 01` (dial) followed
  by the expected `02 36` / `02 45` chunks; the reassembled key 0 image is
  byte-identical to the source file.
- Every transfer completed with USBD status success.
- The dial upload used `00` at offset 6 instead of the `30` sent by the
  vendor application.

## Conclusions

- Feature reads through hidapi: CONFIRMED working on hardware.
- Software mode write and image upload: CONFIRMED on the wire.
- Visual result (do the keys show 0..14 upright, does the dial show the test
  picture): pending confirmation by a person.
