# Standalone Emulator

This guide covers building and running the standalone `hyperscan-emu` binary,
loading firmware and media, keyboard controls, headless diagnostics, and all
command-line options.

Firmware and game media are not included. Obtain and dump them legally from
hardware and media you own.

## Build and launch

```bash
cargo build -p hyperscanemu --release
target/release/hyperscan-emu game.cue --internal-rom spg290.bin --bios hyperscan.bin
```

BIN, single-track CUE, and ZIP disc packages are accepted. The firmware loader
requires a 32 KiB SPG290 internal ROM and a 1 MiB HyperScan BIOS.

## Synopsis

```text
hyperscan-emu [OPTIONS] [MEDIA]
```

There are no subcommands. With a media path, firmware flags, and no
`--headless`/`--screenshot`/`--output`, the windowed emulator opens. This
matches the sibling desktop frontends (Playdia, Dingoo, SPMP, Native32).

Examples:

```bash
# Windowed play
hyperscan-emu game.zip -r spg290.bin -b hyperscan.bin
hyperscan-emu game.cue --internal-rom spg290.bin --bios hyperscan.bin --scale 2 --fullscreen

# Validate media only
hyperscan-emu game.zip --inspect

# Instruction trace (firmware only)
hyperscan-emu -r spg290.bin -b hyperscan.bin --trace 200000

# Headless run + PPM
hyperscan-emu game.zip -r spg290.bin -b hyperscan.bin --headless --frames 300 --output frame.ppm

# Headless PNG screenshot
hyperscan-emu game.zip -r spg290.bin -b hyperscan.bin -S startup.png --screenshot-frames 300
```

## Options

| Option | Default | Description |
|---|---:|---|
| `<MEDIA>` | — | Path to BIN, CUE, or ZIP game media. |
| `-r, --internal-rom <PATH>` | — | Path to the 32 KiB SPG290 internal ROM. |
| `-b, --bios <PATH>` | — | Path to the 1 MiB HyperScan BIOS. |
| `-s, --scale <1-8>` | `1` | Initial integer window scale. |
| `-f, --fullscreen` | off | Open a borderless desktop-sized window. |
| `-v, --volume <0-100>` | `100` | Host audio volume; zero disables audio. |
| `--inspect` | off | Validate the media package and exit (no firmware needed). |
| `--trace <STEPS>` | off | Execute a bounded instruction trace (no media needed). |
| `--headless` | off | Run without a window or audio device. |
| `--frames <N>` | `60` | Frame count used with `--headless`. |
| `-S, --screenshot <PATH>` | none | Run headlessly, save a PNG, and exit. |
| `--screenshot-frames <N>` | `300` | Frame count used before a PNG capture. |
| `--output <PATH>` | none | Write the final headless frame as a binary PPM. |

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
hyperscan-emu game.zip -r spg290.bin -b hyperscan.bin \
  --screenshot startup.png --screenshot-frames 300
```

This uses the same firmware, media, frame execution, and renderer lifecycle as
the interactive frontend, but does not open a window or audio device. The PNG
contains the native framebuffer without host scaling or letterboxing.

## Diagnostic CLI

The same binary supports deterministic workflows that stay free of window, input,
and audio-device dependencies:

```text
hyperscan-emu <media.bin|media.cue|media.zip> --inspect
hyperscan-emu -r <internal-rom.bin> -b <bios.bin> --trace <steps>
hyperscan-emu <media> -r <internal-rom.bin> -b <bios.bin> \
  --headless --frames <N> [--output frame.ppm] [-S frame.png]
```

- `--inspect` validates BIN/CUE/ZIP structure without firmware.
- `--trace` executes a bounded instruction count and reports the final PC or the
  structured stop reason (unknown instruction or MMIO).
- Headless mode executes complete video frames and prints the final geometry,
  framebuffer fingerprint, program counter, UART output, and key video state.
  `--output` writes a binary PPM; `-S` writes a PNG.

Optional environment variables:

| Variable | Effect |
|---|---|
| `HYPERSCANEMU_FRAME_TRACE_INTERVAL` | Print frame state every N frames during headless runs. |
| `HYPERSCANEMU_TRACE_CD_SUBCODE` | Include CD subcode details in the CD command trace. |

## Current compatibility

The standalone frontend and shared core have been validated through a retail
game loading screen. Opening animation, title/menu, and gameplay compatibility
are not yet claimed; CD seek/subcode fidelity and additional CPU/device behavior
remain under development. Audio transport is functional for the DAC ring-buffer
path; the hardware synthesizer is not implemented.

See [Game Compatibility](Game-Compatibility.md) for the title progress matrix
and recorded reference runs.
