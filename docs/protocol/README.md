# MasterHUB USB HID protocol

Derived from our own observations of real hardware (see
[../experiments](../experiments)). Statements carry a confidence level:

- **CONFIRMED**: observed on hardware in a documented experiment.
- **LIKELY**: consistent with observations, but an alternative reading exists.
- **UNKNOWN**: not observed; listed so that nobody assumes it.

Per module: [base](base.md), [15 LCD Keys](keys15.md), [3 Knobs](knobs3.md),
[2 Rollers](rollers2.md), [Dial Display](dial-display.md),
[5 Faders](faders5.md). Where a module sits on the base:
[placement](placement.md).

## Device model

Every module, including the base, is an independent full-speed USB device
(USB 1.1, `bMaxPacketSize0` 8) with vendor ID `0x2516`, one configuration and
one HID interface (number 0). CONFIRMED (experiments 0001, 0003).

| Endpoint | Type | Max packet | Interval |
|---|---|---|---|
| `0x81` IN | interrupt | 64 | 3 ms |
| `0x02` OUT | interrupt | 64 | 3 ms |

Endpoint data was read from the base's configuration descriptor; the HID
report layout is the same on all modules.

A module is identified by its USB serial number string (20 characters, model
prefix + unit digits). Paths and USB addresses change on every reconnect.

## Module families

Report IDs and command codes come in three numbering blocks.

| Family | Modules | Input tag base | Output report size | Feature IDs |
|---|---|---|---|---|
| basic | Base, 5 Faders, 3 Knobs, 2 Rollers | `0x2a` | 64 | `0x2d`..`0x36` |
| keys | 15 LCD Keys | `0x34` | 320 | `0x37`..`0x3e` |
| dial | Dial Display | `0x44` | 320 | `0x47`..`0x4e` |

A code that is harmless in one family can mean something else in another,
so command tables are never shared between families.

## Input reports (device to host)

Interrupt IN, report ID `0x01`, always 64 bytes. CONFIRMED on all six
module types (experiments 0001, 0002, 0005).

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | report ID `0x01` |
| 1 | 1 | tag = family tag base + channel |
| 2 | 2 | payload length, u16 little endian |
| 4 | n | payload |
| 4+n | | zero padding up to 64 bytes |

Channel 0 carries control events:

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | event type (module specific) |
| 1 | 1 | number of controls |
| 2 | count | one byte per control, in index order |

Rotation events use one step code per encoder: `0x00` or `0x01` for one
detent, `0xff` for no movement. Which direction `0x00` stands for depends on
the module: counter-clockwise on the knobs, clockwise on the dial, away from
the operator on the rollers. Button events carry the full state of every button (`0` released,
`1` pressed) and fader events the position of every fader (0..100); a
report is sent on every change.

Channel 1 (`0x2b`) is only used by the base heartbeat, which carries the
orientation of the base ([base.md](base.md)). Channels 2 to 4 have not been
observed. UNKNOWN.

The basic modules send input without any initialisation from the host:
they never receive a single output or feature write from the vendor
application (experiment 0003) and report input with no vendor software
running (experiment 0005). CONFIRMED.

The 15 LCD Keys and the Dial Display only report input in software mode
(experiment 0017), which has to be enabled after every power-up.

Nothing a host sets is remembered across a power cycle unless the vendor
application's persist flag is used: software mode, display brightness, LED
settings and images are all back to their defaults (experiment 0017).

## Feature reports

32 bytes including the report ID, transported as HID class control requests
(`GET_REPORT` / `SET_REPORT`, report type 3).

Read response layout (CONFIRMED, experiment 0003):

| Offset | Size | Field |
|---|---|---|
| 0 | 1 | report ID |
| 1 | 1 | payload length |
| 2 | n | payload |

| Purpose | basic | keys | dial | Status |
|---|---|---|---|---|
| Running firmware version | `0x2e` | `0x38` | `0x48` | CONFIRMED |
| Device info | `0x31` | `0x3b` | `0x4b` | CONFIRMED |
| Fader positions (5 Faders only) | `0x33` | - | - | CONFIRMED |
| Other IDs declared in the descriptor | `0x2d` `0x2f` `0x30` `0x32` `0x34` `0x35` `0x36` | `0x37` `0x39` `0x3a` `0x3c` `0x3d` `0x3e` | `0x47` `0x49` `0x4a` `0x4c` `0x4d` `0x4e` | UNKNOWN, never read by the vendor application in our captures |

