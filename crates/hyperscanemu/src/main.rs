use std::env;
use std::error::Error;
use std::fs;

use hyperscanemu_core::{Emulator, Firmware};
use hyperscanemu_media::load_disc;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("inspect-disc") {
        if args.len() != 2 {
            return Err(usage().into());
        }
        let disc = load_disc(&args[1])?;
        println!(
            "validated MODE1/2352 image: {} sectors, fingerprint {:016x}",
            disc.sector_count(),
            disc.fingerprint()
        );
        return Ok(());
    }
    if args.first().map(String::as_str) == Some("run") {
        if !(4..=5).contains(&args.len()) {
            return Err(usage().into());
        }
        let frames = args
            .get(4)
            .map(|value| value.parse::<u64>())
            .transpose()?
            .unwrap_or(1);
        let firmware = Firmware::from_parts(&fs::read(&args[1])?, &fs::read(&args[2])?)?;
        let disc = load_disc(&args[3])?;
        let mut emulator = Emulator::new(firmware);
        emulator.attach_disc(disc);
        for _ in 0..frames {
            emulator.run_frame()?;
        }
        let (width, height) = emulator.display_size();
        println!(
            "completed {} frame(s): {}x{}, frame fingerprint {:016x}, PC {:08x}",
            frames,
            width,
            height,
            frame_fingerprint(emulator.framebuffer()),
            emulator.cpu().pc()
        );
        return Ok(());
    }
    if args.first().map(String::as_str) != Some("trace") || !(3..=4).contains(&args.len()) {
        return Err(usage().into());
    }

    let steps = args
        .get(3)
        .map(|value| value.parse::<u64>())
        .transpose()?
        .unwrap_or(1_000);
    let internal_rom = fs::read(&args[1])?;
    let bios_rom = fs::read(&args[2])?;
    let firmware = Firmware::from_parts(&internal_rom, &bios_rom)?;
    let mut emulator = Emulator::new(firmware);
    let report = emulator.run_instructions(steps)?;
    println!(
        "executed {} instructions / {} estimated cycles: PC {:08x} -> {:08x}, {} exceptions",
        report.instructions, report.cycles, report.start_pc, report.end_pc, report.exceptions
    );
    Ok(())
}

fn usage() -> &'static str {
    "usage:\n  hyperscanemu inspect-disc <media.bin|media.cue|media.zip>\n  hyperscanemu trace <internal-rom.bin> <bios.bin> [steps]\n  hyperscanemu run <internal-rom.bin> <bios.bin> <media.bin|media.cue|media.zip> [frames]"
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
}
