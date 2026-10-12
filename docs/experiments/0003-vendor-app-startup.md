# 0003: vendor application idle, kill and start

- Date: 2026-10-12
- Captures (local only, not committed):
  `20261012-120447-idle-app-running.pcapng` (6 s, 664 packets),
  `20261012-120610-app-kill-and-start.pcapng` (38 s, 4136 packets)
- Evidence: [fixtures/startup/vendor-app-init.txt](../../fixtures/startup/vendor-app-init.txt)
  (feature traffic of the start, image uploads omitted)
- Vendor application: MasterHUB 1.0.0.113

## Procedure (automated)

1. `capture.ps1 start idle-app-running`, wait 6 s, stop.
2. `capture.ps1 start app-kill-and-start`; at 3.2 s kill all `MasterHUB.exe`
   processes; at 11.9 s start the application again; stop at 38 s.

Nothing was touched on the device.

## Observations

Idle, application running:

- Base: heartbeat `01 2b 01 00 01` on interrupt IN every 516 ms, not
  preceded by any request.
- Dial: 43 output reports `02 45 ..` once per second = one 240 x 240 JPEG of
  the clock face (13.4 kB).
- Every module: three `GET_DESCRIPTOR(string)` requests every ~2 s.
- Keys, knobs, rollers: no report traffic at all.

After the kill (3.2 s to 15.7 s): the dial stream stops, the heartbeat
continues unchanged, nothing else happens on the bus.

Start (times in capture seconds):

| Time | Module | Request | Data |
|---|---|---|---|
| 15.680 | Dial | get `0x4b` | `4b 0b 14 00 00 00 f0 00 f0 00 01 f0 f0` |
| 15.680 | Dial | set `0x03` | `03 49 01` |
| 15.682 | Dial | out | blank 240 x 240 JPEG (2.2 kB, 8 chunks) |
| 15.704 | Rollers | get `0x31` | `31 14` + zeros |
| 15.728 | Knobs | get `0x31` | `31 14` + zeros |
| 15.738 | Keys | get `0x3b` | `3b 0b 14 03 05 0f e0 01 10 01 01 48 48` |
| 15.738 | Keys | set `0x03` | `03 3a 00` |
| 15.887 | Dial | get `0x48` | firmware `01.00.04.00` |
| 16.106 | Rollers | get `0x2e` | firmware `01.00.03.01` |
| 16.263 | Base | get `0x31` | `31 14` + zeros |
| 16.278 | Base | get `0x2e` | firmware `02.00.04.00` |
| 16.340 | Knobs | get `0x2e` | firmware `01.00.05.00` |
| 20.659 | Dial | set `0x03` | `03 49 01` |
| 20.663 | Keys | set `0x03` | `03 3a 01` |
| 20.66..21.29 | all | get firmware again | same values |
| 21.579..21.731 | Keys | out | 15 JPEGs 72 x 72 via `02 36`, keys 0..14 in order |
| 21.872..22.008 | Keys | out | the same 15 images a second time |
| 16.21, 20.49, 21.56 | Dial | out | the blank frame again |
| from 21.62 | Dial | out | clock face once per second |

Bytes following the listed data in feature writes are leftovers of earlier
buffers (device info, JPEG tail) and differ between the two writes to the
same module.

No input report other than the base heartbeat appears anywhere: image
uploads and feature writes are not acknowledged.

## Conclusions

- Heartbeat is spontaneous: CONFIRMED.
- Feature read framing `<id> <len> <payload>`, firmware and device info IDs:
  CONFIRMED.
- Image upload format for keys (`0x36`) and dial (`0x45`): CONFIRMED; images
  extracted from the capture decode as upright baseline JPEGs.
- Basic modules receive no writes at all from the vendor application:
  CONFIRMED.
- Software mode commands `03 3a <0|1>` and `03 49 01`: CONFIRMED as sent;
  their effect on the modules is LIKELY (name taken from third-party notes).
- The application never read a serial number through a feature report; it
  uses the USB string descriptor.

## Open questions

- Why keys get `03 3a 00` first and `03 3a 01` five seconds later.
- What the modules do on a clean application exit (only a kill was tested).
