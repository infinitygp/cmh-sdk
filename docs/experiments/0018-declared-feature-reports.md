# 0018: reading the feature reports the vendor application does not use

- Date: 2026-10-12
- Scope: `GET_REPORT` only, and only for report IDs each module declares in
  its own report descriptor. Nothing was written. Done on explicit request
  of the hardware's owner.
- Tool: a throwaway probe built on hidapi, kept outside this repository so
  that the SDK keeps its closed command set
- Modules: Base, 15 LCD Keys, 3 Knobs, 2 Rollers, Dial Display (5 Faders not
  mounted). Unit-specific serial digits are masked.

## Responses

Basic family:

| ID | Base | 3 Knobs | 2 Rollers |
|---|---|---|---|
| `0x2d` | `0f 90 ff ea ec "02.00.00.00"` | `0f b1 b6 1e e3 "01.00.00.00"` | `0f b1 b6 1e e3 "01.00.00.00"` |
| `0x2e` | `0f 12 54 4c 2f "02.00.04.00"` | `0f a7 df 5b 3f "01.00.05.00"` | `0f 28 35 3e 25 "01.00.03.01"` |
| `0x2f` | same as `0x2e` | same as `0x2e` | same as `0x2e` |
| `0x30` | `14 "MHBS01AA4612516xxxxx"` | `14 "MHKB03AA2212523xxxxx"` | `14 "MHRR02AA4112516xxxxx"` |
| `0x31` | `14` + zeros | `14` + zeros | `14` + zeros |
| `0x32` | `01 32` | `03 64 64 64` | not declared |
| `0x34` | `01 09` | `03 09 09 09` | not declared |
| `0x35` | `00 01` | not declared | not declared |
| `0x36` | `10 38 c8 22 d3 b0 00 00 00` twice | `10 09 92 71 ee c4 00 00 00` twice | `10 23 5c 8f 27 94 00 00 00` twice |

Display modules:

| Purpose | 15 LCD Keys | Dial Display |
|---|---|---|
| first firmware report | `37`: `0f 73 21 42 e0 "02.00.00.00"` | `47`: `0f df 07 5a 29 "01.00.00.00"` |
| running firmware | `38`: `0f 29 63 1c 5d "02.00.01.00"` | `48`: `0f 26 a0 d8 0a "01.00.04.00"` |
| third firmware report | `39`: same as `38` | `49`: same as `48` |
| serial | `3a`: `14 "MHKD15AA4312516xxxxx"` | `4a`: `14 "MHDD01AA2212523xxxxx"` |
| device info | `3b`: as in experiment 0003 | `4b`: as in experiment 0003 |
| brightness | `3c`: `01 32` | `4c`: `01 32` |
| unknown | `3d`: `11 32 4a ac bc d3 85 07 00` twice, `00` | `4d`: `11 c3 07 4c 0d 95 7c 03 00` twice, `00` |
| unknown | `3e`: `10 1a d5 49 5e 74 00 00 00` twice | `4e`: `10 0b 0b 1a 92 74 00 00 00` twice |

Every read succeeded.

## Set, then read (through the SDK)

| Action | Read back |
|---|---|
| `cmh led --module knobs --index 1 --brightness 40` | `0x32`: `64 28 64` |
| `cmh brightness --module keys 30` | `0x3c`: `1e` |
| `cmh led --module knobs --mode static`, then `--mode breathing` | `0x34`: `09 09 09` both times |

## Conclusions

- Serial number: `0x30` / `0x3a` / `0x4a` return the same 20 characters as
  the USB string descriptor. CONFIRMED. Not needed by the SDK.
- Firmware: the first of the three reports holds an older version
  (`xx.00.00.00` everywhere) with its own four leading bytes, identical on
  the knobs and the rollers; the other two hold the running version.
  Reading as "factory / fallback firmware" and "running" / "pending" is
  LIKELY.
- LED brightness per zone (`0x32`) follows what is set at run time.
  CONFIRMED. SDK: `Module::led_brightness`.
- Display brightness (`0x3c` / `0x4c`) follows what is set at run time.
  CONFIRMED. SDK: `Module::display_brightness`.
- LED effect (`0x34`) did **not** follow run-time changes: it kept `09`
  (rainbow, the effect the modules show after power-up) while static and
  breathing were set. It most likely holds the stored setting, which only
  the vendor application's persist flag changes. LIKELY. The SDK does not
  expose it as a current value.
- `0x35` on the base returned `00 01`, a zero length followed by `01`. The
  base was lying flat, so this may be the orientation that the heartbeat
  also carries. UNKNOWN (one reading).
- `0x36` / `0x3e` / `0x4e`: sixteen bytes, an eight byte value twice,
  different on every module. UNKNOWN.
- `0x3d` / `0x4d`: an eight byte value twice plus a zero byte. Split as two
  little endian 32 bit numbers the second is 492 883 on the keys and 228 501
  on the dial, plausible sizes of the built-in animations, with the first
  number as their checksum. UNKNOWN, a guess.

## Not done

- The 5 Faders (declares `0x2d`..`0x34`, `0x36`); expected to match the
  knobs.
- Report `0x03`, the command report: not read.
