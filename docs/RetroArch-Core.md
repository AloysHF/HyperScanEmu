# RetroArch Core

This guide covers installing and running the HyperScanEmu libretro core,
loading content, firmware placement, supported features, and controls.

## Installation

### Manual installation

Build the libretro core:

```bash
cargo build -p hyperscanemu-libretro --release
```

Cargo names the cdylib after its lib target (`hyperscanemu`). Rename the
produced library before placing it into RetroArch's `cores/` directory:

| Platform | Build output | Rename to |
|---|---|---|
| Windows | `hyperscanemu.dll` | `hyperscanemu_libretro.dll` |
| Linux | `libhyperscanemu.so` | `hyperscanemu_libretro.so` |
| macOS | `libhyperscanemu.dylib` | `hyperscanemu_libretro.dylib` |

## Firmware

Place legally dumped firmware in the frontend **system** directory:

```text
spg290.bin      32768 bytes
hyperscan.bin  1048576 bytes
```

## Loading content

1. Open RetroArch and select the HyperScanEmu core.
2. Select **Load Content**.
3. Choose a BIN, single-track CUE, or ZIP disc package.

See [Game File Formats](Game-File-Formats.md) for media validation rules.

## Supported features

- Video output using dynamic-size XRGB8888 frames
- Two joypads plus left analog sticks
- DAC FIFO audio callbacks (hardware synthesizer remains silent)
- Full-path BIN/CUE/ZIP media loading

## Current limitations

- No save-state serialization or exposed memory blocks are reported yet.
- Card image selection and atomic card saving remain frontend work.
- Retail title/menu and gameplay have not been validated under RetroArch.

## Diagnostic frontends

For headless regression and firmware tracing, prefer the standalone diagnostic
commands described in [Standalone Emulator](Standalone-Emulator.md) instead of
the libretro path.
