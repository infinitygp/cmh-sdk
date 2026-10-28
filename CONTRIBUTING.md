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
  `crates/cmh-sdk/fixtures/` in the text format written by `cmh monitor --raw` and
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
cargo test -p cmh-sdk                          # default features
cargo test -p cmh-sdk --features test-support  # without image
RUSTDOCFLAGS="-D warnings" cargo doc -p cmh-sdk --all-features --no-deps
cargo +1.88 test --workspace --all-features --locked   # minimum Rust version
```

The tests need no hardware. The `checks` workflow runs the same on Linux
and Windows, the minimum Rust version on both, the packaged crate and a
compile-only check on macOS. It is started by hand, which GitHub allows
only once the workflow file is on the default branch: from the Actions tab,
or for another branch with `gh workflow run checks --ref <branch>`.
A change that affects hardware behaviour is checked on hardware as well,
and the experiment note says on which platform.

## Commits

One subject line, `type: summary` or `type(scope): summary`, as in the
existing history.

## Publishing

Only `crates/cmh-sdk` is published; `cmh-cli` keeps `publish = false`.
`cargo package -p cmh-sdk` must pass, and `cargo test --all-features --locked`
must pass in the `.crate` archive unpacked outside the repository, before
the owner runs `cargo publish -p cmh-sdk`. The fixtures the tests read live
in the crate so that the published package tests itself.

## License

Contributions are dual licensed under MIT and Apache-2.0, see
[README.md](README.md#license).
