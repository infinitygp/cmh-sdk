# 0001: HID inventory and passive idle listening

- Date: 2026-10-12
- Tools: `cmh list --descriptors` and a passive hidapi read loop (now `cmh monitor --raw --heartbeat`); no report was written to any device
- State: official MasterHUB application 1.0.0.113 running, nothing touched
- Modules: Base, 15 LCD Keys, 3 Knobs, 2 Rollers, Dial Display

## Observations

1. All five modules expose one HID interface with the report layout listed in
   [../inventory.md](../inventory.md).
2. While idle, only the Base produced input reports, one every ~0.5 s:

   ```text
   01 2b 01 00 01 00 00 ... 00      (64 bytes)
   ```

   - `01` report ID
   - `2b` tag
   - `01 00` payload length 1 (u16 little endian)
   - `01` payload

3. The four other modules were silent for the whole window.

## Conclusions

- Input framing `01 <tag> <len u16 LE> <payload>`: CONFIRMED for the Base.
- Base heartbeat on tag `0x2b` with payload `01`: CONFIRMED. Whether it is
  spontaneous or an answer to something the application sends, and what the
  payload byte means, is UNKNOWN (needs a USB capture).

## Fixture

`crates/cmh-sdk/src/protocol/frame.rs`, test `parses_base_heartbeat`.
