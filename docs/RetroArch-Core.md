# RetroArch Core

This guide covers installing and running the HyperScanEmu libretro core,
loading content, firmware placement, supported features, and controls.

## Supported platforms

| Platform | Architecture | Standalone | Libretro |
|---|---|---|---|
| Windows | x86_64 | ✅ | ✅ |
| Windows | i686 | — | ✅ |
| macOS | x86_64, aarch64 | ✅ | ✅ |
| Linux | x86_64, aarch64 | ✅ | ✅ |
| Linux | i686 | — | ✅ |
| Android | arm64-v8a, armeabi-v7a, x86, x86_64 | — | ✅ |
| iOS | arm64 + x86_64 universal, arm64 simulator | — | ✅ |
| webOS | armv7 | — | ✅ |

> Android, iOS, and webOS are supported through the libretro core only. See
> [Android](Android-Libretro-Core.md) and [iOS](iOS-Libretro-Core.md) for
> platform-specific install steps.

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

Also copy `crates/hyperscanemu-libretro/hyperscanemu_libretro.info` into
RetroArch's `info/` directory so the frontend can show core metadata.

Release archives already contain the renamed core and `.info` file.

### webOS

Download `hyperscan-emu-webos-libretro.tar.gz` from the
[Releases](https://github.com/AloysHF/HyperScanEmu/releases) page, then place
`hyperscanemu_libretro.so` and `hyperscanemu_libretro.info` into the webOS
RetroArch (or compatible) core/info locations used on the device.

## Firmware

Place legally dumped firmware in the frontend **system** directory:

```text
spg290.bin      32768 bytes
hyperscan.bin  1048576 bytes
```

See [Game File Formats](Game-File-Formats.md) for sizes and roles.

## Loading content

1. Open RetroArch and select the HyperScanEmu core.
2. Select **Load Content**.
3. Choose a BIN, single-track CUE, or ZIP disc package.

Media validation rules are documented in [Game File Formats](Game-File-Formats.md).
Validate packages without firmware first:

```bash
cargo run -p hyperscanemu -- inspect-disc <media.bin|media.cue|media.zip>
```

## Supported features

- Video output using dynamic-size XRGB8888 frames
- Two joypads plus left analog sticks
- DAC FIFO audio callbacks (hardware synthesizer remains silent)
- Full-path BIN/CUE/ZIP media loading
- Required firmware discovery from the frontend system directory

## Current limitations

- No save-state serialization or exposed memory blocks are reported yet.
- Card image selection and atomic card saving remain frontend work.
- Retail title/menu and gameplay have not been validated under RetroArch.
- The core is marked experimental in `hyperscanemu_libretro.info`.

## Diagnostic frontends

For headless regression and firmware tracing, prefer the standalone diagnostic
commands described in [Standalone Emulator](Standalone-Emulator.md) instead of
the libretro path. Boot and compatibility claims live in
[Game Compatibility](Game-Compatibility.md).
