# Base (M0064)

- USB `2516:01f6`, product string `Base, 4x6`, serial prefix `MHBS01AA46`
- Family: basic (64 byte output report, feature IDs `0x2d`..`0x36`)
- Firmware seen: `02.00.04.00`

## Input: heartbeat and orientation

| | |
|---|---|
| Direction | device to host, interrupt IN `0x81` |
| Report | ID `0x01`, tag `0x2b` (channel 1), payload length 1 |
| Trigger | spontaneous, every ~516 ms, with or without the vendor application |
| Payload | one byte: `01` base lying flat, `00` base standing upright on its short side |
| Confidence | CONFIRMED (experiments 0001, 0003, 0016) |

```text
01 2b 01 00 01 00 .. 00
         |  `- payload: 01 flat, 00 upright
         `---- length 1
```

The byte does not depend on the vendor application. Values for other ways
of placing the base (on a long side, face down) are UNKNOWN. While the base
is being moved the heartbeat can pause for several seconds.

SDK: `InputMessage::Heartbeat`; `InputEvent::OrientationChanged { upright }`
once after connecting and on every change.

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x2e` | firmware version | `2e 0f 12 54 4c 2f "02.00.04.00"` | CONFIRMED |
| `0x31` | device info | `31 14` + 20 zero bytes | CONFIRMED (content is all zero) |

## Output: LEDs

| | |
|---|---|
| Direction | host to device, interrupt OUT `0x02`, 64 bytes |
| Reply | none |
| Confidence | CONFIRMED on the wire (experiment 0006) and on the device (experiment 0008) |

| Report | Meaning |
|---|---|
| `02 2c 00 <zone> <mode> <persist>` | effect |
| `02 2d 00 <zone> <r> <g> <b> <persist>` | colour |
| `02 2e 00 <zone> <level>` | brightness, 0..100 |

The base has one zone, `<zone>` = `00`.

Effect codes: `00` Off, `01` Static, `02` Breathing, `03` Strobe, `05`
Cycling, `06` Random, `07` Wave, `08` Water, `09` Rainbow. The vendor
application offers nothing for `04`.

`<persist>` is `00` for live changes. The vendor application sends the
colour once more with `01` a few seconds after its colour picker is closed,
which makes the module keep that colour across a power cycle. CONFIRMED on
the base (experiment 0027). Effect and brightness are not kept: after
power-up a module runs the rainbow effect at brightness 100, and the stored
colour shows once the static effect is selected. The byte has only been
seen on the colour report; whether the effect report honours it is UNKNOWN.
The SDK always sends
`00`, and settings sent that way are lost on power-down (experiments 0017
and 0027).

```text
02 2d 00 00 74 3a df 00      colour 74 3a df
02 2c 00 00 01 00            static
02 2e 00 00 32               brightness 50
```

SDK: `Module::set_led_mode`, `set_led_color`, `set_led_brightness`.

## Not investigated

- Feature reports `0x2d` `0x2f` `0x30` `0x32` `0x34` `0x35` `0x36`. UNKNOWN.
- The base controller reports nothing about which module sits where;
  positions come from the USB topology, see [placement.md](placement.md).
