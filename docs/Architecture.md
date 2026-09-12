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

## Implementation notes

| Area | Document |
|---|---|
| S+Core 7 interpreter | [Cpu](Cpu.md) |
| SPG290 MMIO and peripherals | [Mmio](Mmio.md) |
| DAC ring-buffer audio | [Audio](Audio.md) |
| Disc and card formats | [Game File Formats](Game-File-Formats.md) |
| Boot evidence | [Boot Validation](Boot-Validation.md) |
