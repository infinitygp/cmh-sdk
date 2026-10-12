# cmh-sdk

Independent, cross-platform Rust SDK for the Cooler Master MasterHUB modular
control deck (Windows and Linux, HID via `hidapi`). Work in progress: the
protocol is being derived from controlled USB observations of real hardware.
Verified on Windows; on Linux it builds and its tests pass, device access is
untested.

- `crates/cmh-sdk`: library (transport, protocol, devices, input, module, manager)
- `crates/cmh-cli`: `cmh` diagnostic tool
- `docs/`: [inventory](docs/inventory.md), [feature matrix](docs/feature-matrix.md),
  [capture workflow](docs/capture-workflow.md), experiment notes
- `tools/capture`: USBPcap helper scripts
- `fixtures/`: recorded reports used by the tests

```sh
cargo run -p cmh-cli -- list
cargo run -p cmh-cli -- info
cargo run -p cmh-cli -- monitor --raw
cargo run -p cmh-cli -- replay fixtures/inputs/all-controls.txt
```

Status and gaps: [docs/feature-matrix.md](docs/feature-matrix.md). Protocol
reference: [docs/protocol](docs/protocol/README.md).

Not affiliated with Cooler Master. Only behaviour confirmed on hardware is
implemented; firmware update, recovery, factory reset and serial rewrite
commands are intentionally out of scope.
