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
├── hyperscanemu-media/     # Checked BIN/CUE/ZIP host-media loading
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

The core includes a checked memory bus, an initial S+Core interpreter, timed
peripherals, the firmware-visible dual-controller and RFID protocols, CD sector
DMA, DAC ring-buffer audio, and an initial RGB565 direct-framebuffer display
path. PPU text/sprite composition and SPU hardware synthesis remain incomplete.

Run a bounded, deterministic firmware trace with:

```bash
cargo run -p hyperscanemu -- trace <internal-rom.bin> <bios.bin> [steps]
```

Firmware and game media are not included. Obtain and dump them legally from
hardware and media you own.

Validate raw BIN, CUE or packaged ZIP media without firmware:

```bash
cargo run -p hyperscanemu -- inspect-disc <media.bin|media.cue|media.zip>
```

Run a deterministic number of complete frames with validated media:

```bash
cargo run -p hyperscanemu -- run <internal-rom.bin> <bios.bin> <media> [frames]
```

See `docs/FRONTENDS.md` for libretro firmware placement and current frontend
limitations.

Do not commit or distribute copyrighted ROMs, firmware, games, extracted assets, logs, or local research material.
