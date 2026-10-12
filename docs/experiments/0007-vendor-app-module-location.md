# 0007: how the vendor application locates modules

- Date: 2026-10-12
- Method: observation of the vendor application 1.0.0.113 only: its window
  and the plain JSON profile it writes to
  `%APPDATA%\MasterHUB\profile<N>_<base serial>`. Nothing was decompiled.
- Layout: keys top-left, rollers below them, 5 Faders rotated by 90 degrees
  in the right-hand two columns (layout B of experiment 0005)

## Observations

1. The application draws the base with every mounted module at its real
   place and in its real orientation, including the rotated faders. It did
   so right after start; nobody arranged anything in the UI. It also offers
   a landscape / portrait switch for the whole view.
2. The profile contains one record per LED-capable module:

   ```json
   {"moduleName": "Base_01",    "portNumbers": [1,4,3,1,4,4], "rootAddress": 4, "portNumber": "143144"}
   {"moduleName": "5xFader_01", "portNumbers": [1,4,3,1,1,3], "rootAddress": 4, "portNumber": "113",
    "parentSerialNumber": "<serial of the base>"}
   ```

   - `portNumbers` is the complete USB port chain from the root hub. It
     equals the Windows location path of the device
     (`...#USB(1)#USB(4)#USB(3)#USB(1)#USB(1)#USB(3)` for the faders).
   - `portNumber` of a module is the tail of that chain below the base's
     first hub: `113` is the path `1.1.3` measured in experiment 0005.
   - A left-over default record for a 3 Knobs module of another base lists
     `"portNumber": "34"`, a two-level path like the `2.2` of the keys.
3. No HID report seen so far carries position data, and the device info
   report of the basic modules is all zero.

## Conclusions

- The vendor application identifies where a module sits from its USB port
  path relative to the base: CONFIRMED (it stores exactly that path and
  nothing else that could serve).
- The port path therefore determines position, and together with the module
  type also orientation: LIKELY. The table that maps paths to grid cells
  lives inside the application and is not known.
- Consequence for the SDK: physical position can be offered once (a) the
  port path is read on each platform (Windows location path, Linux sysfs
  path; hidapi does not expose it) and (b) the path-to-cell table has been
  built from our own placements.

## Known placements

See [0005, part 5](0005-no-app-hotplug-faders.md#part-5-port-paths): five
module placements with their paths. The base controller itself is `1.4.4`.

## Next step

Mount one 2x2 module in each possible position (and both orientations where
the contacts allow) and record the path each time.
