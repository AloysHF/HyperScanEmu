//! Environment-gated firmware activity instrumentation.
//!
//! Enabled by setting `HYPERSCANEMU_TRACE_BIOS_FETCH` in the environment.
//! While enabled the trace measures whether guest code touches either
//! firmware ROM window after the BIOS-to-game handoff, which bounds how
//! much firmware behavior an HLE boot path would have to reproduce.
//! Disabled runs only pay a single branch per instruction.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

/// Environment variable that enables the trace. No value is required.
pub const TRACE_ENV_VAR: &str = "HYPERSCANEMU_TRACE_BIOS_FETCH";

/// HYPER.EXE entry point documented by the SPG290 SDK linker script
/// (`0xA0091000`, which aliases to this DRAM offset).
pub const GAME_ENTRY_PHYSICAL: u32 = 0x0009_1000;

const EXTERNAL_ROM_START: u32 = 0x1000_0000;
const INTERNAL_ROM_START: u32 = 0x0b00_0000;
const INTERNAL_ROM_END: u32 = 0x0b00_7fff;
const MMIO_START: u32 = 0x0800_0000;

/// Distinct DRAM entry PCs recorded before the handoff is seen.
const PRE_HANDOFF_DRAM_CAP: usize = 16;
/// Distinct firmware fetch PCs recorded after the handoff.
const POST_HANDOFF_FETCH_CAP: usize = 32;
/// Distinct firmware byte-read addresses recorded after the handoff.
const POST_HANDOFF_READ_CAP: usize = 32;
/// Distinct DRAM service entries recorded after the handoff.
const POST_HANDOFF_SERVICE_CAP: usize = 64;

/// DRAM addresses at or above this line belong to the loaded game image
/// (HYPER.EXE at 0x000901FC with entry 0x00091000).
const GAME_IMAGE_START: u32 = 0x0009_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchRegion {
    Dram,
    BiosWindow,
    InternalRom,
    Other,
}

fn fetch_region(physical: u32) -> FetchRegion {
    if physical >= EXTERNAL_ROM_START {
        FetchRegion::BiosWindow
    } else if (INTERNAL_ROM_START..=INTERNAL_ROM_END).contains(&physical) {
        FetchRegion::InternalRom
    } else if physical < MMIO_START {
        FetchRegion::Dram
    } else {
        FetchRegion::Other
    }
}

/// Data reads from the firmware windows. Accessed through `&self` from the
/// bus read paths, so the fields shared with the fetch path use atomics and
/// the distinct-address lists use a mutex.
#[derive(Debug, Default)]
struct RomDataTrace {
    handoff_seen: AtomicBool,
    /// Physical word address of the in-progress instruction fetch, or zero.
    fetch_word: AtomicU32,
    bios_byte_reads: AtomicU64,
    internal_byte_reads: AtomicU64,
    bios_read_addrs: Mutex<Vec<u32>>,
    internal_read_addrs: Mutex<Vec<u32>>,
}

impl RomDataTrace {
    fn record_handoff(&self) {
        self.handoff_seen.store(true, Ordering::Relaxed);
    }

    fn note_rom_byte_read(&self, address: u32) {
        if !self.handoff_seen.load(Ordering::Relaxed) {
            return;
        }
        let physical = address & crate::ADDRESS_MASK;
        if self.fetch_word.load(Ordering::Relaxed) == physical & !3 {
            return;
        }
        if (INTERNAL_ROM_START..=INTERNAL_ROM_END).contains(&physical) {
            self.internal_byte_reads.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut addrs) = self.internal_read_addrs.lock() {
                if addrs.len() < POST_HANDOFF_READ_CAP && !addrs.contains(&physical) {
                    addrs.push(physical);
                }
            }
        } else {
            self.bios_byte_reads.fetch_add(1, Ordering::Relaxed);
            if let Ok(mut addrs) = self.bios_read_addrs.lock() {
                if addrs.len() < POST_HANDOFF_READ_CAP && !addrs.contains(&physical) {
                    addrs.push(physical);
                }
            }
        }
    }
}

