# 0016: base orientation, dial touch, picture conversion, poll timing

- Date: 2026-10-12
- Tools: `cmh monitor --raw --heartbeat`, `cmh image`, a passive timing
  probe; a person at the device
- Evidence: [crates/cmh-sdk/fixtures/inputs/base-orientation.txt](../../crates/cmh-sdk/fixtures/inputs/base-orientation.txt)
- Layout: the normal one; vendor application running

## Base orientation

Procedure: with the heartbeat being logged, the base was stood upright on
its short side for a while and laid flat again.

| Time (s) | Heartbeat payload | Reports |
|---|---|---|
| 0 .. 547 | `01` | 1060 |
| 549 .. 556 | `00` | 14 |
| 556 .. 564 | `01` | 5 |
| 582 .. 595 | `00` | 25 |
| 596 | `01` | 1 |
| 597 .. 605 | `00` | 14 |
| 606 .. | `01` | 185 |

The heartbeat also paused for up to 17 s twice while the base was being
handled.

Conclusions:

- The heartbeat status byte reports orientation: `01` flat, `00` upright on
  the short side. CONFIRMED (the only `00` values of the whole day coincide
  with the base standing up). This matches the "VERTICAL / HORIZONTAL"
  marking under the base.
- Whether standing on a long side or lying face down gives other values is
  UNKNOWN.
- SDK: `InputEvent::OrientationChanged { upright }`, reported once after
  connecting and on every change.

## Dial touch

Procedure: the glass of the Dial Display was touched and swiped without
pressing the dial.

Result: no input report from the dial during that time.

Conclusion: the dial display is not touch sensitive, or at least reports
nothing. CONFIRMED as far as the HID interface goes. The three quick
press / release pairs of experiment 0002 were therefore real presses.

## Picture conversion

`cmh image --module keys --key 7 transparent.png` converted a 300 x 300 PNG
with a transparent background to a 72 x 72 JPEG of 4.4 kB and sent it.

Seen on the device: the orange disc with the text on a black background.

Conclusion: JPEGs produced by the `image` crate's encoder are accepted by
the keys: CONFIRMED. (The vendor application's files come from a different
encoder.)

## `Deck::poll` timing

An earlier version waited 1 ms per module for input. A review measured such
a wait at about 15 ms each on Windows, the length of a timer tick, which
would have made one pass over six modules take about 90 ms and lose fader
movement. `poll` now reads without blocking and sleeps between passes.

Measured with five modules open, nothing being touched:

| Call | Result |
|---|---|
| `poll(Duration::ZERO)`, 200 calls | median 1 us, maximum 1.5 us |
| `poll(50 ms)`, 40 calls | median 50.3 ms, maximum 50.5 ms |

Conclusion: polling no longer costs a timer tick per module: CONFIRMED on
Windows. Behaviour under a burst of reports is covered by a unit test with
a scripted transport, not by a hardware measurement.

Superseded by [0029](0029-idle-cost.md): `poll` now sleeps and a thread per
module waits in the read, so no pass over the modules is made at all.
