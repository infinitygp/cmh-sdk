# Linux permissions

Every MasterHUB module is a USB HID device of its own and appears on Linux
as a `/dev/hidrawN` node. These nodes belong to root by default. Listing
the modules works without access; opening one fails with
`Error::PermissionDenied` (or `cmh` reporting `permission denied opening
/dev/hidrawN`). A udev rule gives access to the MasterHUB nodes only, so
that applications do not have to run as root.

This page is the reference for that rule. Hardware notes from the Linux
checks, including WSL2, are in [linux-udev.md](linux-udev.md).

## Which devices the rule matches

The rule matches the hidraw nodes whose USB device has vendor ID `2516`
(Cooler Master) and one of the six MasterHUB product IDs:

| Product ID | Module |
|---|---|
| `01f6` | Base |
| `01fe` | 15 LCD Keys |
| `0200` | 5 Faders |
| `0202` | 3 Knobs |
| `0204` | 2 Rollers |
| `0206` | Dial Display |

`idVendor` and `idProduct` are attributes of the USB device, two levels
above the hidraw node (hidraw node, HID device, USB interface, USB
device). `ATTRS{...}` searches the node and its parents, and both keys of
one rule must match on the same parent, which they do here. Other Cooler
Master devices, such as keyboards or mice with the same vendor ID, are not
matched.

## Desktop sessions: `uaccess`

On a desktop where systemd-logind manages the seat, tag the nodes with
`uaccess`. logind then grants the user who is logged in at the seat access
through an ACL, and takes it away when they log out or switch user.

```udev
# /etc/udev/rules.d/70-masterhub.rules
# Cooler Master MasterHUB modules: Base, 15 LCD Keys, 5 Faders, 3 Knobs,
# 2 Rollers, Dial Display.
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2516", ATTRS{idProduct}=="01f6|01fe|0200|0202|0204|0206", TAG+="uaccess"
```

The file name matters: the `uaccess` tag is acted on by
`73-seat-late.rules`, so the rule must sort before it. A name starting with
`70-` does.

`uaccess` does nothing without a local logind seat: over SSH, in a system
service, in a container or on WSL2, use a group instead.

## Services and headless systems: a dedicated group

Create a group that exists only for this purpose and give it read and
write access:

```sh
sudo groupadd --system masterhub
```

```udev
# /etc/udev/rules.d/70-masterhub.rules
SUBSYSTEM=="hidraw", ATTRS{idVendor}=="2516", ATTRS{idProduct}=="01f6|01fe|0200|0202|0204|0206", GROUP="masterhub", MODE="0660"
```

Then add the account that runs the application to the group, either with
`sudo usermod --append --groups masterhub <user>` (effective from that
user's next login) or, for a systemd service, with
`SupplementaryGroups=masterhub` in its unit.

Avoid broader rules: `MODE="0666"` gives every local account and process
access, a rule without `idProduct` covers every Cooler Master device, and
an administrative group such as `wheel` mixes device access with the right
to administer the machine.

## Applying the rule

Reload the rules and apply them to the nodes that already exist, or unplug
and replug the modules:

```sh
sudo udevadm control --reload
sudo udevadm trigger --action=change --subsystem-match=hidraw
```

A module that is plugged in later gets the permissions from the rule
automatically. The kernel creates its node before udev has applied the
rule; `Deck` allows for that: it tries a failed open again for two seconds
before it reports `DeckEvent::OpenFailed`, and keeps trying once a second
after that.

## Checking the result

Find the MasterHUB nodes (the HID ID contains the vendor ID):

```sh
grep -l ':00002516:' /sys/class/hidraw/hidraw*/device/uevent
```

Each match `/sys/class/hidraw/hidrawN/device/uevent` stands for
`/dev/hidrawN`. For one of them:

```sh
# The parent attributes the rule matches on.
udevadm info --attribute-walk --name=/dev/hidrawN | grep -E 'idVendor|idProduct'

# uaccess: the tag is set and the logged-in user has an ACL entry.
udevadm info --query=property --name=/dev/hidrawN | grep TAGS
getfacl /dev/hidrawN

# Group variant: owner group masterhub, mode crw-rw----.
ls -l /dev/hidrawN
```

The node numbers change when modules are reconnected; never store them.
Finally, open a module as the user in question, for example with the
`events` example or `cmh list` and `cmh info` from a checkout of the
repository. `cmh list` works without access; `cmh info` needs it.

## Verification status

The hardware checks on Linux (experiments
[0012](experiments/0012-linux-hardware.md) and
[0020](experiments/0020-native-linux.md)) ran with an earlier, broader rule
that matched vendor ID `2516` alone and gave the `wheel` group access. The
narrower rules on this page add the product IDs, which are attributes of
the same USB device; they, and the `uaccess` variant, have not been tested
on hardware.

## Building on Linux

The `hidapi` crate compiles its bundled C library against libudev, which
needs a C compiler, `pkg-config` and the libudev development files:
`libudev-dev` on Debian and Ubuntu, `systemd-devel` on Fedora,
`systemd-libs` on Arch Linux.