impl Clone for RomDataTrace {
    fn clone(&self) -> Self {
        Self {
            handoff_seen: AtomicBool::new(self.handoff_seen.load(Ordering::Relaxed)),
            fetch_word: AtomicU32::new(self.fetch_word.load(Ordering::Relaxed)),
            bios_byte_reads: AtomicU64::new(self.bios_byte_reads.load(Ordering::Relaxed)),
            internal_byte_reads: AtomicU64::new(self.internal_byte_reads.load(Ordering::Relaxed)),
            bios_read_addrs: Mutex::new(
                self.bios_read_addrs
                    .lock()
                    .map(|list| list.clone())
                    .unwrap_or_default(),
            ),
            internal_read_addrs: Mutex::new(
                self.internal_read_addrs
                    .lock()
                    .map(|list| list.clone())
                    .unwrap_or_default(),
            ),
        }
    }
}

/// Accumulated firmware activity measurements for one boot.
#[derive(Debug, Clone, Default)]
pub struct BiosActivity {
    enabled: bool,
    instructions: u64,
    fetches_bios: u64,
    fetches_internal: u64,
    fetches_dram: u64,
    handoff_seen: bool,
    handoff_cycles: u64,
    handoff_pc: u32,
    pre_handoff_dram_pcs: Vec<u32>,
    post_handoff_bios_fetches: u64,
    post_handoff_internal_fetches: u64,
    post_handoff_bios_fetch_pcs: Vec<u32>,
    post_handoff_internal_fetch_pcs: Vec<u32>,
    /// Fetch PCs of crossings from game DRAM into BIOS-resident DRAM.
    post_handoff_service_entries: Vec<u32>,
    post_handoff_service_calls: u64,
    previous_fetch_physical: u32,
    rom_data: RomDataTrace,
}

