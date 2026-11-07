//! High-level-emulation boot path.
//!
//! Boots a retail disc without firmware by reproducing the machine state the
//! real BIOS leaves at the BIOS-to-game handoff. The contract was measured
//! with the `HYPERSCANEMU_TRACE_BIOS_FETCH` instrumentation (see
//! `docs/Architecture.md`): the BIOS loads HYPER.EXE flat at 0xA00901FC and
//! enters it at 0xA0091000 (SDK `hyperscan_Prog.ld`); retail titles install
//! their own exception vectors and never fetch from either firmware ROM
//! window again.
//!
//! Known limitation: firmware code also stays resident in guest DRAM (an
//! interrupt dispatcher below 0x00010000 and a CD service around
//! 0x00028000-0x00034000) that games call at runtime. Reimplementing those
//! services natively is tracked separately; until then the HLE boot path is
//! the measurement and infrastructure baseline.

use crate::{iso9660, Bus, DiscImage, EmulatorError, Score7};

/// HYPER.EXE path on the game disc.
pub const GAME_EXECUTABLE_PATH: &str = "HYPER.EXE";
/// DRAM load address of HYPER.EXE (SDK `hyperscan_Prog.ld`, `.exception`).
pub const GAME_LOAD_ADDRESS: u32 = 0x0009_01FC;
/// HYPER.EXE entry point (SDK `hyperscan_Prog.ld`).
pub const GAME_ENTRY: u32 = 0x0009_1000;
/// Stack pointer handed to the game. The bus address mask mirrors DRAM, so
/// pushes from this value land at the top of the 16 MiB DRAM exactly like
/// the 0xA0FFFFxx stack window of the SDK linker script.
pub const GAME_STACK_POINTER: u32 = 0x2000_0000;
/// Exception vector base (`cr3`) measured at the handoff.
pub const HANDOFF_VECTOR_BASE: u32 = 0xA000_0000;

/// Control registers `cr0`-`cr7` measured at the handoff. Retail titles
/// leave identical values; interrupts are enabled and vectors point into the
/// BIOS-resident DRAM dispatcher.
const HANDOFF_CONTROLS: [(usize, u32); 8] = [
    (0, 0x0000_0005),
    (1, 0x0000_0106),
    (2, 0x00e0_0014),
    (3, HANDOFF_VECTOR_BASE),
    (4, 0x0000_0000),
    (5, 0xa002_502c),
    (6, 0x0000_0000),
    (7, 0x0000_0000),
];

/// General registers measured at the handoff. Retail titles leave identical
/// values; the two counters that differ between runs (`r7`, `ce`) are
/// transient and stay at their reset values.
const HANDOFF_REGISTERS: [(usize, u32); 23] = [
    (0, 0xa0ff_ff58),
    (1, 0xa004_2bfc),
    (2, 0xa0ff_ff58),
    (3, 0xa000_1b24),
    (4, GAME_ENTRY),
    (6, 0x8006_feff),
    (8, 0xa004_bbc4),
    (9, 0xfefe_feff),
    (10, 0x0000_0095),
    (12, 0xfcc5_92db),
    (13, 0xfcc5_92db),
    (14, 0x0000_4834),
    (16, 0x0000_0001),
    (18, 0x0000_0090),
    (19, 0x8815_0010),
    (20, 0x0000_0005),
    (23, 0x0000_0008),
    (24, 0x0000_0001),
    (25, 0x0100_0000),
    (26, 0xfcc5_92db),
    (27, 0x0040_0000),
    (28, 0xa004_5140),
    (30, 0xa000_1000),
];

/// Boot a game disc without firmware: parse the disc's ISO 9660 tree, load
/// HYPER.EXE flat into DRAM, and enter it with the measured handoff state.
pub fn boot(cpu: &mut Score7, bus: &mut Bus, disc: &DiscImage) -> Result<(), EmulatorError> {
    let file = iso9660::find_file(disc, GAME_EXECUTABLE_PATH)?;
    let bytes = iso9660::read_file(disc, file)?;
    bus.load_dram(GAME_LOAD_ADDRESS, &bytes)?;
    for (index, value) in HANDOFF_REGISTERS {
        cpu.set_register(index, value)?;
    }
    cpu.set_register(29, GAME_STACK_POINTER)?;
    for (index, value) in HANDOFF_CONTROLS {
        cpu.set_control_register(index, value)?;
    }
    cpu.set_pc(GAME_ENTRY);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{iso9660::test_fixtures::synthetic_disc, DRAM_SIZE};

    #[test]
    fn handoff_contract_constants_match_the_linker_script() {
        assert_eq!(GAME_LOAD_ADDRESS, 0xA009_01FC & crate::ADDRESS_MASK);
        assert_eq!(GAME_ENTRY, 0xA009_1000 & crate::ADDRESS_MASK);
        // The stack pointer aliases to DRAM base; the first push wraps into
        // the top of the 16 MiB DRAM, matching the 0xA0FFFFxx stack window.
        let first_push = (GAME_STACK_POINTER - 4) & crate::ADDRESS_MASK;
        assert_eq!(first_push % DRAM_SIZE as u32, DRAM_SIZE as u32 - 4);
        assert_eq!(HANDOFF_VECTOR_BASE & 0xffff_0000, 0xA000_0000);
    }

    #[test]
    fn handoff_state_has_no_conflicting_registers() {
        let mut indices: Vec<usize> = HANDOFF_REGISTERS.iter().map(|entry| entry.0).collect();
        indices.push(29);
        indices.sort_unstable();
        indices.dedup();
        assert_eq!(indices.len(), HANDOFF_REGISTERS.len() + 1);
    }

    #[test]
    fn boots_the_synthetic_game_with_the_measured_handoff_state() {
        let game: Vec<u8> = (0..48_u32).map(|index| index as u8).collect();
        let disc = synthetic_disc(&[("HYPER.EXE;1", &game)]);
        let mut cpu = Score7::new();
        let mut bus = Bus::new_hle();

        boot(&mut cpu, &mut bus, &disc).unwrap();

        assert_eq!(cpu.pc(), GAME_ENTRY);
        assert_eq!(cpu.register(29), Some(GAME_STACK_POINTER));
        assert_eq!(cpu.register(4), Some(GAME_ENTRY));
        assert_eq!(cpu.register(0), Some(0xa0ff_ff58));
        assert_eq!(
            cpu.control_register(crate::cpu::CR_EXCEPTION_VECTOR),
            Some(HANDOFF_VECTOR_BASE)
        );
        for (index, byte) in game.iter().enumerate() {
            assert_eq!(
                bus.read_u8(GAME_LOAD_ADDRESS + index as u32).unwrap(),
                *byte
            );
        }
    }

    #[test]
    fn reports_a_disc_without_the_game_executable() {
        let disc = synthetic_disc(&[("OTHER.BIN", &[1, 2, 3])]);
        let mut cpu = Score7::new();
        let mut bus = Bus::new_hle();

        assert_eq!(
            boot(&mut cpu, &mut bus, &disc),
            Err(EmulatorError::InvalidDisc(
                "the disc directory has no such entry"
            ))
        );
    }
}
