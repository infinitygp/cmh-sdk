# 0021: why the display modules stopped answering

- Date: 2026-10-12, continuing [experiment 0020](0020-native-linux.md) on
  the same host and modules
- Question: what puts the 15 LCD Keys and the Dial Display into the state
  seen in 0020, and what ends it
- Tools: `cmh` from this repository, called in scripted sequences about
  15 ms apart; the `cmh` of the previous commit, for comparison; a passive
  `usbmon` capture of the base's USB bus, started by the owner and
  filtered to the base and its modules
- Outside the SDK's command set, both run by the owner as root and both
  standard USB requests, no HID report: `lsusb -v` on the keys, and three
  `GET_DESCRIPTOR(string)` requests to the keys
- A person at the device for the physical steps

Notation for sequences on the keys: `F` software mode off, `N` software
mode on, `Bn` backlight n, `R` one `cmh info` of the previous commit, which
reads firmware `0x38`, device info `0x3b` twice and backlight `0x3c`.
"Answers" means that the firmware read returns data.

## Observations

### Standalone mode leaves two reads empty

On keys that answered, `F` was followed 17 ms later by

```text
a1 01 0338 0000 0020   ->  0 bytes        firmware
a1 01 033b 0000 0020   -> 32 bytes        3b 0b 14 03 05 0f e0 01 10 01 01 48 48
a1 01 033c 0000 0020   ->  0 bytes        backlight
```

which is the picture of 0020. The transfers complete without error; the
data stage is empty.

### An empty read delays the writes that follow

| Sequence | Result | Runs |
|---|---|---|
| `F N R` | answers | more than 20, with 0 to 5 s between `F` and `N` |
| `F R N R` | does not answer, also when 5 s pass after `N` | 5 |
| `F R N N R` | answers | more than 40 |
| `F R N`, 3 s, `N R` | answers | 1 |
| `F R B45 N R` | does not answer | 1 |
| `F R F N R` | does not answer | 1 |
| `F R N N`, then `F N R` | does not answer | 1 |
| `F R N N N N`, then `F N R` | does not answer | 1 |
| `F R N N R`, then `F N R` | answers | 2 |

After an empty read a feature write is carried out when the next feature
write arrives, and not at all if a read arrives first. That stays so while
only writes follow. A read that returns data ends it.

The same on the reseated dial of 0020, which had been read every few
seconds and had ignored every write for 35 minutes. Three writes without a
read between them:

```text
21 09 0303 0000 0020   03 49 01      software mode on
21 09 0303 0000 0020   03 44 3c      backlight 60        17 ms later
21 09 0303 0000 0020   03 49 01      software mode on    18 ms later
a1 01 0348 0000 0020   -> 48 0f 26 a0 d8 0a "01.00.04.00"
a1 01 034c 0000 0020   -> 4c 01 3c
```

The owner had turned the dial one detent a minute earlier; the reads
between that and the first write were still empty.

### What does not matter

- Pressing or turning the dial in standalone mode puts nothing on the bus;
  the interrupt IN endpoint stayed silent while `cmh monitor` polled it.
- Time, and whether a process holds the module open (0020).
- Standard requests: after the `lsusb -v` requests and after a string
  descriptor request, `R N R` still did not answer.

### The module answers standard requests from a stale buffer

After an empty backlight read:

```text
80 06 0600 0000 000a   -> 3c 01 3c 00 00 00 00 00 00 00      device qualifier
80 06 0301 0409 00ff   -> 28 bytes: 3c 01 3c 00 ...          string 1, first request
80 06 0301 0409 00ff   -> 28 bytes: 00 00 00 00 1c 03 "COOLER MAST"    second request
```

`3c 01 3c` is the backlight report for level 60, which was the level set.
The module had prepared the answer to the read and not sent it; 32 bytes
then stayed ahead of whatever it sent next on the control endpoint.

### Backlight

- Written in standalone mode it never took effect: `F`, `B20`, `N` and a
  read gave the old level, three times out of three, twice with seconds
  between the steps.
- Written shortly after the module entered software mode it was sometimes
  ignored. After `F R N N R`: ignored when sent 0.1 s and 0.2 s later,
  applied at 0.3 s, 0.5 s, 0.8 s and 1.2 s. After `F N R` without an
  empty read: applied in 16 runs out of 18.
- Sent again after a read-back that showed the old level, it was applied.

## Conclusions

- In standalone mode the 15 LCD Keys answer the firmware and backlight
  reads with an empty report: CONFIRMED. The Dial Display shows the same
  symptom after power-up; it cannot be sent back to standalone mode to
  repeat the test.
- Such a read makes the module carry out each later feature write only
  when the next one arrives, and drop it when a read comes first:
  CONFIRMED on the keys, and it fits both episodes on the dial.
- Two software mode writes in a row bring the module into software mode
  from that state, and a firmware read after them ends it: CONFIRMED on
  the keys.
- This explains experiment 0020. `cmh info` asked every module for its
  firmware, the display modules were in standalone mode, and every later
  command was a single write followed by a read. The state "ended" at
  15:57 because the next command to each module happened to be another
  write with no read in between; on the keys that was a backlight level,
  which is why it was not applied. Touch and time were coincidences.
- It is behaviour of the modules, not of Linux: LIKELY. The requests on
  the bus are the ones Windows sends, and the vendor application was never
  seen reading the keys' firmware before `03 3a 01` (experiment 0003). Not
  repeated on Windows.
- The backlight can only be set in software mode: CONFIRMED on the keys.
  A level sent within about 0.3 s after entering software mode can be
  lost: CONFIRMED on the keys; the cause is UNKNOWN.

## Changes in the SDK

- `Module::firmware`, `Module::display_brightness` and
  `Module::set_display_brightness` return `Error::NotInSoftwareMode` on a
  display module, without sending anything, unless the same handle enabled
  software mode.
- `Module::set_software_mode(true)` sends the command twice and reads the
  firmware version to confirm; `Error::NotInSoftwareMode` if the answer is
  empty. `Deck` then reports the module as not opened and tries again on
  its next rescan.
- `Module::set_display_brightness` reads the level back and sends it again,
  up to eight times 60 ms apart, until the module reports it;
  `Error::BrightnessNotApplied` if it never does.
- `cmh info` leaves the two reads out on display modules unless
  `--software-mode` is given; `cmh brightness` enables software mode first.

Checked on the keys with the new `cmh`:

| Start | Command | Read back | Runs |
|---|---|---|---|
| `F`, then two `R` of the previous commit | `cmh brightness n` | n | 5 of 5 |
| `F`, 1 s | `cmh brightness n` | n | 5 of 5 |

## Not covered

- The Dial Display with the new SDK from standalone mode, and both modules
  right after a power-up.
- Whether sending software mode on to a module that already shows pictures
  changes what it shows.
- Windows.
- Why the firmware behaves this way.
