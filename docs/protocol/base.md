# Base (M0064)

- USB `2516:01f6`, product string `Base, 4x6`, serial prefix `MHBS01AA46`
- Family: basic (64 byte output report, feature IDs `0x2d`..`0x36`)
- Firmware seen: `02.00.04.00`

## Input: heartbeat

| | |
|---|---|
| Direction | device to host, interrupt IN `0x81` |
| Report | ID `0x01`, tag `0x2b` (channel 1), payload length 1 |
| Trigger | spontaneous, every ~516 ms, with or without the vendor application |
| Confidence | CONFIRMED (experiments 0001, 0003) |

```text
01 2b 01 00 01 00 .. 00
         |  `- payload: status byte, always 01 so far
         `---- length 1
```

The meaning of the status byte is UNKNOWN. It stayed `01` while the vendor
application was running, after it was killed and while it restarted.

The SDK decodes it as `InputMessage::Heartbeat` and emits no event.

## Feature reads

| ID | Content | Example | Confidence |
|---|---|---|---|
| `0x2e` | firmware version | `2e 0f 12 54 4c 2f "02.00.04.00"` | CONFIRMED |
| `0x31` | device info | `31 14` + 20 zero bytes | CONFIRMED (content is all zero) |

## Not investigated

- Output reports and LEDs: the vendor application sent nothing to the base
  in any capture so far. UNKNOWN.
- Feature reports `0x2d` `0x2f` `0x30` `0x32` `0x34` `0x35` `0x36`. UNKNOWN.
- Whether the base knows which module sits where. UNKNOWN; the base's own
  reports carry no such data so far. See the topology hypothesis in
  [../inventory.md](../inventory.md).
