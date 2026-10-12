# 0027: what the vendor application's "store" byte keeps

- Date: 2026-10-12
- Host: Windows 11, modules in their usual layout
- Tools: the vendor application (colour picker of the Base panel) with
  USBPcap recording, `cmh led` and `cmh info`, a person at the device
- Captures (local, not committed): `led-base-colour-store`,
  `led-base-colour-restore`
- The SDK sent no report with the store byte set. The two stores described
  here were made by the vendor application as part of its normal use.

## Procedure

1. In the vendor application the colour of the base was changed from its
   purple `74 3a df` to green with the colour picker, and the picker was
   closed. OK was not clicked, so the application's profile stayed as it
   was.
2. The application was closed. `cmh led --module base --mode static
   --color 0000ff` made the base blue; the owner confirmed blue.
3. The owner unplugged the base for about five seconds and plugged it in
   again. Nothing ran on the host except a script that waited 15 s after
   the base returned and then sent `cmh led --module base --mode static`,
   an effect without any colour.
4. Afterwards the purple was put back with the application's colour picker.

## Observations

Sent by the application to the base in step 1:

```text
 9.314  02 2d 00 00 ff ff ff 00      picker open, white on the way
22.423  02 2d 00 00 05 ff 00 00      green picked
28.587  02 2d 00 00 05 ff 00 01      about 5 s after the picker was closed
39.579  02 2d 00 00 05 ff 00 00      the application's periodic refresh
39.581  02 2c 00 00 01 00
39.583  02 2e 00 00 32
```

The colour passed through while the picker was open (white) was never sent
with the last byte set.

After the power cycle:

- The owner saw the base cycle through colours (the rainbow effect).
- `cmh info`: LED brightness `[100]`; it had been `[50]` before.
- Feature report `0x34` read `01 09`, as in experiment 0018.
- After the script sent the static effect, the owner saw the base green.

In step 4 the application sent `02 2d 00 00 74 3a df 01` 12 s after its
last live colour report.

## Conclusions

- The last byte of the colour report, when `01`, makes the module keep
  that colour across a power cycle: CONFIRMED on the base. The colour sent
  later without it (blue) was lost.
- The stored colour is not shown by itself after power-up. The module
  starts with the rainbow effect at brightness 100; the stored colour
  appears once the static effect is selected without a new colour.
  CONFIRMED.
- Effect and brightness are not kept: CONFIRMED for the base. The vendor
  application was never seen sending the effect report with a byte set in
  that position, and the brightness report has no such byte.
- The vendor application stores a colour once, a few seconds after its
  colour picker is closed, and not while the user is still choosing. It has
  no separate control for this. CONFIRMED (experiment 0006 and this one).
- This refines experiment 0017, which concluded from the knobs returning
  to a colour cycle that LED settings without the store byte are lost: that
  observation showed the effect being reset, not the colour.

## Side observation: the vendor application and the state of 0024

After the power cycle the reader's own probe of experiment 0018 was run
once; it reads every declared feature report and therefore asked the keys
and the dial for firmware and backlight in standalone mode. Both were then
listed by Windows with the wrong strings of experiment 0024.

The vendor application, started in that situation, showed "No device
connected" and sent nothing to any module for 16 s. Half a second after
`cmh info --software-mode` had brought both modules back, it read the
device infos of all modules, set their modes and offered its normal start
screen.

Conclusion: the state after an empty read can keep the vendor application
from finding the deck at all. LIKELY (one observation; the cause inside the
application is not known).

## Changes in the SDK

None. The SDK still never sets the store byte. Protocol notes corrected:
the byte has been seen on the colour report only.
