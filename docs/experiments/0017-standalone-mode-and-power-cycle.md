# 0017: standalone mode, brightness 0, power cycle

- Date: 2026-10-12
- Tools: `cmh led`, `cmh brightness`, `cmh software-mode`, `cmh monitor --raw`
  (passive); a person at the device
- Vendor application closed for the whole experiment
- Layout: the normal one

## Procedure

1. Through the SDK: knob LED zones static red at brightness 100, dial
   display brightness 0, keys software mode off (`03 3a 00`).
2. Person: describe the keys, press the top-left key, describe the dial.
3. Person: unplug the base's USB cable for about five seconds, plug it in
   again, describe knobs, keys and dial. Nothing is started on the host.

## Observations

Before the power cycle:

- Keys: an animation, the same one the module shows when it is plugged in
  without any software running.
- Pressing the top-left key produced no input report (157 s of monitoring,
  no report from the keys at all).
- Dial: completely dark.

After the power cycle:

- Keys: the same animation as before.
- Dial: a similar animation.
- Knob rings: cycling through colours.

`cmh monitor` across the power cycle:

```text
#   157.861 left    2 Rollers
#   160.453 left    3 Knobs
#   160.487 left    Base
#   160.519 left    Dial Display
#   160.582 left    15 LCD Keys
#   165.219 arrived 15 LCD Keys  mount_path=? placement: ?
#   166.268 arrived Base         mount_path=1.4.4
#   166.268 placed  15 LCD Keys  mount_path=2.2 placement: columns 1-4 rows 1-3 turned 0
#   166.268 arrived 3 Knobs      mount_path=1.3.4 ...
#   166.268 arrived 2 Rollers    mount_path=1.1.4 ...
#   166.268 arrived Dial Display mount_path=1.4.3 ...
```

## Conclusions

- `03 3a 00` puts the 15 LCD Keys into a standalone mode in which they play
  a built-in animation and send **no input reports**. Input from the keys
  requires software mode. CONFIRMED.
- Software mode is not remembered: after power-up the keys and the dial are
  in standalone mode again. CONFIRMED. A host has to enable it after every
  connect, which `Deck` does by default.
- Display brightness 0 switches the backlight off completely (dial).
  CONFIRMED. The brightness setting does not survive a power cycle either:
  the dial lit up again.
- LED settings sent without the persist flag are lost on power-down: the
  knobs returned to a colour cycle instead of the red set by the SDK.
  CONFIRMED.
- Uploaded images are not stored: the displays show their built-in
  animation after power-up. CONFIRMED.
- A module can enumerate before its base after power-up (here the keys,
  one second earlier). The deck then delivers the placement in a separate
  `Relocated` event; seen working. CONFIRMED.

## Follow-up in the same state

After the power cycle the dial was still in standalone mode (the SDK cannot
put it there itself), and the keys were switched to software mode through
the SDK without sending any image.

- Dial, standalone: turned one detent and pressed. No input report arrived,
  while the base heartbeat kept being received by the same monitor.
- Keys, software mode without images: a black screen.

Conclusions:

- The Dial Display, like the keys, sends no input in standalone mode.
  CONFIRMED (one attempt).
- Enabling software mode clears the key display to black; whatever should
  be visible has to be uploaded afterwards. CONFIRMED.

Afterwards the vendor application was started again and restored its icons,
the clock face and its lighting.
