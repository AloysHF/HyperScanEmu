use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Parser;
use hyperscanemu_core::{CdCommandKind, Emulator, Firmware, StepOutcome};
use hyperscanemu_media::load_disc;

mod audio_output;
mod cli;
mod input;
mod standalone;

use cli::{Cli, PlayOptions};

fn main() -> Result<()> {
    let args = Cli::parse();

    if args.inspect {
        let media = args
            .media
            .as_deref()
            .context("--inspect requires a media path (BIN, CUE, or ZIP)")?;
        inspect_disc(media)?;
        return Ok(());
    }

    if let Some(steps) = args.trace {
        let internal_rom = args
            .internal_rom
            .as_deref()
            .context("--trace requires --internal-rom <PATH>")?;
        let bios = args
            .bios
            .as_deref()
            .context("--trace requires --bios <PATH>")?;
        trace(internal_rom, bios, steps)?;
        return Ok(());
    }

    let media = args
        .media
        .as_deref()
        .context("provide a media path (BIN, CUE, or ZIP)")?;
    let internal_rom = args
        .internal_rom
        .as_deref()
        .context("provide --internal-rom <PATH> (32 KiB SPG290 internal ROM)")?;
    let bios = args
        .bios
        .as_deref()
        .context("provide --bios <PATH> (1 MiB HyperScan BIOS)")?;

    if args.headless || args.screenshot.is_some() || args.output.is_some() {
        let frames = args
            .screenshot
            .as_ref()
            .map_or(args.frames, |_| args.screenshot_frames);
        run_headless(
            internal_rom,
            bios,
            media,
            frames,
            args.output.as_deref(),
            args.screenshot.as_deref(),
        )?;
        return Ok(());
    }

    let options = PlayOptions {
        internal_rom: internal_rom.to_path_buf(),
        bios: bios.to_path_buf(),
        media: media.to_path_buf(),
        scale: args.scale,
        fullscreen: args.fullscreen,
        volume: args.volume,
        headless: false,
        frames: args.frames,
        screenshot: args.screenshot.clone(),
        screenshot_frames: args.screenshot_frames,
        output: args.output.clone(),
    };
    standalone::run(options)
}

fn inspect_disc(media: &Path) -> Result<()> {
    let disc = load_disc(media)?;
    println!(
        "validated MODE1/2352 image: {} sectors, fingerprint {:016x}",
        disc.sector_count(),
        disc.fingerprint()
    );
    Ok(())
}

fn run_headless(
    internal_rom: &Path,
    bios: &Path,
    media: &Path,
    frames: u64,
    output: Option<&Path>,
    screenshot: Option<&Path>,
) -> Result<()> {
    let firmware = load_firmware(internal_rom, bios)?;
    let disc = load_disc(media)?;
    let mut emulator = Emulator::new(firmware);
    emulator.attach_disc(disc);
    let trace_interval = std::env::var("HYPERSCANEMU_FRAME_TRACE_INTERVAL")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|interval| *interval != 0);
    for frame in 1..=frames {
        emulator.run_frame()?;
        if trace_interval.is_some_and(|interval| frame % interval == 0) {
            let cd = emulator.cd_servo_state();
            eprintln!(
                "frame {frame}: PC {:08x}, CD sector {}, seek {}, skip {}, speed {}x, found {}",
                emulator.cpu().pc(),
                cd.current_sector,
                cd.seek_lba,
                cd.skip,
                cd.speed,
                cd.frame_found,
            );
        }
    }
    print_uart(&mut emulator);
    print_run_summary(&emulator, frames)?;
    if let Some(path) = output {
        let (width, height) = emulator.display_size();
        fs::write(path, encode_ppm(emulator.framebuffer(), width, height))
            .with_context(|| format!("failed to write {}", path.display()))?;
        println!("wrote frame to {}", path.display());
    }
    if let Some(path) = screenshot {
        standalone::save_png(path, &emulator)?;
        println!("wrote screenshot to {}", path.display());
    }
    Ok(())
}

