# HyperScanEmu

HyperScanEmu is an evidence-driven, clean-room Rust emulator for the Mattel
HyperScan. It is currently an early research implementation and cannot boot games
yet.

## Intended target

- Strategy: `lle`
- Required firmware: 32 KiB SPG290 internal ROM and 1 MiB HyperScan BIOS
- Planned game media: raw ISO/BIN/CUE images, with strict format validation
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

The loader already rejects truncated and uniform blank firmware dumps. The core
still returns `ExecutionNotImplemented` until the first S+Core execution slice is
connected to the checked memory bus.

Run the current headless probe with:

```bash
cargo run -p hyperscanemu -- <internal-rom.bin> <bios.bin>
```

Firmware and game media are not included. Obtain and dump them legally from
hardware and media you own.

Do not commit or distribute copyrighted ROMs, firmware, games, extracted assets, logs, or local research material.
