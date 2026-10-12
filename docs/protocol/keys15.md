# 15 LCD Keys (M4315)

- USB `2516:01fe`, product string `15xKey w/ IPS, 4x3`, serial prefix `MHKD15AA43`
- Family: keys (320 byte output report, feature IDs `0x37`..`0x3e`)
- Firmware seen: `02.00.01.00`
- Controls: 15 keys in a 5 x 3 grid, each showing a 72 x 72 image

## Key index order

Index 0 is the top-left key when the `COOLER MASTER` bar is at the bottom;
indexes run left to right, then top to bottom. The same index is used for
input and for images. CONFIRMED for input (experiment 0002) and for image
uploads of the vendor application (experiment 0003: image 0 is the picture
shown top-left).

## Input: key states

| | |
|---|---|
| Report | ID `0x01`, tag `0x34`, payload `02 0f k0 .. k14` |
| Trigger | on every press and release; carries the state of all 15 keys |
| Values | `00` released, `01` pressed |
| Confidence | CONFIRMED (experiment 0002) |

```text
01 34 11 00 02 0f 00 00 01 00 00 00 00 00 00 00 00 00 00 00 00   key 2 pressed
01 34 11 00 02 0f 01 01 00 ..                                    keys 0 and 1 held
```

SDK: `InputEvent::ButtonDown { index }` / `ButtonUp { index }`.

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x38` | firmware version | `38 0f 29 63 1c 5d "02.00.01.00"` | CONFIRMED |
| `0x3b` | device info | `3b 0b 14 03 05 0f e0 01 10 01 01 48 48` | CONFIRMED (bytes), LIKELY (meaning) |
| `0x3c` | backlight level | `3c 01 32` | CONFIRMED (experiment 0018) |

The firmware version and the backlight level are answered in software mode
only. In standalone mode the read succeeds with no data, and it must not be
attempted: afterwards the module carries out a feature write only when the
next feature write arrives, and drops it if a read comes first. Two
`03 3a 01` in a row followed by a firmware read end that state. CONFIRMED
on Linux (experiment 0021) and on Windows (experiment 0024). While it lasts
the module also answers string descriptor requests out of step, which
Windows shows as wrong serial, product and manufacturer strings. The device
info is answered in both modes.

Device info payload (11 bytes):

| Offset | Value | Interpretation | Confidence |
|---|---|---|---|
| 0 | `14` | unknown | UNKNOWN |
| 1 | `03` | rows | LIKELY |
| 2 | `05` | columns | LIKELY |
| 3 | `0f` | key count | LIKELY |
| 4..6 | `e0 01` | panel width 480, u16 LE | LIKELY |
| 6..8 | `10 01` | panel height 272, u16 LE | LIKELY |
| 8 | `01` | unknown | UNKNOWN |
| 9 | `48` | key image width 72 | CONFIRMED by uploaded JPEG size |
| 10 | `48` | key image height 72 | CONFIRMED by uploaded JPEG size |

## Feature write: software mode

| | |
|---|---|
| Report | `03 3a <mode>`, 32 bytes, `SET_REPORT` feature |
| Values | `01` application control, `00` standalone |
| Reply | none |
| Confidence | CONFIRMED (experiments 0003, 0017) |

In standalone mode the keys play a built-in animation and send no input
reports at all; key presses only arrive in software mode. The module starts
in standalone mode after every power-up, so the host has to send `03 3a 01`
after each connect. Switching to software mode blanks the display until
images are uploaded; sending `03 3a 01` again later leaves the images in
place (experiment 0021).

The vendor application sends `03 3a 00` right after opening the module and
`03 3a 01` about five seconds later, immediately before the first image
upload. It reads the firmware version only after that. When it exits it
sends `03 3a 00` (experiment 0026).

SDK: `Module::set_software_mode`. Enabling sends the command twice and
confirms with a firmware read, see [feature reads](#feature-reads).

## Feature write: display brightness

| | |
|---|---|
| Report | `03 34 <level>`, 32 bytes, `SET_REPORT` feature |
| Values | percent; the vendor application was seen sending 40..55 in steps of 5 |
| Reply | none |
| Confidence | CONFIRMED on the wire and on the device (experiment 0009: level 15 is clearly dimmer); level 0 was only tried on the dial |

```text
03 34 32      brightness 50
```

In the vendor application brightness is not a setting but an action bound
to a control; one detent changes both display modules by 5.

The level is ignored in standalone mode, and can be ignored during the
first 0.3 s or so after the module entered software mode (experiment
0021).

SDK: `Module::set_display_brightness`, which reads the level back and
sends it again until the module reports it.

## Output: key image

| | |
|---|---|
| Direction | host to device, interrupt OUT `0x02` |
| Report | ID `0x02`, 320 bytes |
| Reply | none |
| Confidence | CONFIRMED: on the wire for the vendor application (0003) and the SDK (0004), on the displays by photo (0005) |

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | report ID `0x02` |
| 1 | 1 | command `0x36` |
| 2 | 2 | chunk index, u16 LE, from 0 |
| 4 | 2 | image bytes in this report, u16 LE, 1..312 |
| 6 | 1 | key index 0..14 |
| 7 | 1 | `01` on the last chunk, else `00` |
| 8 | 312 | image bytes; unused tail is ignored (the vendor application leaves stale data there) |

```text
02 36 00 00 38 01 00 00 ff d8 ff e0 00 10 4a 46 49 46 ...   key 0, chunk 0, 312 bytes
02 36 1e 00 ad 00 00 01 ...                                 key 0, chunk 30, 173 bytes, last
```

Image format as produced by the vendor application: baseline JPEG (SOF0),
72 x 72, three components without chroma subsampling (4:4:4), JFIF header,
not rotated, 2.6 kB to 9.5 kB. The key lenses hide a few pixels along the
image borders, so content should keep a margin.

What else the module decodes (experiment 0019):

| JPEG property | Result |
|---|---|
| 4:2:2 and 4:2:0 chroma subsampling | displayed. CONFIRMED |
| optimised Huffman tables, restart markers, quality 30 | displayed. CONFIRMED |
| 19.2 kB | displayed. CONFIRMED; the real limit is UNKNOWN |
| progressive (SOF2) | not decoded. CONFIRMED |
| greyscale (one component) | not decoded. CONFIRMED |
| other image sizes | UNKNOWN |

A stream the module cannot decode does not leave the key as it was: the key
is redrawn with the image the module decoded last, whichever key that was
meant for. CONFIRMED (0019).

Uploads of all 15 keys take about 150 ms from the vendor application. The
module takes several hundred small images per second and keeps reporting
key presses meanwhile; 30 per second for five minutes looked smooth
(experiment 0025). Images are re-sent by the vendor
application on every start, so they are not stored permanently by this
command. LIKELY.

Images stay on the keys after the sending program exits (0005) but are
gone after a power cycle (0017).

SDK: `Module::set_image(key, jpeg)`; sends nothing unless the data is a
baseline three component JPEG (4:4:4, 4:2:2 or 4:2:0) of exactly 72 x 72
and at most 18 KiB.

## Not investigated

- `03 37 00 00 00 00`, a feature write the vendor application sends once
  before it starts animating a key. LIKELY "fill key 0 with black"
  (experiment 0026). The SDK does not send it; a plain image does the same.
- Full-screen images and other output commands. UNKNOWN; the vendor
  application was not seen using any. It animates a key by uploading
  images, about 11 per second (experiment 0026).
- The feature reports the vendor application does not use were read once;
  see the table in [README.md](README.md) and experiment 0018.
