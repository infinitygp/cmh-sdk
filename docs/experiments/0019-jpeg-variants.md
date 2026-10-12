# 0019: which JPEGs the display modules decode

- Date: 2026-10-12
- Tool: the throwaway probe of experiment 0018 (hidapi, outside this
  repository), which sends the image reports of
  [protocol/keys15.md](../protocol/keys15.md) without the SDK's checks and
  then reads the firmware version to see whether the module still answers
- Test images: made with Pillow 12.2, each showing its own label
- Vendor application closed; software mode enabled through the SDK
- Evidence: a photo of the device taken by its owner (not committed)

## Procedure

1. Keys 0 to 8 received the same grey 4:4:4 image labelled "old".
2. One variant per key was sent in this order: key 0, 8, 5, 7, 2, 1, 4, 3, 6.
3. The dial received a reference image and then a large one labelled "30K".
4. The owner photographed the device.

## Results

15 LCD Keys, 72 x 72:

| Key | Variant | Frame | Sampling factors | Bytes | Shown |
|---|---|---|---|---|---|
| 0 | reference, quality 90 | SOF0 | `11 11 11` | 3615 | its own label |
| 8 | quality 30 | SOF0 | `11 11 11` | 1644 | its own label |
| 5 | optimised Huffman tables | SOF0 | `11 11 11` | 2493 | its own label |
| 7 | restart markers (`DRI`, 20 `RSTn`) | SOF0 | `11 11 11` | 3317 | its own label |
| 2 | 4:2:2 | SOF0 | `21 11 11` | 3253 | its own label |
| 1 | 4:2:0 | SOF0 | `22 11 11` | 2903 | its own label |
| 4 | greyscale, one component | SOF0 | `11` | 2205 | the image of key 1 |
| 3 | progressive | SOF2 | `11 11 11` | 2890 | the image of key 1 |
| 6 | noise, quality 100 | SOF0 | `11 11 11` | 19183 | its own label |

Dial Display, 240 x 240:

| Variant | Frame | Sampling factors | Bytes | Shown |
|---|---|---|---|---|
| reference | SOF0 | `11 11 11` | 8798 | replaced by the next one |
| noise, labelled "30K" | SOF0 | `11 11 11` | 28059 | its own label |

The module answered the firmware read after every upload, including the two
it did not decode.

## Conclusions

- The keys decode baseline JPEG with 4:4:4, 4:2:2 and 4:2:0 sampling, with
  default or optimised Huffman tables, with restart markers, at low quality,
  and at 19.2 kB. CONFIRMED (one upload each). 72 is not a multiple of 16,
  so partial blocks at the right and bottom edge are handled too.
- The keys do **not** decode progressive or greyscale JPEG. CONFIRMED (one
  upload each).
- A stream the module cannot decode is not ignored: the target key is
  redrawn with the image decoded last, here the 4:2:0 one of key 1, which
  was sent just before. The "old" image did not stay. CONFIRMED. A host must
  therefore not send such data; the SDK refuses it before sending anything.
- The dial decodes a 28.1 kB image. CONFIRMED. Progressive and greyscale
  were not tried on the dial.
- Follow-up: a 4:2:0 image (`22 11 11`, 6457 bytes) sent to the dial
  afterwards was displayed, as reported by the owner. CONFIRMED. 4:2:2 on
  the dial has not been looked at.
- The real size limit of either module is UNKNOWN; only that it is above
  the sizes listed.

## Changes in the SDK

- `MAX_IMAGE_LEN` raised from 16 KiB to 18 KiB, below the sizes seen
  working on both modules.
- `Module::set_image` accepts 4:2:2 and 4:2:0 on the 15 LCD Keys and,
  since the follow-up, 4:2:0 on the Dial Display.
- Progressive, greyscale and other sampling layouts stay refused.
