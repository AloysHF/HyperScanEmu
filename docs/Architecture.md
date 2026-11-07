# Architecture

The project uses a platform-independent core with thin host adapters.

```text
validated content -> core loader -> execution/memory/services -> frame/audio/state
                                      ^
                                      |
                  scripted input and injected time/storage

standalone adapter -------------------+
libretro adapter ---------------------+
```

## Current boundary

- `hyperscanemu-core` owns target behavior and deterministic state.
- `hyperscanemu-media` owns bounded host I/O plus strict BIN/CUE/ZIP resolution.
- `hyperscanemu` owns host paths, CLI, window/audio devices, and headless driving.
- `hyperscanemu-libretro` owns only the C ABI and frontend translation.

The selected strategy is firmware LLE. The initial device graph is:

```text
S+Core 7 -> checked SPG290 bus -> DRAM / SRAM / internal ROM / BIOS
                              -> IRQ / timer / I2C / GPIO / CD
                              -> PPU / TVE / SPU
```

The core owns all guest-visible timing and state. Optical media, firmware paths,
windowing, host audio and physical controller mappings stay in adapters. Unknown
instructions and MMIO accesses stop execution with structured diagnostics; they
are never silently treated as successful operations.

`DiscImage` validates raw sectors and exposes their raw framing plus 2048-byte
user payload. The CD servo consumes that interface for DMA, while CUE path
resolution, ZIP decompression and host files remain outside the device model.

## Boot modes

The machine boots one of two ways:

- **Firmware LLE (default).** Both firmware ROMs are required; the S+Core 7
  interpreter executes the real BIOS from reset at `0x9f000000`. The BIOS
  initializes the peripherals, reads the disc TOC, loads `HYPER.EXE` flat to
  `0xa00901fc` and enters it at `0xa0091000`.
- **HLE boot (`--hle-boot`, or automatically when the firmware files are
  missing).** The core reproduces the measured BIOS-to-game handoff state
  without executing firmware: an ISO 9660 walk of the disc locates
  `HYPER.EXE`, loads it flat to `0xa00901fc`, and enters `0xa0091000` with
  the stack pointer `0x20000000` and the measured control-register state
  (vector base `0xa0000000`, interrupts enabled).

The handoff contract was measured with the `HYPERSCANEMU_TRACE_BIOS_FETCH`
instrumentation. Across retail titles the games never fetch from either
firmware ROM window after the handoff; they do, however, call firmware code
that the BIOS leaves resident in guest DRAM (an interrupt dispatcher below
`0x00010000` and a CD service around `0x00028000`-`0x00034000`). Rebuilding
those resident services natively is required before the HLE path can play
retail titles end to end; until then HLE boot is the measurement baseline and
the firmware stays the recommended way to play.

Setting `HYPERSCANEMU_TRACE_BIOS_FETCH=1` prints the handoff snapshot
(registers, control registers, MMIO state, DRAM occupancy) and an end-of-run
firmware-activity summary; `HYPERSCANEMU_HANDOFF_DRAM_DUMP=<path>` writes the
DRAM image captured at the handoff for offline analysis.

## Implementation notes

| Area | Document |
|---|---|
| S+Core 7 CPU, SPG290 MMIO, DAC audio | [Hardware](Hardware.md) |
| Disc and card formats | [Game File Formats](Game-File-Formats.md) |
| Title progress and boot milestones | [Game Compatibility](Game-Compatibility.md) |
| Standalone CLI and controls | [Standalone Emulator](Standalone-Emulator.md) |
| RetroArch install and features | [RetroArch Core](RetroArch-Core.md) |
| Mobile and TV libretro targets | [Android](Android-Libretro-Core.md), [iOS](iOS-Libretro-Core.md) |
| Release checklist | [Release Process](Release-Process.md) |
| Contributor workflow | [Contributing](CONTRIBUTING.md) |
