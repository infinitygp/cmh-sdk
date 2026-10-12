# USB capture workflow (Windows, USBPcap + TShark)

Goal: one short capture per experiment, containing MasterHUB traffic only.

## One-time setup per session

USBPcap requires administrator rights. Start the helper once from an
**elevated** PowerShell and leave the window open:

```powershell
powershell -ExecutionPolicy Bypass -File tools\capture\capture-daemon.ps1
```

The daemon only accepts a capture name through
`captures\.control\start.json`; it never executes anything taken from a
request. Everything else runs unprivileged.

## One experiment

```powershell
tools\capture\capture.ps1 start knob1-cw     # 1. start recording
# 2. perform exactly one action (one detent clockwise on knob 1)
tools\capture\capture.ps1 stop               # 3. stop, prints file and packet count
```

Result: `captures\<yyyyMMdd-HHmmss>-<name>.pcapng`.

Turn the capture into reviewable text (one HID report per line, the format
read by `cmh replay` and by the unit tests):

```powershell
tools\capture\export-fixture.ps1 captures\<file>.pcapng                       # everything
tools\capture\export-fixture.ps1 captures\<file>.pcapng -Directions get,set   # feature traffic only
tools\capture\export-fixture.ps1 captures\<file>.pcapng -ProductId 0202 -From 4.4 -To 4.5
```

Compare two experiments with any text diff of the exported files. Input
reports can also be recorded without USBPcap, straight through hidapi:
`cmh monitor --raw > session.txt`.

For hot-plug experiments add `-AllDevices`: re-enumerated modules get new USB
addresses, so the source-side address filter cannot be used.

## What the daemon does

1. Records a sub-second descriptor probe on each USBPcap root hub and looks
   for devices with `idVendor == 0x2516` to find the right root hub and the
   current USB addresses. The probe file is deleted immediately.
2. Runs `USBPcapCMD -d <hub> --devices <addresses> --inject-descriptors`, so
   traffic from keyboards, storage and other devices on the same root hub is
   never written to disk.
3. On stop, filters again with `usb.device_address in {...}` into the final
   `.pcapng` and deletes the temporary raw file.
4. Stops on its own after 300 s.

## Rules

- Captures stay local: `captures/`, `*.pcap` and `*.pcapng` are git-ignored.
  Only curated text fixtures derived from them are committed.
- One action per capture, named after the action.
- Every experiment gets a note in `docs/experiments/` with the capture name,
  the action performed and the observed reports.
- The official application and this SDK may contend for the same HID
  interfaces; run them sequentially when an experiment writes to a device.
