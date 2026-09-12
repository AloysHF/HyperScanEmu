# Frontends

## Command line

The `hyperscanemu` binary supports three deterministic workflows:

```text
hyperscanemu inspect-disc <media.bin|media.cue|media.zip>
hyperscanemu trace <internal-rom.bin> <bios.bin> [steps]
hyperscanemu run <internal-rom.bin> <bios.bin> <media> [frames] [frame.ppm]
```

`run` executes complete video frames and prints the final geometry, framebuffer
fingerprint, program counter and key video state. If the final path is supplied,
it also writes a binary PPM screenshot. This is intended for headless regressions
while the interactive standalone backend is still under development.

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
