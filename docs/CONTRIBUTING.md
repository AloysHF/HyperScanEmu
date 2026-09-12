# Contributing

## How to Contribute

1. **Fork** this repository
2. **Create** a feature branch (`git checkout -b feature/your-feature`)
3. **Commit** your changes (`git commit -m 'Add your feature'`)
4. **Push** to the branch (`git push origin feature/your-feature`)
5. **Open** a Pull Request

## Code Style

- Use English for all comments and documentation
- Use `snake_case` for functions and variables
- Use `PascalCase` for types and structs
- Prefer `anyhow::Result` in adapters; structured errors in the core
- Do not use `println!` in library code; CLI adapters may print diagnostics

## Validation before a PR

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Areas That Need Help

- **S+Core 7 CPU** — custom-engine, debug, interrupt, and cycle-accurate timing
- **SPG290 peripherals** — SPU synthesis, timer modes, IRQ priority conformance
- **CD fidelity** — EDC/ECC, multi-track CUE, seek timing, subcode CRC
- **Game compatibility testing** — report boot progress with frame fingerprints or screenshots
- **libretro integration** — save states, memory maps, card persistence
- **Documentation** — improve docs and code comments
- **Bug reports** — include firmware/media hashes and a deterministic `run`/`trace` command when possible

## Getting Started

Read [Architecture](Architecture.md) first, then pick a device or CPU gap from
[Boot Validation](Boot-Validation.md). Firmware and media are not distributed;
use legally obtained dumps locally and keep them out of the repository.
