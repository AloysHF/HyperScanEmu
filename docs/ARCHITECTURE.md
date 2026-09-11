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
- `hyperscanemu` owns host paths, CLI, future window/audio devices, and headless driving.
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

`DiscImage` validates raw sectors and exposes only their 2048-byte user-data
payload. Future CD-servo emulation will consume that interface, so sector framing,
CUE path resolution and host files do not leak into the device model.
