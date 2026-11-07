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
that the BIOS leaves resident in guest DRAM. The HLE path replaces those
resident services natively (`resident_services.rs`):

- interrupt dispatch: the vector table at `0x200 + source * 4` and the
  42-entry source jump table are reproduced natively, calling the
  game-registered handlers with the dispatcher's conventions and performing
  the VBlank continuation (frame tick, PPU acknowledge) and the timer ISR
  side effects (acknowledge, tick counter);
- file API: open/read/close over ISO 9660 extents plus the filesystem
  status, mode and query services;
- helper services: frame pacing, DMA setup, memory-control unlock,
  critical sections and the OS version query;
- boot state: the measured handoff MMIO register file, write-only timer
  gates, and the post-identification CD servo state.

Retail titles additionally depend on deeper firmware behavior that is not
reproduced yet, and each stops with a structured diagnostic naming the
missing piece: timing-sensitive loader state (Spider-Man), the deferred
asset pump and an unmodeled SD-controller probe (X-Men, Marvel Heroes),
overlay code loading (IWL) and a parity-format instruction decode (Ben 10).
The dumped firmware remains the recommended way to play.

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
