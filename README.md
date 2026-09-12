# HyperScanEmu — A Mattel HyperScan emulator written in Rust

HyperScanEmu is an evidence-driven, clean-room Rust emulator for the Mattel
HyperScan. It boots the retail BIOS and reaches a retail game's loading screen
and opening animation with real media attached. Retail gameplay is not yet
validated.

The HyperScan is a 2006 Mattel home console built around the Sunplus SPG290
SoC (S+Core 7 CPU). Games ship on CD with optional RFID save cards. The
selected strategy is low-level emulation of the firmware-visible hardware.

## Features

- **SPG290 LLE core** — checked memory bus, S+Core 7 interpreter, deterministic peripheral scheduling
- **Firmware boot** — 32 KiB internal ROM plus 1 MiB HyperScan BIOS with structured unknown-instruction / MMIO stops
- **Disc packages** — raw `MODE1/2352` BIN, single-track CUE, and ZIP loading with ISO/UDF validation
- **CD servo** — timed sector DMA, analog feedback, and single-track Q subchannel synthesis
- **Display** — TVE direct frames plus PPU bitmap, character, and sprite layers (640×480 output)
- **Input** — dual I²C controllers with buttons, analog axes, and checksums
- **Audio** — 54 MHz DAC ring-buffer DMA (16-bit PCM); hardware synthesizer not yet implemented
- **RFID cards** — 120-byte card protocol on GPIO with dirty tracking
- **Standalone frontend** — resizable window, fullscreen, keyboard gamepads, PNG screenshots
- **libretro core** — RetroArch integration with firmware from the system directory
- **Headless diagnostics** — `trace`, `run`, and `inspect-disc` without window or audio devices

## Usage

### Standalone Mode

```bash
cargo run -p hyperscanemu --release -- play spg290.bin hyperscan.bin game.cue
```

See the [Standalone Emulator](docs/Standalone-Emulator.md) guide for options,
keyboard controls, headless capture, and diagnostic CLI workflows.

### RetroArch Mode

Build the libretro core, rename it to `hyperscanemu_libretro.<ext>`, and place
legally dumped firmware in the frontend system directory:

```text
spg290.bin      32768 bytes
hyperscan.bin  1048576 bytes
```

See the [RetroArch Core](docs/RetroArch-Core.md) guide for installation,
loading content, and current limitations.

### Diagnostic CLI

```bash
cargo run -p hyperscanemu -- inspect-disc <media.bin|media.cue|media.zip>
cargo run -p hyperscanemu -- trace <internal-rom.bin> <bios.bin> [steps]
cargo run -p hyperscanemu -- run <internal-rom.bin> <bios.bin> <media> [frames] [frame.ppm]
```

## Building

Requires [Rust](https://www.rust-lang.org/tools/install) (stable).

### Standalone

```bash
cargo build -p hyperscanemu --release
```

The binary is produced at `target/release/hyperscanemu`.

### Libretro Core

```bash
cargo build -p hyperscanemu-libretro --release
```

Rename the output to `hyperscanemu_libretro.<ext>` before installing it into
RetroArch's `cores/` directory. See [RetroArch Core](docs/RetroArch-Core.md).

## Architecture

```text
crates/
├── hyperscanemu-core/      # Platform-independent emulator state and execution
├── hyperscanemu-media/     # Checked BIN/CUE/ZIP host-media loading
├── hyperscanemu/           # Headless/standalone host adapter
└── hyperscanemu-libretro/  # libretro ABI boundary
```

Details: [Architecture](docs/Architecture.md), [CPU](docs/Cpu.md),
[MMIO](docs/Mmio.md), [Audio](docs/Audio.md),
[Game File Formats](docs/Game-File-Formats.md).

## Testing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

Firmware and game media are not included and cannot run in CI. Local boot
evidence is documented in [Boot Validation](docs/Boot-Validation.md).

Validate media without firmware:

```bash
cargo run -p hyperscanemu -- inspect-disc <media.bin|media.cue|media.zip>
```

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for
code style, validation commands, and areas that need help.

## License

This project is licensed under the BSD 3-Clause License.
