# 0006: lighting settings of the vendor application

- Date: 2026-10-12
- Method: one USBPcap capture per UI action; the vendor application
  1.0.0.113 was operated through GUI automation
- Captures (local only): `led-fader-brightness-drag`, `led-fader-ok-click`,
  `led-fader-effect-menu`, `led-fader-color`, `led-base`,
  `keys-display-settings`, `led-restore`, `led-restore-per-module`, `sdk-led`
  (all `20261012-124*` / `1250*`)
- Evidence: [fixtures/leds/vendor-app-lighting.txt](../../fixtures/leds/vendor-app-lighting.txt)
- Modules mounted: Base, 15 LCD Keys, 2 Rollers, 5 Faders

The application's "Lighting" tab lists only `5xFader` and `Base`, each with
Effect, Color and Brightness, plus a "Whole kit" mode with one set of
controls.

## Observations

| UI action | Reports (to the module's own HID device) |
|---|---|
| Open the 5xFader panel | `02 2d 01 <i> 74 3a df`, `02 2e 01 <i> 32`, `02 2c 01 <i> 01 00` for i = 0..4 |
| Drag 5xFader brightness 50 -> 100 | `02 2e 01 <i> <v>` for i = 0..4 at v = 69, 83, 93, 100 while dragging |
| Click OK | nothing |
| Effect Breathing / Strobe / Cycling / Random / Off | `02 2c 01 <i> 02` / `03` / `05` / `06` / `00`, each sent twice per zone |
| Effect Wave / Water / Rainbow | `02 2c 01 <i> 07` / `08` / `09` |
| Effect Static | `02 2c 01 <i> 01` |
| Colour picker hue to red | `02 2d 01 <i> df 3a 3a` |
| Colour picker to pure red | `02 2d 01 <i> ff 00 00` |
| About 3 s after closing the picker | `02 2d 01 <i> ff 00 00 01`, 140 ms apart per zone |
| Open the Base panel | `02 2d 00 00 74 3a df`, `02 2e 00 00 32`, `02 2c 00 00 01 00` |
| Drag Base brightness 50 -> 76 | `02 2e 00 00 <v>` at v = 68, 75, 76 |
| "Whole kit" brightness | the same `02 2e` reports to base and faders |

- The byte after the colour is `00` while the user is adjusting and `01`
  only in the delayed batch whose reports are spaced 140 ms apart. Reading:
  `01` asks the module to store the setting, and the spacing is the write
  time. LIKELY.
- Bytes behind the fields listed above vary and match earlier buffer
  contents of the application (fader positions, firmware text).
- No input report answers any of these.
- Nothing was sent to the 15 LCD Keys or the 2 Rollers by lighting actions.
  No control for the key display brightness was found in the Lighting tab,
  the Feature tab or the settings dialog.
- Effect code `04` is not offered by the application.

Side effect of the experiment: the colour of the 5 Faders was stored as red
and then as purple `74 3a df` again by the application. Settings were put
back (static, purple, brightness 50 on the base and 49 on the faders).
The application writes its profile file only when OK is clicked, so the
restore had to be repeated with OK after a first attempt was lost when the
application was closed.

## SDK check

With the vendor application closed, `cmh led` set the five fader zones to
red, green, blue, yellow and white and the base to green, all static at
brightness 100. On the wire the reports are `02 2d/2c/2e <module> <index>
<value>` followed by zeros, all completed successfully. The SDK refused the
knobs (not mounted, and unverified), index 5 and brightness 101 without
sending anything.

## Conclusions

- LED effect, colour and brightness commands for the Base (module code
  `00`, one zone) and the 5 Faders (module code `01`, five zones):
  CONFIRMED on the wire for the vendor application and the SDK.
- Effect codes 0, 1, 2, 3, 5, 6, 7, 8, 9 and their names: CONFIRMED against
  the application's menu.
- Zones are addressed individually: CONFIRMED on the wire; whether the
  module really shows different colours per zone awaits a photo.
- Persist flag: LIKELY; never sent by the SDK.

## Open questions

- Visual confirmation of the SDK run (per-zone colours, base colour).
- LED control of the 3 Knobs (module code unobserved).
- Key display brightness: no UI control found.
- Whether settings without the persist flag survive a power cycle.
