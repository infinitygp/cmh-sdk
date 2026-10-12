# 0008: LED zones seen on the device, fader and roller orientation

- Date: 2026-10-12
- Tools: `cmh led`, `cmh monitor --raw`; photos and answers from the person
  at the device
- Layout: keys top-left with the logo bar at the bottom, rollers along the
  bottom edge below the keys, 5 Faders rotated in the right-hand two columns
  (USB cable at the top-left)
- Vendor application closed

## LED zones (follow-up to 0006)

`cmh led` had set fader zones 0..4 to red, green, blue, yellow, white and
the base to green, all static at brightness 100.

Photo: the five fader strips show red, green, blue, yellow, white from top
to bottom; a green glow comes from under the base.

A first photo showed all five strips red. That was the vendor application,
started in between, applying the profile it had stored at that time; the
SDK colours were sent again after it was closed.

Conclusions:

- Per-zone LED colour on the 5 Faders and the base LED through the SDK:
  CONFIRMED visually.
- LED zone index equals fader index: CONFIRMED.
- Each strip lights from one end of the slot up to the knob, so it doubles
  as a level indicator; the firmware does that on its own.

## Fader orientation

Action: the topmost fader (nearest the USB cable) was pushed fully to the
right. Result: `AnalogChanged { index: 0, value: 100 }`.

- Index 0 is the fader at the top in this mounting. With the module turned
  so that the faders are upright and their LED strips start at the bottom,
  index 0 is the leftmost fader. CONFIRMED.
- Position 0 is the end where the LED strip starts, 100 the opposite end.
  CONFIRMED.

## Roller orientation

Action: the left roller was turned one detent away from the operator.
Result: `EncoderDelta { index: 1, delta: -1 }` (step code `00`).

- With the module along the bottom edge as described above, index 1 is the
  left roller and index 0 the right one. CONFIRMED.
- Step code `00` (delta -1) is "away from the operator / up", `01`
  (delta +1) is "towards the operator / down". CONFIRMED.
- The rollers cannot be pressed (statement of the operator, consistent with
  the absence of any button report). CONFIRMED.

## Display brightness

According to the operator the vendor application has no brightness setting
as such; brightness is an action that can be bound to a control, by default
to one knob of the 3 Knobs module. Not captured yet.

## Incident: "Firmware update failed" notice

At 13:07 the vendor application, freshly started, showed the notice
"Firmware update failed. Please try again later." Experiments were paused
and the state checked without writing to any module:

- All four mounted modules were enumerated normally by Windows.
- The application's own log of that start lists unchanged firmware versions
  (Keys `02.00.01.00`, Rollers `01.00.03.01`, Faders `01.00.05.00`, Base
  `02.00.04.00`) followed by
  `Error checking firmware version: TypeError: Failed to fetch`.

Reading: the application could not reach its update server and reports
that with a misleading text; no update was started. LIKELY (no USB capture
was running at that moment). The notice was dismissed; the "Device
Firmware" dialog of the application has never been opened in these
experiments.
