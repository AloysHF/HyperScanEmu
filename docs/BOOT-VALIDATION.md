# Boot validation

Firmware and game data are intentionally excluded from the repository. With a
legally dumped 32 KiB internal ROM, 1 MiB BIOS and supported retail image, run:

```text
hyperscanemu trace <internal-rom.bin> <bios.bin> 15000000
hyperscanemu run <internal-rom.bin> <bios.bin> <media> 300 frame.ppm
```

The current reference run completes 15 million instructions without an unknown
MMIO or unsupported-instruction stop. A 300-frame media run reaches a 640x480
HyperScan startup image and produces a stable non-black framebuffer fingerprint.
At 900 frames the BIOS initializes and verifies the CD DSP firmware, reads the
TOC and identifies the disc. Deterministic analog feedback completes the focus
and tracking calibration stages without a motor timeout. Correct single-track
TOC descriptors let the firmware leave its lead-in track-jump loop, read raw
sectors and transfer control to the retail program. A 1600-frame reference run
shows the game's loading screen; by 3600 frames its opening animation is visible,
and distinct animation frames continue through 5000 frames.
This establishes boot progress, not gameplay compatibility.

A second locally supplied retail image reaches its own 320x240 loading screen
by 1800 frames, exercising a different video mode and the same CD boot path.

UART output and the final PC, PPU/TVE controls, layer state, CD command origins
and servo position are printed to make regressions diagnosable without a
graphical frontend. The optional PPM output is the visual oracle; fingerprints
alone do not establish rendering correctness.
