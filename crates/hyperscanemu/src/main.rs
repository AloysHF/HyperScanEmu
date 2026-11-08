use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use clap::Parser;
use hyperscanemu_core::{CdCommandKind, Emulator, StepOutcome};
use hyperscanemu_media::load_disc;

mod audio_output;
mod cli;
mod input;
mod standalone;

use cli::{Cli, PlayOptions};
use standalone::load_firmware;

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
        trace(&args.system_dir, steps)?;
        return Ok(());
    }

    let media = args
        .media
        .as_deref()
        .context("provide a media path (BIN, CUE, or ZIP)")?;

    if args.headless || args.screenshot.is_some() || args.output.is_some() {
        let frames = args
            .screenshot
            .as_ref()
            .map_or(args.frames, |_| args.screenshot_frames);
        run_headless(
            &args.system_dir,
            media,
            args.hle_boot,
            frames,
            args.output.as_deref(),
            args.screenshot.as_deref(),
        )?;
        return Ok(());
    }

    let options = PlayOptions {
        system_dir: args.system_dir.clone(),
        media: media.to_path_buf(),
        hle_boot: args.hle_boot,
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
    system_dir: &Path,
    media: &Path,
    force_hle: bool,
    frames: u64,
    output: Option<&Path>,
    screenshot: Option<&Path>,
) -> Result<()> {
    let disc = load_disc(media)?;
    let mut emulator = match load_firmware(system_dir, force_hle)? {
        Some(firmware) => Emulator::new(firmware),
        None => Emulator::new_hle(),
    };
    emulator.attach_disc(disc);
    if emulator.boot_mode() == hyperscanemu_core::BootMode::Hle {
        emulator.boot_hle()?;
    }
    let trace_interval = std::env::var("HYPERSCANEMU_FRAME_TRACE_INTERVAL")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|interval| *interval != 0);
    let run_result = (1..=frames).try_for_each(|frame| {
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
        Ok(())
    });
    if let Err(error) = run_result {
        let registers = (0..32)
            .filter_map(|index| {
                let value = emulator.cpu().register(index)?;
                (value != 0).then_some(format!("r{index}={value:08x}"))
            })
            .collect::<Vec<_>>()
            .join(" ");
        eprintln!("fault at frame PC {:08x}: {}", emulator.cpu().pc(), error);
        eprintln!("occurrence of dump target: {}", emulator.state_occurrence());
        eprintln!("registers: {registers}");
        if let Some(path) = std::env::var_os("HYPERSCANEMU_STATE_DUMP_PATH") {
            let _ = fs::write(Path::new(&path), emulator.serialize());
        }
        if let Some(path) = std::env::var_os("HYPERSCANEMU_RESIDENT_DATA_DUMP") {
            let samples = emulator.resident_data_samples();
            let mut text = String::new();
            for ((pc, address), value) in samples {
                text.push_str(&format!(
                    "{pc:08x} {address:06x} {value:02x}
"
                ));
            }
            let _ = fs::write(Path::new(&path), text);
        }
        return Err(error);
    }
    print_uart(&mut emulator);
    print_run_summary(&emulator, frames)?;
    if let Some(summary) = emulator.bios_activity_summary() {
        println!("{summary}");
    }
    if std::env::var_os("HYPERSCANEMU_PC_PROFILE").is_some() {
        for (address, count) in emulator.bus().mmio_read_summary() {
            println!("[mmio-profile] {address:08x} {count}");
        }
    }
    if std::env::var_os("HYPERSCANEMU_PC_PROFILE").is_some() {
        for (pc, count) in emulator.pc_profile_summary() {
            println!("[pc-profile] {pc:08x} {count}");
        }
    }
    if let Some(path) = std::env::var_os("HYPERSCANEMU_FINAL_DRAM_DUMP") {
        let dram = emulator.bus().dram_contents();
        let path = Path::new(&path);
        fs::write(path, dram).with_context(|| format!("failed to write {}", path.display()))?;
        println!("wrote final DRAM dump to {}", path.display());
    }
    if let Some(path) = std::env::var_os("HYPERSCANEMU_RESIDENT_DATA_DUMP") {
        let samples = emulator.resident_data_samples();
        let mut text = String::new();
        for ((pc, address), value) in samples {
            text.push_str(&format!(
                "{pc:08x} {address:06x} {value:02x}
"
            ));
        }
        let path = Path::new(&path);
        fs::write(path, text).with_context(|| format!("failed to write {}", path.display()))?;
        println!("wrote resident data samples to {}", path.display());
    }
    if let Some(path) = std::env::var_os("HYPERSCANEMU_HANDOFF_DRAM_DUMP") {
        if let Some(dram) = emulator.take_handoff_dram() {
            let path = Path::new(&path);
            fs::write(path, dram).with_context(|| format!("failed to write {}", path.display()))?;
            println!("wrote handoff DRAM dump to {}", path.display());
        }
    }
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

fn trace(system_dir: &Path, steps: u64) -> Result<()> {
    let firmware = hyperscanemu_core::firmware::load(system_dir)
        .context("the instruction trace executes real firmware and requires it")?;
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
    if let Some(summary) = emulator.bios_activity_summary() {
        println!("{summary}");
    }
    Ok(())
}

fn print_run_summary(emulator: &Emulator, frames: u64) -> Result<()> {
    let (width, height) = emulator.display_size();
    println!(
        "completed {} frame(s): {}x{}, frame fingerprint {:016x}, PC {:08x}, PPU {:08x}, TVE {:08x}, boot {}",
        frames,
        width,
        height,
        frame_fingerprint(emulator.framebuffer()),
        emulator.cpu().pc(),
        emulator.bus().read_u32(0x0801_0000)?,
        emulator.bus().read_u32(0x0803_0000)?,
        emulator.emulation_strategy(),
    );
    println!(
        "video state: sprites {:08x}, layers [{:08x}, {:08x}, {:08x}], TV buffer {}",
        emulator.bus().read_u32(0x0801_0004)?,
        emulator.bus().read_u32(0x0801_002c)?,
        emulator.bus().read_u32(0x0801_0048)?,
        emulator.bus().read_u32(0x0801_0064)?,
        emulator.bus().read_u32(0x0809_0020)?
    );
    println!(
        "layer buffers: [{:08x}, {:08x}, {:08x}], sprite {}, max {}",
        emulator.bus().read_u32(0x0801_00a0)?,
        emulator.bus().read_u32(0x0801_00ac)?,
        emulator.bus().read_u32(0x0801_00b8)?,
        emulator.bus().read_u32(0x0801_00d0)?,
        emulator.bus().read_u32(0x0801_0008)?
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
