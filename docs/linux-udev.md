# Linux access

Status: verified on a natively installed Linux (CachyOS, kernel 7.2) with
the Base, 15 LCD Keys, 3 Knobs, 2 Rollers and Dial Display
([experiment 0020](experiments/0020-native-linux.md)), and before that on
Arch Linux under WSL2 with the 3 Knobs and the 15 LCD Keys attached through
usbipd-win ([experiment 0012](experiments/0012-linux-hardware.md)). The
5 Faders followed in [experiment 0022](experiments/0022-faders-linux.md).

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
./target/release/cmh info --software-mode
./target/release/cmh monitor --seconds 60
```

Result from `info`: the firmware versions read on Windows (Base
`02.00.04.00`, Keys `02.00.01.00`, Knobs `01.00.05.00`, Rollers
`01.00.03.01`, Dial `01.00.04.00`). The keys and the dial give theirs only
in software mode, hence the option; see below. During `monitor`, with
software mode enabled on the keys and the dial beforehand: one roller step,
one dial step and press and one key press each printed their line;
unplugging the base and lifting the dial off it printed `left`, and both
came back with mount path and placement.

```sh
./target/release/cmh image --module dial some-picture.png
./target/release/cmh led --module base --mode static --color 00ff00
```

Result: the picture on the dial, the base lit steady green.

## Display modules after power-up

No vendor application runs on Linux, so the 15 LCD Keys and the Dial
Display are in standalone mode until something enables software mode. In
standalone mode they play their animation, send no input, and do not
answer the firmware and backlight reads. Asking for those anyway is what
made the first run of this check look broken: such a read makes the module
carry out later feature writes late or not at all
([experiment 0021](experiments/0021-standalone-mode-reads.md)).

The SDK therefore puts those questions only after it enabled software mode
itself, and enables it in a way that also works on a module left in that
state by other software. `Deck` enables software mode when a display
module connects; with `cmh`, `image`, `brightness`, `software-mode on` and
`info --software-mode` do. Nothing of this is specific to Linux as far as
is known; it was found here because the modules are never in software mode
on their own.

## Known differences from Windows

- hidraw paths (`/dev/hidrawN`) are reused by the next device that appears.
  The SDK re-checks vendor ID, product ID and serial number after opening a
  path and refuses to talk to anything else (`Error::WrongDevice`).
- Linux returns the report descriptor as stored in the module; Windows
  returns a reconstruction of the same layout.
- A freshly plugged module may not be accessible until udev has applied the
  rule; `cmh monitor` retries once per second.
