# Standalone Emulator

This guide covers building and running the standalone `hyperscan-emu` binary,
loading firmware and media, keyboard controls, headless diagnostics, and all
command-line options.

Firmware and game media are not included. Obtain and dump them legally from
hardware and media you own.

## Build and launch

```bash
cargo build -p hyperscanemu --release
target/release/hyperscan-emu play spg290.bin hyperscan.bin game.cue
```

BIN, single-track CUE, and ZIP disc packages are accepted. The firmware loader
requires a 32 KiB SPG290 internal ROM and a 1 MiB HyperScan BIOS.

## Synopsis

```text
hyperscan-emu play [OPTIONS] <INTERNAL_ROM> <BIOS> <MEDIA>
```

## Options

| Option | Default | Description |
|---|---:|---|
| `-s, --scale <1-8>` | `1` | Initial integer window scale. |
| `-f, --fullscreen` | off | Open a borderless desktop-sized window. |
| `-v, --volume <0-100>` | `100` | Host audio volume; zero disables audio. |
| `--headless` | off | Run without a window or audio device. |
| `--frames <N>` | `60` | Frame count used with `--headless`. |
| `-S, --screenshot <PATH>` | none | Run headlessly, save a PNG, and exit. |
| `--screenshot-frames <N>` | `300` | Frame count used before a PNG capture. |

The window can be resized at runtime. The source framebuffer retains its 4:3
aspect ratio and is centered by the host window system. The title bar reports
measured frontend FPS once per second.

## Runtime controls

| Key | Frontend action |
|---|---|
| `P` | Pause or resume emulation. |
| `Ctrl+R` | Cold-reset the emulated system and clear queued audio. |
| `F12` | Save the current native-resolution frame as PNG. |
| `Escape` | Exit. |

### Player 1

| Keys | Controller input |
|---|---|
| Arrow keys | Analog stick |
| `Z`, `X`, `A`, `S` | Green, Red, Blue, Yellow |
| `Q`, `W` | Left, Right shoulder |
| `E`, `R` | Left, Right trigger |
| `Enter`, `Backspace` | Start, Select |

### Player 2

| Keys | Controller input |
|---|---|
| `I`, `J`, `K`, `L` | Analog stick |
| `F`, `G`, `T`, `Y` | Green, Red, Blue, Yellow |
| `3`, `4` | Left, Right shoulder |
| `5`, `6` | Left, Right trigger |
| `1`, `2` | Start, Select |

## Headless PNG capture

```bash
hyperscan-emu play spg290.bin hyperscan.bin game.zip \
  --screenshot startup.png --screenshot-frames 300
```

This uses the same firmware, media, frame execution, and renderer lifecycle as
the interactive frontend, but does not open a window or audio device. The PNG
contains the native framebuffer without host scaling or letterboxing.

## Diagnostic CLI

The same binary supports deterministic workflows that stay free of window, input,
and audio-device dependencies:

```text
hyperscanemu inspect-disc <media.bin|media.cue|media.zip>
hyperscanemu trace <internal-rom.bin> <bios.bin> [steps]
hyperscanemu run <internal-rom.bin> <bios.bin> <media> [frames] [frame.ppm]
```

- `inspect-disc` validates BIN/CUE/ZIP structure without firmware.
- `trace` executes a bounded instruction count and reports the final PC or the
  structured stop reason (unknown instruction or MMIO).
- `run` executes complete video frames and prints the final geometry, framebuffer
  fingerprint, program counter and key video state. If the final path is supplied,
  it also writes a binary PPM screenshot.

Optional environment variables:

| Variable | Effect |
|---|---|
| `HYPERSCANEMU_FRAME_TRACE_INTERVAL` | Print frame state every N frames during `run`. |
| `HYPERSCANEMU_TRACE_CD_SUBCODE` | Include CD subcode details in the CD command trace. |

## Current compatibility

The standalone frontend and shared core have been validated through a retail
loading screen and opening animation. Title/menu and gameplay compatibility are
not yet claimed; CD seek/subcode fidelity and additional CPU/device behavior
remain under development. Audio transport is functional for the DAC ring-buffer
path; the hardware synthesizer is not implemented.

See [Boot Validation](Boot-Validation.md) for the current reference runs.