fn trace(internal_rom: &Path, bios: &Path, steps: u64) -> Result<()> {
    let firmware = load_firmware(internal_rom, bios)?;
    let mut emulator = Emulator::new(firmware);
    let start_pc = emulator.cpu().pc();
    let start_cycles = emulator.cpu().cycles();
    let mut exceptions = 0;
    for executed in 0..steps {
        match emulator.step() {
            Ok(StepOutcome::Exception { .. }) => exceptions += 1,
            Ok(_) => {}
            Err(error) => {
                print_uart(&mut emulator);
                let registers = (0..32)
                    .filter_map(|index| {
                        let value = emulator.cpu().register(index)?;
                        (value != 0).then_some(format!("r{index}={value:08x}"))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                if !registers.is_empty() {
                    eprintln!("registers: {registers}");
                }
                bail!(
                    "trace stopped after {executed} instructions at PC {:08x}: {error}",
                    emulator.cpu().pc()
                );
            }
        }
    }
    print_uart(&mut emulator);
    println!(
        "executed {} instructions / {} estimated cycles: PC {:08x} -> {:08x}, {} exceptions, boot {:?}",
        steps,
        emulator.cpu().cycles().wrapping_sub(start_cycles),
        start_pc,
        emulator.cpu().pc(),
        exceptions,
        emulator.bus().boot_source()
    );
    Ok(())
}

fn load_firmware(internal_rom: &Path, bios: &Path) -> Result<Firmware> {
    let internal = fs::read(internal_rom)
        .with_context(|| format!("failed to read {}", internal_rom.display()))?;
    let bios_data = fs::read(bios).with_context(|| format!("failed to read {}", bios.display()))?;
    Firmware::from_parts(&internal, &bios_data).map_err(Into::into)
}

fn print_run_summary(emulator: &Emulator, frames: u64) -> Result<()> {
    let (width, height) = emulator.display_size();
    println!(
        "completed {} frame(s): {}x{}, frame fingerprint {:016x}, PC {:08x}, PPU {:08x}, TVE {:08x}",
        frames,
        width,
        height,
        frame_fingerprint(emulator.framebuffer()),
        emulator.cpu().pc(),
        emulator.bus().read_u32(0x0801_0000)?,
        emulator.bus().read_u32(0x0803_0000)?
    );
    println!(
        "video state: sprites {:08x}, layers [{:08x}, {:08x}, {:08x}], TV buffer {}",
        emulator.bus().read_u32(0x0801_0004)?,
        emulator.bus().read_u32(0x0801_002c)?,
        emulator.bus().read_u32(0x0801_0048)?,
        emulator.bus().read_u32(0x0801_0064)?,
        emulator.bus().read_u32(0x0809_0020)?
    );
    print_cd_trace(emulator);
    Ok(())
}

fn print_uart(emulator: &mut Emulator) {
    let output = emulator.drain_uart_output();
    if !output.is_empty() {
        eprintln!("UART: {}", String::from_utf8_lossy(&output));
    }
}

fn print_cd_trace(emulator: &Emulator) {
    let trace = emulator.cd_command_trace();
    if trace.is_empty() {
        return;
    }
    let include_subcode = std::env::var_os("HYPERSCANEMU_TRACE_CD_SUBCODE").is_some();
    let control = trace
        .iter()
        .filter(|event| include_subcode || !(0x340..=0x34c).contains(&event.address))
        .collect::<Vec<_>>();
    let start = control.len().saturating_sub(32);
    let events = control[start..]
        .iter()
        .map(|event| {
            let kind = match event.kind {
                CdCommandKind::Read => 'R',
                CdCommandKind::Write => 'W',
            };
            format!(
                "{kind}{:03x}={:02x}#{}@{:08x}/{:08x}",
                event.address, event.data, event.sector, event.pc, event.link
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    println!("CD trace: {events}");
    let state = emulator.cd_servo_state();
    println!(
        "CD state: sector {}, seek LBA {}, skip {}, speed {}x, frame found {}",
        state.current_sector, state.seek_lba, state.skip, state.speed, state.frame_found
    );
}

fn encode_ppm(frame: &[u32], width: usize, height: usize) -> Vec<u8> {
    let mut output = format!("P6\n{width} {height}\n255\n").into_bytes();
    output.reserve(frame.len() * 3);
    for pixel in frame {
        output.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    output
}

fn frame_fingerprint(frame: &[u32]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x100_0000_01b3;
    frame.iter().fold(OFFSET_BASIS, |hash, pixel| {
        pixel.to_le_bytes().iter().fold(hash, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_fingerprint_is_deterministic_and_content_sensitive() {
        assert_eq!(frame_fingerprint(&[1, 2]), frame_fingerprint(&[1, 2]));
        assert_ne!(frame_fingerprint(&[1, 2]), frame_fingerprint(&[2, 1]));
    }

    #[test]
    fn ppm_encoder_writes_rgb_pixels() {
        assert_eq!(
            encode_ppm(&[0xff12_3456], 1, 1),
            b"P6\n1 1\n255\n\x12\x34\x56"
        );
    }
}
