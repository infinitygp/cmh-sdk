# Linux access

Status: **untested**. The SDK has only been run on Windows so far; this page
records what is expected to be needed on Linux.

## Build dependencies

The `hidapi` crate is used with its default hidraw backend, which compiles
the bundled C library. That needs a C compiler, `pkg-config` and the libudev
development headers (`libudev-dev` on Debian/Ubuntu, `systemd-devel` on
Fedora, `systemd-libs` on Arch).

## Permissions

hidraw nodes are root-only by default. Grant the logged-in user access to
the MasterHUB modules:

```udev
# /etc/udev/rules.d/70-masterhub.rules
KERNEL=="hidraw*", SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2516", ATTRS{idProduct}=="01f6|01fe|0200|0202|0204|0206", TAG+="uaccess"
```

```sh
sudo udevadm control --reload-rules && sudo udevadm trigger
```

Without the rule `cmh` reports `permission denied opening /dev/hidrawN`.

## Known differences from Windows

- hidraw paths (`/dev/hidrawN`) are reused by the next device that appears.
  The SDK re-checks vendor ID, product ID and serial number after opening a
  path and refuses to talk to anything else (`Error::WrongDevice`).
- A freshly plugged module may not be accessible until udev has applied the
  rule; `cmh monitor` retries once per second.
