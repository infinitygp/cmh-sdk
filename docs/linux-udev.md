# Linux access

Status: verified on a natively installed Linux (CachyOS, kernel 7.2) with
the Base, 15 LCD Keys, 3 Knobs, 2 Rollers and Dial Display
([experiment 0020](experiments/0020-native-linux.md)), and before that on
Arch Linux under WSL2 with the 3 Knobs and the 15 LCD Keys attached through
usbipd-win ([experiment 0012](experiments/0012-linux-hardware.md)). The
5 Faders have not been tried on Linux. One thing did not go as on Windows:
see [Display modules after power-up](#display-modules-after-power-up).

## Build dependencies

The `hidapi` crate is used with its default hidraw backend, which compiles
the bundled C library. That needs a C compiler, `pkg-config` and the libudev
development headers (`libudev-dev` on Debian/Ubuntu, `systemd-devel` on
Fedora, `systemd-libs` on Arch).

## Permissions

hidraw nodes are root-only by default. Listing modules, mount paths and
placements works regardless; opening a module does not. A udev rule opens
the MasterHUB modules to a group (tested, with `wheel`):

```udev
# /etc/udev/rules.d/70-masterhub.rules
KERNEL=="hidraw*", ATTRS{idVendor}=="2516", MODE="0660", GROUP="wheel"
```

On a desktop with logind, `TAG+="uaccess"` instead of `MODE` / `GROUP` gives
access to whoever is logged in at the seat; that variant is untested.

```sh
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Without access `cmh` reports `permission denied opening /dev/hidrawN`.

## Testing from WSL2

WSL2 has no USB access of its own; [usbipd-win](https://github.com/dorssel/usbipd-win)
forwards single USB devices into it. Each module is its own device.

```powershell
winget install --exact dorssel.usbipd-win      # once, elevated
usbipd bind --hardware-id 2516:0202            # once per module, elevated
usbipd attach --wsl --busid <busid>            # per session, see "usbipd list"
usbipd detach --busid <busid>                  # hand the module back to Windows
```

- An attached module is gone from Windows until it is detached.
- usbipd warns that the USBPcap filter driver is incompatible; binding and
  attaching worked regardless, without `--force`.
- The WSL kernel used (6.18) ships USB/IP, `usbhid` and hidraw; the modules
  load on attach.
- Devices arrive on a virtual host controller, so `cmh list` shows
  `mount_path=?`.

## Check on natively installed Linux

Run once ([experiment 0020](experiments/0020-native-linux.md)). With the
base in its usual layout:

```sh
cargo build --release
./target/release/cmh list
```

Result: the same `mount_path` and `placement` lines as on Windows. The
port chain in front of the mount path depends on the PC's own ports (it was
`1.4.3` here).

| Module | `mount_path` | `placement` |
|---|---|---|
| Base | `1.4.4` | - |
| 15 LCD Keys | `2.2` | columns 1-4 rows 1-3 turned 0 |
| 3 Knobs | `1.3.4` | columns 5-6 rows 3-4 turned 0 |
| 2 Rollers | `1.1.4` | columns 1-4 rows 4-4 turned 0 |
| Dial Display | `1.4.3` | columns 5-6 rows 1-2 turned 0 |

```sh
./target/release/cmh info
./target/release/cmh monitor --seconds 60
```

Result from `info`: the firmware versions read on Windows (Base
`02.00.04.00`, Keys `02.00.01.00`, Knobs `01.00.05.00`, Rollers
`01.00.03.01`, Dial `01.00.04.00`), for the keys and the dial only once
they had left the state described below. During `monitor`, with software
mode enabled on the keys and the dial beforehand (`cmh software-mode on`):
one roller step, one dial step and press and one key press each printed
their line; unplugging the base and lifting the dial off it printed `left`,
and both came back with mount path and placement.

```sh
./target/release/cmh image --module dial some-picture.png
./target/release/cmh led --module base --mode static --color 00ff00
```

Result: the picture on the dial, the base lit steady green.

## Display modules after power-up

In each case looked at on this host (as found at the start of the session,
after a power cycle of the base, after reseating the dial) the 15 LCD Keys
and the Dial Display answered the device info read but returned no data
for the firmware and display brightness reads:

```text
firmware: <malformed report: feature report too short: 0 bytes>
```

In that state `cmh software-mode`, `cmh brightness` and `cmh image`
finish without error and nothing changes on the module: it keeps playing
its built-in animation and sends no input. Base, Knobs and Rollers were
never affected.

The state ended once, three minutes after a base power cycle, for both
modules; everything then worked as on Windows. On the reseated dial it
lasted for the 17 minutes it was watched, through waiting, repeated writes
and a press of the dial. What ends it is unknown, and so is whether it is
specific to Linux; it was not noticed on Windows. Before relying on a
display module, read its firmware version: an empty answer means it is not
listening yet.

## Known differences from Windows

- hidraw paths (`/dev/hidrawN`) are reused by the next device that appears.
  The SDK re-checks vendor ID, product ID and serial number after opening a
  path and refuses to talk to anything else (`Error::WrongDevice`).
- Linux returns the report descriptor as stored in the module; Windows
  returns a reconstruction of the same layout.
- A freshly plugged module may not be accessible until udev has applied the
  rule; `cmh monitor` retries once per second.