Firmware payload: 4 bytes of unknown meaning (constant per firmware build,
possibly a checksum) followed by 11 ASCII characters `NN.NN.NN.NN`.

```text
2e 0f 12 54 4c 2f 30 32 2e 30 30 2e 30 34 2e 30 30      Base, "02.00.04.00"
```

All feature writes use report ID `0x03`: `03 <command> <arguments>`.
Observed on the display modules only:

| Purpose | keys | dial | Status |
|---|---|---|---|
| Display brightness, `<level>` in percent | `03 34 <level>` | `03 44 <level>` | CONFIRMED |
| Software mode | `03 3a <0\|1>` | `03 49 01` | CONFIRMED as sent |

Details are in [keys15.md](keys15.md) and
[dial-display.md](dial-display.md). The vendor
application does not clear its buffer, so bytes after the arguments contain
stale data on the wire; they are not part of the command.

## Output reports (host to device)

Interrupt OUT, report ID `0x02`: 64 bytes on the basic family, 320 bytes on
the display modules, laid out as `02 <command> <arguments>`. Observed so
far:

| Command | Modules | Purpose | Details |
|---|---|---|---|
| `0x2c` `0x2d` `0x2e` | Base, 5 Faders, 3 Knobs | LED effect, colour, brightness | [base.md](base.md#output-leds), [faders5.md](faders5.md#output-leds), [knobs3.md](knobs3.md#output-leds) |
| `0x36` | 15 LCD Keys | key image | [keys15.md](keys15.md#output-key-image) |
| `0x45` | Dial Display | display image | [dial-display.md](dial-display.md#output-display-image) |

No output report is acknowledged. UNKNOWN: every other command.

## Vendor application behaviour

Observed with MasterHUB 1.0.0.113 (experiments 0001, 0003):

- Polls the USB string descriptors of every module every ~2 s (device
  enumeration; this is its hot-plug detection).
- On start: reads device info and firmware version of every module, writes
  software mode to both display modules, uploads all 15 key images, then
  redraws the dial once per second.
- Reads the fader positions on start and every ~2 s afterwards.
- Sends LED settings live while the user adjusts them and stores the colour
  in the module a few seconds later (experiment 0006). While running it
  re-sends the LED settings of its profile every 60 s, overriding anything
  another program has set (experiment 0009).
- Draws the mounted modules at their real positions and orientation. It
  derives them from the USB port path of each module
  ([placement](placement.md)); no HID report carries that information.
- When the application is killed nothing is sent; the modules keep their
  last images and the base heartbeat continues unchanged.

## Commands that must never be sent

Reported elsewhere as destructive or persistent. None of them was sent or
verified here; they are listed so that they can be recognised in captures.

| Module family | Report | Command | Reported effect |
|---|---|---|---|
| Base, Faders, Knobs, Rollers | output `0x02` | `0x2a` | firmware update |
| Base, Faders, Knobs, Rollers | output `0x02` | `0x2b` | set serial number |
| Base, Faders, Knobs, Rollers | output `0x02` | `0x31` | save factory data |
| Base, Faders, Knobs, Rollers | output `0x02` | `0x49` | firmware recovery |
| Base, Faders, Knobs, Rollers | output `0x02` | `0x4a` | factory reset |
| 15 LCD Keys | output `0x02` | `0x34` | firmware update |
| 15 LCD Keys | output `0x02` | `0x38` | save screen picture (flash write) |
| 15 LCD Keys | feature `0x03` | `0x35` | set serial number |
| 15 LCD Keys | feature `0x03` | `0x39` | firmware recovery |
| 15 LCD Keys | feature `0x03` | `0x3c` | save factory data |
| 15 LCD Keys | feature `0x03` | `0x3d` | factory reset |
| 15 LCD Keys | feature `0x03` | `0x3e` | animation recovery |

The same command byte can be harmless on one module and destructive on
another (`0x49` is "software mode" as a Dial Display feature command and
"firmware recovery" as an output command elsewhere), so command tables are
kept per module and per report type.

The SDK encodes a closed set of commands: raw report access is private to
the crate, and a unit test (`emits_only_the_verified_command_set`) pins the
exact list of reports that can leave it.
