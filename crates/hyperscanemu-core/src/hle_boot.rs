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

/// MMIO register values measured at the BIOS-to-game handoff. The BIOS
/// configures interrupts, video, timers, clocks and the memory interface
/// during boot; retail titles rely on that state without reprogramming
/// all of it. Values from a Spider-Man reference run; see
/// `tmp/project-status.md`.
// Measured firmware handoff MMIO register values (Spider-Man LLE run).
// Configuration state the BIOS leaves behind; see tmp/project-status.md.
const HANDOFF_MMIO_REGISTERS: [(u32, &[u32]); 77] = [
    (
        0x08010000,
        &[
            0x00001001, 0x00000003, 0x000001ff, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08010020,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x0000100b, 0xa03c0080, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08010040,
        &[
            0x00000000, 0x00002000, 0x0000018b, 0xa04c8080, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08010060,
        &[
            0x00000000, 0x00000002, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08010080,
        &[
            0x00000001, 0x00000002, 0x00000000, 0x00000000, 0x0000005a, 0x000001f5, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080100a0,
        &[
            0xa03c4100, 0x00000000, 0x00000000, 0xa04cc100, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080100c0,
        &[
            0x00000000, 0xa0cf0000, 0xa0df0000, 0xa0ef0000, 0xa0000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080100e0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08050000,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08050020,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08050040,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08050060,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08050080,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080500a0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080500c0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080500e0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (0x08060000, &[0x000000ff]),
    (0x08060008, &[0x00000016, 0x00000000]),
    (
        0x08060040,
        &[
            0x00004813, 0x00000088, 0x00000004, 0x00000006, 0x00000004, 0x00000000,
        ],
    ),
    (
        0x08060060,
        &[0x8005d000, 0x8006feff, 0x80066c80, 0x00000940, 0x00000010],
    ),
    (0x08060080, &[0x00000002]),
    (
        0x0807000c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x0807002c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00200000, 0xa0cf0000,
            0xa0df0000,
        ],
    ),
    (
        0x0807004c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x95404b04, 0xa0ef0000,
            0x00000000,
        ],
    ),
    (
        0x0807006c,
        &[
            0x00000001, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x0807008c,
        &[
            0x00000000, 0x00000000, 0x00000003, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080700ac,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080700cc,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080700ec,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00118317,
            0x00000000,
        ],
    ),
    (
        0x0807010c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x0807012c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x0807014c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x0807016c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x0807018c,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080701ac,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080701cc,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080701ec,
        &[0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
    ),
    (
        0x08090000,
        &[
            0x00000000, 0x000000f0, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08090020,
        &[
            0x00000003, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000001,
        ],
    ),
    (
        0x08090040,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000001, 0x00000000, 0x00000000, 0x00000001,
            0x00000000,
        ],
    ),
    (
        0x08090060,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08090080,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080900a0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080900c0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080900e0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x080a0000,
        &[
            0x00000008, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (0x080a0020, &[0x00000000, 0x00000000]),
    (
        0x08130020,
        &[
            0x00000049, 0x00000000, 0x00000258, 0x000000a8, 0x00000014, 0x00000000, 0x000003f9,
        ],
    ),
    (
        0x08150000,
        &[0x000000ff, 0x00000000, 0x00001076, 0x00000000, 0x00000090],
    ),
    (
        0x08160000,
        &[0x8800fdbb, 0x00000000, 0x0000f8f8, 0x00000000, 0x00000000],
    ),
    (
        0x08161000,
        &[0x8800f9df, 0x00000000, 0x0000f8f8, 0x00000000, 0x00000000],
    ),
    (
        0x08162000,
        &[0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
    ),
    (
        0x08163000,
        &[0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
    ),
    (
        0x08164000,
        &[0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
    ),
    (
        0x08165000,
        &[0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000],
    ),
    (
        0x08200000,
        &[
            0x00000000, 0x00000001, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x000fffff,
            0x000fffff,
        ],
    ),
    (
        0x08200020,
        &[
            0x00000000, 0x1ffe0000, 0x00000001, 0x00000000, 0xf819f819, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08200040,
        &[
            0x00000000, 0x00000000, 0xff000000, 0x00003302, 0x00000000, 0x00000000, 0x00000003,
            0x00000000,
        ],
    ),
    (
        0x08200060,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08200080,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000100,
        ],
    ),
    (
        0x082000a0,
        &[
            0x00000000, 0x00010000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x082000c0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x082000e0,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08200100,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        ],
    ),
    (
        0x08210000,
        &[
            0x00000006, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000003,
            0x00000008,
        ],
    ),
    (
        0x08210020,
        &[
            0x00000003, 0x00000000, 0x00000000, 0x00000000, 0x00000003, 0x00000003, 0x00000002,
            0x00000003,
        ],
    ),
    (
        0x08210040,
        &[
            0x00000000, 0x00000003, 0x00000000, 0x00000000, 0x00000003, 0x00000000, 0x00000003,
            0x00000003,
        ],
    ),
    (
        0x08210060,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08210080,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000003, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x082100a0,
        &[
            0x00000000, 0x00000003, 0x00000000, 0x00000003, 0x00000001, 0x00000001, 0x00000007,
            0x00000007,
        ],
    ),
    (
        0x082100c0,
        &[
            0x00000000, 0x00000000, 0x00000003, 0x00000000, 0x00000000, 0x00000000, 0x00000001,
            0x00000000,
        ],
    ),
    (
        0x082100e0,
        &[
            0x00000000, 0x0000000e, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000003,
        ],
    ),
    (
        0x08210100,
        &[
            0x00000003, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
        ],
    ),
    (
        0x08230000,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08230020,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (
        0x08230040,
        &[
            0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000, 0x00000000,
            0x00000000,
        ],
    ),
    (0x08230060, &[0x00000000, 0x00000000]),
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
    for (base, values) in HANDOFF_MMIO_REGISTERS {
        for (index, value) in values.iter().enumerate() {
            let _ = bus.write_u32(base + (index as u32) * 4, *value);
        }
    }
    // Timer gates are write-only derived state: the firmware enables them
    // during boot while the gate registers read back as zero, so the
    // measured snapshot cannot restore them. Re-enable all six.
    for timer in 0..6usize {
        let _ = bus.write_u32(0x0821_006c + (timer as u32) * 4, 1);
    }
    // The servo state itself is internal device state, not an MMIO
    // register: the firmware leaves the disc identified after the TOC read.
    bus.initialize_cd_servo(crate::CdServoState {
        current_sector: 7765,
        seek_lba: 425,
        skip: 2,
        speed: 1,
        frame_found: true,
    });
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
