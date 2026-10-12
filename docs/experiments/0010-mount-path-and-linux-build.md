# 0010: mount path through the SDK, first Linux build

- Date: 2026-10-12
- Layout: keys top-left, dial top-right, knobs bottom-right, rollers
  bottom-left (layout A of experiment 0005)

## Mount path

`HidInterface::port_chain` holds the hub port numbers from the root hub to a
module. `Manager::mount_path` removes the route from the host to the base's
first hub, using the base controller's own path `1.4.4` as the anchor, which
is how the vendor application's profile numbers its modules too
(experiment 0007).

`cmh list` on the test machine (Windows 11):

| Module | USB port chain | Mount path | Measured in 0005 |
|---|---|---|---|
| Base | 1.4.3.1.4.4 | 1.4.4 | 1.4.4 |
| 15 LCD Keys | 1.4.3.2.2 | 2.2 | 2.2 |
| 3 Knobs | 1.4.3.1.3.4 | 1.3.4 | 1.3.4 |
| 2 Rollers | 1.4.3.1.1.4 | 1.1.4 | 1.1.4 |
| Dial Display | 1.4.3.1.4.3 | 1.4.3 | 1.4.3 |

Conclusions:

- Reading the port chain on Windows (device interface -> device node ->
  parent -> location path): CONFIRMED.
- Mount path relative to the base: CONFIRMED for one base. That the base
  controller always sits at `1.4.4` is an assumption drawn from this single
  unit.
- The Linux implementation (sysfs path of the hidraw node) is covered by a
  parser test with a constructed path only. UNKNOWN on real hardware.

Still missing for "physical position": the table from mount path to grid
cells, to be built by moving modules around.

## Linux build

Arch Linux under WSL2 (kernel 6.18), rustc 1.98.0, libudev 261:

- `cargo test`: all unit and doc tests pass.
- `cargo clippy --all-targets`: no warnings.
- `cmh list` starts and reports no modules (WSL has no access to the USB
  devices); `cmh replay` decodes fixtures.

Conclusion: the workspace builds and its hardware-independent parts work on
Linux. Device I/O, permissions and hot-plug on Linux remain UNKNOWN.
