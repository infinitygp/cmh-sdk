# cmh-sdk

Independent, cross-platform Rust SDK for the Cooler Master MasterHUB modular
control deck (Windows and Linux, HID via `hidapi`). Work in progress: the
protocol is being derived from controlled USB observations of real hardware.

- `crates/cmh-sdk`: library (transport, protocol, devices)
- `crates/cmh-cli`: `cmh` diagnostic tool
- `docs/`: [inventory](docs/inventory.md), [feature matrix](docs/feature-matrix.md),
  [capture workflow](docs/capture-workflow.md), experiment notes
- `tools/capture`: USBPcap helper scripts

```sh
cargo run -p cmh-cli -- list --descriptors
cargo run -p cmh-cli -- sniff --seconds 10
```

Not affiliated with Cooler Master. Only behaviour confirmed on hardware is
implemented; firmware update, recovery, factory reset and serial rewrite
commands are intentionally out of scope.
