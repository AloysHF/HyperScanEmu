# Frontends

## Standalone desktop

The `play` command runs the shared core in a resizable, aspect-correct desktop
window at 60 frontend frames per second:

```text
hyperscanemu play [OPTIONS] <internal-rom.bin> <bios.bin> <media>
```

It supports borderless fullscreen, volume control, two keyboard controllers,
pause, reset, live FPS in the title bar, and F12 PNG screenshots. Audio is sent
to the default host output device and resampled from the core's 44.1 kHz stereo
stream. Failure to open an audio device is non-fatal. The same command can run
without host devices using `--headless`, or create a deterministic PNG using
`--screenshot`.

See `STANDALONE.md` for the complete option and control reference.

## Diagnostic command line

The `hyperscanemu` binary supports three deterministic workflows:

```text
hyperscanemu inspect-disc <media.bin|media.cue|media.zip>
hyperscanemu trace <internal-rom.bin> <bios.bin> [steps]
hyperscanemu run <internal-rom.bin> <bios.bin> <media> [frames] [frame.ppm]
```

`run` executes complete video frames and prints the final geometry, framebuffer
fingerprint, program counter and key video state. If the final path is supplied,
it also writes a binary PPM screenshot. This path remains intentionally free of
window, input, and audio-device dependencies for regressions.

## libretro

The `hyperscanemu-libretro` crate exports the complete base libretro lifecycle,
loads full-path BIN/CUE/ZIP media, polls two joypads plus left analog sticks, and
submits dynamic-size XRGB8888 frames. Place legally dumped firmware in the
frontend system directory as:

```text
spg290.bin      32768 bytes
hyperscan.bin  1048576 bytes
```

The core currently reports no serialization or exposed memory blocks. Audio
callbacks receive DAC FIFO output; the 24-channel hardware synthesizer remains
silent. Card image selection and atomic card saving also remain frontend work.
