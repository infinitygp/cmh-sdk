# 0024: the standalone mode reads of 0021, repeated on Windows

- Date: 2026-10-12
- Host: Windows 11, hidapi crate 2.6.7 with its Windows backend; the same
  PC, ports and modules as in experiments 0020 and 0021
- Question: experiment 0021 was made on Linux only. Do the 15 LCD Keys
  behave the same when Windows sends the requests, and does the changed SDK
  cope?
- Tools: `cmh` built from commit `785eb0f` ("old", reads the firmware in
  any mode) and from `c1e9b48` ("new"); the `events` example of `c1e9b48`
- Vendor application closed for the whole experiment. No person at the
  device; everything below is what the tools printed.
- Not covered: the Dial Display, which cannot be sent back to standalone
  mode by a command.

Notation as in 0021: `F` software mode off, `N` software mode on, `R` one
`cmh info` of the old build (reads `0x38`, `0x3b`, `0x3c`).

## Observations

### Reads and writes

Each line is its own process, started about 0.3 s after the previous one.

| Step | Firmware read | Backlight read |
|---|---|---|
| old `R`, keys as the vendor application left them | `02.00.01.00` | 60 |
| old `F`, old `R` | 0 bytes | 0 bytes |
| old `N` (one write), old `R` | 0 bytes | 0 bytes |
| old `R`, 2 s later | 0 bytes | 0 bytes |
| new `cmh info --software-mode` | `02.00.01.00` | 60 |
| new `cmh brightness 40`, new `cmh info --software-mode` | `02.00.01.00` | 40 |
| old `F`, 1 s, new `cmh brightness 50`, new `cmh info --software-mode` | `02.00.01.00` | 50 |

An empty read reaches the SDK on Windows the way it does on Linux: the call
succeeds and returns no data.

### String descriptors while a write is pending

After old `F`, old `R`, three runs of `cmh list` in a row printed for the
keys (serial digits masked):

```text
serial=Key w/ IPS, 4x3<U+032A>MHKD
product="7<U+031C>COOLER MASTER<U+0326>15"  manufacturer="5AA4312516xxx"
```

and after new `cmh info --software-mode`, again:

```text
serial=MHKD15AA4312516xxxxx
product="15xKey w/ IPS, 4x3"  manufacturer="COOLER MASTER"
```

The three strings are the three string descriptors of the module read as
one stream that is out of step: every answer starts with the tail of the
previous descriptor (`U+032A`, `U+031C` and `U+0326` are the two header
bytes of the next descriptor, read as a character). This is the stale
buffer of 0021 as Windows sees it. Windows asks the module for its strings
on every enumeration, so the wrong text stays for as long as the state
lasts; Linux reads them once when the module is plugged in, which is why
0020 and 0021 show nothing of it.

### A deck on a module in that state

After old `F`, old `R`, the `events` example (a `Deck` with its defaults)
ran for six seconds:

```text
connected    Base (...)
connected    15 LCD Keys (Key w/ IPS, 4x3<U+032A>MHKD)
             columns 1-4, rows 1-3, turned 0 degrees
connected    3 Knobs (...)
connected    2 Rollers (...)
connected    Dial Display (...)
disconnected 15 LCD Keys (Key w/ IPS, 4x3<U+032A>MHKD)
connected    15 LCD Keys (MHKD15AA4312516xxxxx)
             columns 1-4, rows 1-3, turned 0 degrees
```

`cmh info --software-mode` afterwards gave firmware and backlight for the
keys and for the dial (which the vendor application had left in software
mode and the deck had sent its two writes and one read to as well).

## Conclusions

- The behaviour of 0021 is the module's, not Linux's: on Windows the keys
  answer the firmware and backlight reads with an empty report in
  standalone mode, and one software mode write after such a read does not
  bring them back. CONFIRMED (one run).
- The enabling sequence of the changed SDK, two writes and a firmware
  read, brings them back on Windows too, from standalone mode and from the
  state after an empty read. CONFIRMED (three runs, one of them through
  `Deck`).
- The backlight is set and read back correctly by the changed SDK right
  after it enabled software mode: CONFIRMED on Windows (twice).
- While a module is in the state after an empty read, Windows reports wrong
  serial, product and manufacturer strings for it: CONFIRMED. A `Deck`
  that meets such a module connects it under the wrong serial, repairs it
  by enabling software mode, and on its next rescan reports that module as
  disconnected and the real one as connected. The SDK no longer creates
  the state itself, since it does not read in standalone mode; it can
  still be left behind by other software, including builds of this SDK
  from before `31bab05`.

## Changes in the SDK

None.

## Not covered

- The Dial Display in this state on Windows.
- Avoiding the short-lived wrong identity in `Deck`.
