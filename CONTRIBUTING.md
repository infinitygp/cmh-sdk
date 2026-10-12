# Contributing

This is a one-person project. Issues and pull requests are welcome; answers
may take a while.

## What a change needs

- **An observation behind every protocol fact.** A new or changed behaviour
  comes with a note in `docs/experiments/` (next free number): the host, what
  was done and what was seen. Statements carry a confidence level, CONFIRMED,
  LIKELY or UNKNOWN, as defined in
  [docs/protocol/README.md](docs/protocol/README.md).
- **Your own observations.** Facts come from a device and from traffic on the
  wire, not from decompiled vendor software.
- **Fixtures instead of captures.** Reports recorded from hardware go into
  `fixtures/` in the text format written by `cmh monitor --raw` and
  `tools/capture/export-fixture.ps1`. Raw captures (`*.pcapng`) are not
  committed, and the unit digits of serial numbers are masked as in
  [docs/inventory.md](docs/inventory.md).
- **A closed command set.** The SDK sends only commands that were verified on
  a device. Firmware update, recovery, factory reset and serial rewrite stay
  out. The test `emits_only_the_verified_command_set` pins every report that
  can leave the crate; a new command is added there on purpose.

## Checks

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

The tests need no hardware.

## Commits

One subject line, `type: summary` or `type(scope): summary`, as in the
existing history.

## License

Contributions are dual licensed under MIT and Apache-2.0, see
[README.md](README.md#license).
