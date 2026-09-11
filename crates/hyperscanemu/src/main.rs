use std::env;
use std::error::Error;
use std::fs;

use hyperscanemu_core::{DiscImage, Emulator, Firmware};

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    if args.next().as_deref() == Some("inspect-disc") {
        let path = args
            .next()
            .ok_or("usage: hyperscanemu inspect-disc <track.bin>")?;
        if args.next().is_some() {
            return Err("usage: hyperscanemu inspect-disc <track.bin>".into());
        }
        let disc = DiscImage::from_mode1_2352(fs::read(path)?)?;
        println!(
            "validated MODE1/2352 image: {} sectors, fingerprint {:016x}",
            disc.sector_count(),
            disc.fingerprint()
        );
        return Ok(());
    }

    let mut args = env::args().skip(1);
    let internal_path = args
        .next()
        .ok_or("usage: hyperscanemu <internal-rom.bin> <bios.bin>")?;
    let bios_path = args
        .next()
        .ok_or("usage: hyperscanemu <internal-rom.bin> <bios.bin>")?;
    if args.next().is_some() {
        return Err("usage: hyperscanemu <internal-rom.bin> <bios.bin>".into());
    }

    let internal_rom = fs::read(internal_path)?;
    let bios_rom = fs::read(bios_path)?;
    let firmware = Firmware::from_parts(&internal_rom, &bios_rom)?;
    let mut emulator = Emulator::new(firmware);

    emulator.run_frame()?;
    Ok(())
}
