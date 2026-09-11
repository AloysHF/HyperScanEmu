use std::env;
use std::error::Error;
use std::fs;

use hyperscanemu_core::{DiscImage, Emulator, Firmware};

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("inspect-disc") {
        if args.len() != 2 {
            return Err(usage().into());
        }
        let disc = DiscImage::from_mode1_2352(fs::read(&args[1])?)?;
        println!(
            "validated MODE1/2352 image: {} sectors, fingerprint {:016x}",
            disc.sector_count(),
            disc.fingerprint()
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
    "usage:\n  hyperscanemu inspect-disc <track.bin>\n  hyperscanemu trace <internal-rom.bin> <bios.bin> [steps]"
}
