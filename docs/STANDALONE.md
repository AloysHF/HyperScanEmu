# Standalone Emulator

The `hyperscanemu` binary can run the shared emulator core in a native desktop
window on Windows, macOS, and Linux. Firmware and game media are not included;
users must dump them legally from hardware and media they own.

## Build and launch

```bash
cargo build -p hyperscanemu --release
target/release/hyperscanemu play spg290.bin hyperscan.bin game.cue
```

BIN, single-track CUE, and ZIP disc packages are accepted. The firmware loader
requires a 32 KiB SPG290 internal ROM and a 1 MiB HyperScan BIOS.

## Options

```text
hyperscanemu play [OPTIONS] <INTERNAL_ROM> <BIOS> <MEDIA>
```

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
hyperscanemu play spg290.bin hyperscan.bin game.zip \
  --screenshot startup.png --screenshot-frames 300
```

This uses the same firmware, media, frame execution, and renderer lifecycle as
the interactive frontend, but does not open a window or audio device. The PNG
contains the native framebuffer without host scaling or letterboxing.

## Current compatibility

The standalone frontend and shared core have been validated through a retail
loading screen and opening animation. Title/menu and gameplay compatibility are
not yet claimed; CD seek/subcode fidelity and additional CPU/device behavior
remain under development. Audio transport is functional for the DAC ring-buffer
path; the hardware synthesizer is not implemented.