impl BiosActivity {
    pub fn new() -> Self {
        Self {
            enabled: std::env::var_os(TRACE_ENV_VAR).is_some(),
            ..Self::default()
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Record one instruction fetch. Returns `true` exactly when this fetch
    /// is the BIOS-to-game handoff transition (first fetch of the documented
    /// game entry point), so the caller can dump a handoff snapshot.
    pub fn note_fetch(&mut self, pc: u32, cycles: u64) -> bool {
        if !self.enabled {
            return false;
        }
        self.instructions += 1;
        let physical = pc & crate::ADDRESS_MASK;
        self.rom_data
            .fetch_word
            .store(physical & !3, Ordering::Relaxed);
        let region = fetch_region(physical);
        if !self.handoff_seen {
            match region {
                FetchRegion::BiosWindow => self.fetches_bios += 1,
                FetchRegion::InternalRom => self.fetches_internal += 1,
                FetchRegion::Dram => {
                    self.fetches_dram += 1;
                    if self.pre_handoff_dram_pcs.len() < PRE_HANDOFF_DRAM_CAP
                        && !self.pre_handoff_dram_pcs.contains(&physical)
                    {
                        self.pre_handoff_dram_pcs.push(physical);
                    }
                    if physical == GAME_ENTRY_PHYSICAL {
                        self.handoff_seen = true;
                        self.handoff_cycles = cycles;
                        self.handoff_pc = physical;
                        self.rom_data.record_handoff();
                        return true;
                    }
                }
                FetchRegion::Other => {}
            }
            return false;
        }
        match region {
            FetchRegion::BiosWindow => {
                self.fetches_bios += 1;
                self.post_handoff_bios_fetches += 1;
                if self.post_handoff_bios_fetch_pcs.len() < POST_HANDOFF_FETCH_CAP
                    && !self.post_handoff_bios_fetch_pcs.contains(&physical)
                {
                    self.post_handoff_bios_fetch_pcs.push(physical);
                }
            }
            FetchRegion::InternalRom => {
                self.fetches_internal += 1;
                self.post_handoff_internal_fetches += 1;
                if self.post_handoff_internal_fetch_pcs.len() < POST_HANDOFF_FETCH_CAP
                    && !self.post_handoff_internal_fetch_pcs.contains(&physical)
                {
                    self.post_handoff_internal_fetch_pcs.push(physical);
                }
            }
            FetchRegion::Dram => self.fetches_dram += 1,
            FetchRegion::Other => {}
        }
        if region == FetchRegion::Dram
            && self.previous_fetch_physical >= GAME_IMAGE_START
            && physical < GAME_IMAGE_START
        {
            self.post_handoff_service_calls += 1;
            if self.post_handoff_service_entries.len() < POST_HANDOFF_SERVICE_CAP
                && !self.post_handoff_service_entries.contains(&physical)
            {
                self.post_handoff_service_entries.push(physical);
            }
        }
        self.previous_fetch_physical = physical;
        false
    }

    /// Mark the current instruction fetch as finished so later byte reads of
    /// the same word count as data accesses.
    pub fn end_fetch(&mut self) {
        if self.enabled {
            self.rom_data.fetch_word.store(0, Ordering::Relaxed);
        }
    }

    /// Record a byte read from the internal ROM or BIOS ROM window. Fetch
    /// bytes of the current instruction are attributed to the fetch, not to
    /// data traffic.
    pub fn note_rom_byte_read(&self, address: u32) {
        if self.enabled {
            self.rom_data.note_rom_byte_read(address);
        }
    }

    /// Human-readable end-of-run summary. `None` while disabled.
    pub fn summary(&self) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let mut text = String::new();
        let _ = writeln!(text, "BIOS activity: {} instructions", self.instructions);
        let _ = writeln!(
            text,
            "  fetches: bios-window {}, internal-rom {}, dram {}",
            self.fetches_bios, self.fetches_internal, self.fetches_dram
        );
        if self.handoff_seen {
            let _ = writeln!(
                text,
                "  handoff: PC {:08x} at {} estimated cycles",
                self.handoff_pc, self.handoff_cycles
            );
            let _ = writeln!(
                text,
                "  post-handoff bios-window fetches: {} (distinct up to {}: {:?})",
                self.post_handoff_bios_fetches,
                POST_HANDOFF_FETCH_CAP,
                self.post_handoff_bios_fetch_pcs
            );
            let _ = writeln!(
                text,
                "  post-handoff internal-rom fetches: {} (distinct up to {}: {:?})",
                self.post_handoff_internal_fetches,
                POST_HANDOFF_FETCH_CAP,
                self.post_handoff_internal_fetch_pcs
            );
            let _ = writeln!(
                text,
                "  post-handoff bios-window data byte reads: {} (distinct up to {}: {:?})",
                self.rom_data.bios_byte_reads.load(Ordering::Relaxed),
                POST_HANDOFF_READ_CAP,
                self.rom_data
                    .bios_read_addrs
                    .lock()
                    .map(|list| list.clone())
                    .unwrap_or_default()
            );
            let _ = writeln!(
                text,
                "  post-handoff internal-rom data byte reads: {} (distinct up to {}: {:?})",
                self.rom_data.internal_byte_reads.load(Ordering::Relaxed),
                POST_HANDOFF_READ_CAP,
                self.rom_data
                    .internal_read_addrs
                    .lock()
                    .map(|list| list.clone())
                    .unwrap_or_default()
            );
            let _ = writeln!(
                text,
                "  post-handoff crossings into resident dram below {GAME_IMAGE_START:08x}: {} (distinct up to {}: {:?})",
                self.post_handoff_service_calls,
                POST_HANDOFF_SERVICE_CAP,
                self.post_handoff_service_entries
            );
        } else {
            let _ = writeln!(text, "  handoff: not reached");
        }
        let _ = writeln!(
            text,
            "  pre-handoff dram entry PCs (up to {}): {:?}",
            PRE_HANDOFF_DRAM_CAP, self.pre_handoff_dram_pcs
        );
        Some(text)
    }
}
