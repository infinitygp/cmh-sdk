# Hardware and tooling inventory

Snapshot taken on 2026-10-12. Unit-specific serial digits are masked (`x`).

## Host and tools

| Item | Version |
|---|---|
| OS | Windows 11 Pro 10.0.26200 |
| Official MasterHUB application | 1.0.0.113 (`C:\Program Files\Cooler Master\MasterHUB\MasterHUB.exe`) |
| Wireshark / TShark | 4.6.9 (`C:\Program Files\Wireshark\tshark.exe`) |
| USBPcap | 1.5.4.0 (`C:\Program Files\USBPcap\USBPcapCMD.exe`), filter driver attached, 6 root hubs (`\\.\USBPcap1`..`6`) |
| Npcap | 1.88 |
| Rust | rustc 1.98.1, cargo 1.98.1, target `x86_64-pc-windows-msvc` |
| hidapi (crate) | 2.6.7 |

USBPcap needs administrator rights; see [capture-workflow.md](capture-workflow.md).

## Connected modules

Source: `Get-PnpDevice` / `Get-PnpDeviceProperty` and `cmh list --descriptors`.

| Module | VID:PID | USB product string | Serial | bcdDevice |
|---|---|---|---|---|
| Base | `2516:01f6` | `Base, 4x6` | `MHBS01AA4612516xxxxx` | `0000` |
| 15 LCD Keys | `2516:01fe` | `15xKey w/ IPS, 4x3` | `MHKD15AA4312516xxxxx` | `0000` |
| 3 Knobs | `2516:0202` | `3xKnob, 2x2` | `MHKB03AA2212523xxxxx` | `0000` |
| 2 Rollers | `2516:0204` | `2xRoller, 4x1` | `MHRR02AA4112516xxxxx` | `0000` |
| Dial Display | `2516:0206` | `1xDial w/ IPS, 2x2` | `MHDD01AA2212523xxxxx` | `0000` |
| 5 Faders | `2516:0200` | `5xFader, 4x2` | `MHFD05AA4212517xxxxx` | `0000` |

Manufacturer string is `COOLER MASTER` on every module.

Observations:

- The USB serial number is unique per module and starts with a 10 character
  model prefix (`MH` + type + count + `AA` + footprint). It is the only stable
  identity; USB addresses are reassigned on reconnect. CONFIRMED for all six
  module types (the 5 Faders were added in experiment 0005).
- The product string carries the module footprint in grid cells (`4x6`, `4x3`,
  `4x2`, `2x2`, `4x1`). CONFIRMED.
- The remaining 10 serial digits look like `1` + `YYWW` + 5 digit sequence
  (`2516`, `2523`). UNKNOWN, pattern guess only.
- `bcdDevice` is `0000` everywhere, so it does not carry the firmware version.

## USB topology

Every module is a separate full USB device behind a tree of Prolific
`067b:2586` hubs that live in the base. Port numbers below are relative to the
first Prolific hub (the base's upstream port).

```text
067b:2586 hub A  (base upstream)
|- port 1: 067b:2586 hub A.1
|  |- port 1: 067b:2586 hub A.1.1 -- port 4: 2 Rollers      (path 1.1.4)
|  |- port 2: 067b:2586 hub A.1.2    (no MasterHUB device)
|  |- port 3: 067b:2586 hub A.1.3 -- port 4: 3 Knobs        (path 1.3.4)
|  `- port 4: 067b:2586 hub A.1.4 -- port 3: Dial Display   (path 1.4.3)
|                                 `- port 4: Base           (path 1.4.4)
|- port 2: 067b:2586 hub A.2 ------- port 2: 15 LCD Keys    (path 2.2)
|- port 3: 067b:2586 hub A.3
`- port 4: 067b:2586 hub A.4
```

Nine 4-port hubs (1 + 4 + 4) give 28 downstream ports for a 4x6 = 24 cell
grid plus the base controller. Hypothesis (UNKNOWN): each contact position
on the base is wired to a fixed hub port, so the port path of a module
encodes its physical position. First data points are in
[experiment 0005](experiments/0005-no-app-hotplug-faders.md#part-5-port-paths).

## HID interface

Identical layout on every module: one interface (number 0), one top-level
application collection, usage page `0x000c`, usage `0x0001`. All report fields
are opaque byte arrays with usage `0xff00`.

| Report ID | Type | Size incl. ID | Modules |
|---|---|---|---|
| `0x01` | Input | 64 | all |
| `0x02` | Output | 64 | Base, 3 Knobs, 2 Rollers, 5 Faders |
| `0x02` | Output | 320 | 15 LCD Keys, Dial Display |
| `0x03` | Feature | 32 | all |
| `0x2d` `0x2e` `0x2f` `0x30` `0x31` `0x36` | Feature | 32 | Base, 3 Knobs, 2 Rollers, 5 Faders |
| `0x32` `0x34` | Feature | 32 | Base, 3 Knobs, 5 Faders |
| `0x33` | Feature | 32 | 5 Faders |
| `0x35` | Feature | 32 | Base |
| `0x37`..`0x3e` | Feature | 32 | 15 LCD Keys |
| `0x47`..`0x4e` | Feature | 32 | Dial Display |

CONFIRMED from the report descriptors returned by the OS. Windows
reconstructs descriptors from parsed data: the base's real descriptor is 211
bytes long according to its HID descriptor, the reconstruction 199.

USB descriptors of the base, taken from a USBPcap capture (experiment 0003):
USB 1.1, `bMaxPacketSize0` 8, one configuration, bus powered, 100 mA, one
HID interface without boot protocol, interrupt IN `0x81` and interrupt OUT
`0x02`, both 64 bytes with a 3 ms interval.

Raw descriptor of the base as returned by hidapi (199 bytes):

```text
05 0c 09 01 a1 01
85 01 0a 00 ff 15 00 26 ff 00 75 08 95 3f 81 02      input   0x01, 63 bytes
85 02 0a 00 ff 15 00 26 ff 00 75 08 95 3f 91 02      output  0x02, 63 bytes
85 03 0a 00 ff 15 00 26 ff 00 75 08 95 1f b1 04      feature 0x03, 31 bytes
85 2d ... 95 1f b1 04                                feature 0x2d, 31 bytes
(same item sequence for 0x2e 0x2f 0x30 0x31 0x32 0x34 0x35 0x36)
c0
```

The 15 LCD Keys and the Dial Display use `96 3f 01` (319 bytes) for output
report `0x02`.
