# HyperScanEmu

HyperScanEmu is an evidence-driven, clean-room Rust emulator for the Mattel
HyperScan. It is currently an early research implementation and cannot boot games
yet.

## Intended target

- Strategy: `lle`
- Required firmware: 32 KiB SPG290 internal ROM and 1 MiB HyperScan BIOS
- Game media: single-track CUE + raw `MODE1/2352` BIN with an ISO/UDF bridge
- Display geometry: 640×480 output with hardware-controlled lower-resolution modes

These values are hypotheses until supported by evidence in `tmp/PROJECT-STATUS.md`.

## Structure

```text
crates/
├── hyperscanemu-core/      # Platform-independent emulator state and execution
├── hyperscanemu/           # Headless/standalone host adapter
└── hyperscanemu-libretro/  # libretro ABI boundary
```

## Build

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The core includes a checked memory bus, an initial S+Core interpreter, timers and
the firmware-visible dual-controller I²C protocol. Frame rendering remains
`ExecutionNotImplemented` until the display pipeline is connected.

Run a bounded, deterministic firmware trace with:

```bash
cargo run -p hyperscanemu -- trace <internal-rom.bin> <bios.bin> [steps]
```

Firmware and game media are not included. Obtain and dump them legally from
hardware and media you own.

Validate a raw disc track without firmware:

```bash
cargo run -p hyperscanemu -- inspect-disc <track.bin>
```

Do not commit or distribute copyrighted ROMs, firmware, games, extracted assets, logs, or local research material.
