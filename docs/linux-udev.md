# Linux access

Status: verified on Arch Linux under WSL2 with the 3 Knobs and the 15 LCD
Keys attached through usbipd-win
([experiment 0012](experiments/0012-linux-hardware.md)). Not yet run on a
machine with Linux installed natively, which is the only way to see the
real USB topology and therefore mount paths.

## Build dependencies

The `hidapi` crate is used with its default hidraw backend, which compiles
the bundled C library. That needs a C compiler, `pkg-config` and the libudev
development headers (`libudev-dev` on Debian/Ubuntu, `systemd-devel` on
Fedora, `systemd-libs` on Arch).

## Permissions

hidraw nodes are root-only by default. A udev rule opens the MasterHUB
modules to a group (tested, with `wheel`):

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

## Known differences from Windows

- hidraw paths (`/dev/hidrawN`) are reused by the next device that appears.
  The SDK re-checks vendor ID, product ID and serial number after opening a
  path and refuses to talk to anything else (`Error::WrongDevice`).
- Linux returns the report descriptor as stored in the module; Windows
  returns a reconstruction of the same layout.
- A freshly plugged module may not be accessible until udev has applied the
  rule; `cmh monitor` retries once per second.
