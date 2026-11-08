//! Native replacement for the firmware-resident DRAM services.
//!
//! The retail BIOS keeps a runtime in guest DRAM below the game image: an
//! interrupt dispatcher (vector slots at `0x200 + source * 4`), a set of
//! small API services entered through trampolines at fixed addresses, and
//! shared data fields that games read directly. Without firmware those
//! regions are zero, so this module reproduces the measured behavior of
//! every service retail titles actually call:
//!
//! - service calls are intercepted before the fetch and executed natively;
//! - interrupt delivery calls the game-registered handler with the same
//!   register conventions the firmware dispatcher uses and returns through
//!   a magic address (`IRQ_RETURN_MAGIC`) that restores the interrupted
//!   state;
//! - BIOS data fields games read are maintained in guest DRAM so plain
//!   loads observe them.

use crate::{iso9660, Bus, DiscImage, EmulatorError, Score7};

/// Context pointer every service trampoline installs in `r28`.
pub const FIRMWARE_CONTEXT: u32 = 0xa004_5140;
/// First DRAM address outside the firmware-resident region.
pub const GAME_IMAGE_START: u32 = 0x0009_0000;
/// Start of the resident region (exception vector page).
pub const RESIDENT_START: u32 = 0x0000_0200;
/// Unused interrupt vector slot used as the native handler return address.
pub const IRQ_RETURN_MAGIC: u32 = 0x0000_027c;
/// Return address handed to the VBlank handler: the native continuation
/// that performs the firmware's post-VBlank bookkeeping.
pub const VBLANK_CONTINUATION_MAGIC: u32 = 0x0000_0278;
/// Park address while a blocking service waits for its wake condition.
pub const BLOCKED_MAGIC: u32 = 0x0000_0274;
/// Firmware counter the delay service sleeps against, advanced by the
/// timer ISR chain (r28-relative 0xa0045140-0x3d7c).
pub const DELAY_TICKS: u32 = 0xa004_13c4;
/// The delay service sleeps until ten delay ticks have passed.
const DELAY_TICKS_PER_SLEEP: u32 = 10;
/// Version returned by the OS version service (measured).
pub const OS_VERSION: u32 = 21;

/// BIOS data fields maintained in guest DRAM, indexed from the measured
/// firmware image.
pub mod bios_field {
    /// Frame counter incremented by the frame-tick service (0xc500).
    pub const FRAME_TICKS: u32 = 0xa004_14d4;
    /// System tick counter read by service 0xa94, bumped per timer IRQ.
    pub const TICKS: u32 = 0xa004_1434;
    /// VBlank handler slot written by service 0xa4c.
    pub const VBLANK_HANDLER: u32 = 0xa004_16c0;
    /// Secondary handler slot written by service 0xa88.
    pub const HANDLER_SLOT_2: u32 = 0xa004_16d4;
    /// Mode fields written by service 0xa1c.
    pub const MODE_FIELD: u32 = 0xa004_12a4;
    pub const MODE_FIELD_2: u32 = 0xa004_12a8;
    pub const MODE_FLAG: u32 = 0xa004_2b70;
    /// The game's sprite list (copied to the PPU sprite RAM per frame) and
    /// its word-count limit.
    pub const SPRITE_LIST: u32 = 0xa004_76b0;
    pub const SPRITE_LIST_LIMIT: u32 = 0xa004_14dc;
    /// The game's runtime sprite list (filled by the game's compiler).
    pub const GAME_SPRITE_LIST: u32 = 0xa015_0ab0;
}

/// PPU register the frame-tick service writes (measured 0xc500 body).
const PPU_FRAME_SERVICE_REG: u32 = 0x0801_0084;
/// Sprite hardware config the firmware sprite manager publishes per frame.
const SPRITE_CONFIG: u32 = 0x0801_0008;
const SPRITE_CONFIG_VALUE: u32 = 0x1ff;
/// The game's sprite list and limit in the firmware data area.
const SPRITE_RAM_WORDS: u32 = 0x400;

/// Interrupt source numbers from `devices.rs`.
const TIMER_SOURCE: u8 = 56;

/// Service entry points (physical DRAM addresses), measured from retail
/// titles and the resident firmware image.
pub mod entry {
    pub const OS_VERSION: u32 = 0x0b18;
    pub const MODE_SET: u32 = 0x0a1c;
    pub const VBLANK_REGISTER: u32 = 0x0a4c;
    pub const HANDLER_REGISTER: u32 = 0x0a88;
    pub const TICKS_GET: u32 = 0x0a94;
    pub const FILE_OPEN: u32 = 0x089c;
    pub const FILE_READ: u32 = 0x08a8;
    pub const FILE_CLOSE: u32 = 0x08c0;
    pub const FILE_INFO: u32 = 0x0908;
    pub const FILE_QUERY: u32 = 0x0914;
    /// Frame-pacing busy wait; served immediately by the native path.
    pub const DELAY: u32 = 0x080c;
    /// Filesystem status query returning BIOS data-area pointers.
    pub const FS_STATUS: u32 = 0x08fc;
    /// Filesystem mode configuration (mode 1-4 plus a context value).
    pub const FS_CONFIGURE: u32 = 0x08f0;
    /// Critical-section enter/leave pair (measured bodies are flag waits).
    pub const CRITICAL_ENTER: u32 = 0x0890;
    pub const CRITICAL_LEAVE: u32 = 0x08cc;
    /// Mode-field query returning whether the field equals -1.
    pub const MODE_QUERY: u32 = 0x0a28;
    /// Tick counter read (same counter as 0xa94's slot at 0x1434).
    pub const TICKS_GET2: u32 = 0x0a9a;
    /// USB-controller configuration write (256 to 0x081b_00dc).
    pub const USB_CONFIG: u32 = 0x0a58;
    /// MIU/PPU DMA setup writing measured register windows.
    pub const DMA_SETUP: u32 = 0x0b00;
    /// Memory-control unlock sequence plus a status query.
    pub const MEM_CONTROL: u32 = 0x08e4;
    pub const FRAME_TICK: u32 = 0xc500;
}

/// Why a resident-service dispatch could not be completed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResidentError {
    /// A fetch landed in the resident region with no native replacement.
    UnhandledResidentEntry { pc: u32 },
    /// A service returned to an address outside the game image.
    BadReturn { entry: u32, target: u32 },
    /// A service referenced a file handle that is not open.
    BadFileHandle { entry: u32, handle: u32 },
}

/// A blocking delay service call waiting for its tick budget.
#[derive(Debug, Clone, Copy)]
struct BlockedDelay {
    saved: u32,
    return_addr: u32,
}

#[derive(Debug)]
struct OpenFile {
    lba: u32,
    size: u32,
    pos: u32,
}

/// Saved CPU state for one native interrupt-handler invocation.
#[derive(Debug, Clone)]
struct IrqFrame {
    registers: [u32; 32],
    control: [u32; 32],
    pc: u32,
}

impl IrqFrame {
    fn capture(cpu: &Score7) -> Self {
        let mut registers = [0; 32];
        let mut control = [0; 32];
        for (index, slot) in registers.iter_mut().enumerate() {
            *slot = cpu.register(index).unwrap_or(0);
        }
        for (index, slot) in control.iter_mut().enumerate() {
            *slot = cpu.control_register(index).unwrap_or(0);
        }
        Self {
            registers,
            control,
            pc: cpu.pc(),
        }
    }

    fn restore(&self, cpu: &mut Score7) {
        for (index, value) in self.registers.iter().enumerate() {
            let _ = cpu.set_register(index, *value);
        }
        for (index, value) in self.control.iter().enumerate() {
            let _ = cpu.set_control_register(index, *value);
        }
        cpu.set_pc(self.pc);
    }
}

/// Native state for the firmware-resident services.
#[derive(Debug, Default)]
pub struct ResidentServices {
    irq_handlers: Vec<(u8, u32)>,
    files: Vec<Option<OpenFile>>,
    irq_frames: Vec<IrqFrame>,
    unhandled_logged: Vec<u32>,
    delivery_counts: std::collections::HashMap<u32, u64>,
    /// Interrupt sources observed while delivery was deferred (handler
    /// running or multiple sources in one batch).
    carry: u64,
    blocked: Option<BlockedDelay>,
    log: bool,
}

impl ResidentServices {
    pub fn new() -> Self {
        Self {
            log: std::env::var_os(crate::SERVICE_TRACE_ENV_VAR).is_some(),
            ..Self::default()
        }
    }

    /// Classify a physical fetch address inside the resident region.
    pub fn classify(pc: u32) -> Option<ResidentFetch> {
        if pc == IRQ_RETURN_MAGIC || pc == VBLANK_CONTINUATION_MAGIC || pc == BLOCKED_MAGIC {
            return Some(ResidentFetch::IrqReturn);
        }
        // Odd-alias entries: titles call some services two bytes into the
        // trampoline; the fetch still reads the aligned entry's word, so
        // route it to the aligned service.
        if pc & 3 != 0 {
            if let Some(ResidentFetch::Service(entry)) = Self::classify(pc & !3) {
                return Some(ResidentFetch::Service(entry));
            }
        }
        if (RESIDENT_START..GAME_IMAGE_START).contains(&pc) {
            if matches!(
                pc,
                entry::OS_VERSION
                    | entry::MODE_SET
                    | entry::VBLANK_REGISTER
                    | entry::HANDLER_REGISTER
                    | entry::TICKS_GET
                    | entry::TICKS_GET2
                    | entry::FILE_OPEN
                    | entry::FILE_READ
                    | entry::FILE_CLOSE
                    | entry::FILE_INFO
                    | entry::FILE_QUERY
                    | entry::DELAY
                    | entry::FS_STATUS
                    | entry::FS_CONFIGURE
                    | entry::CRITICAL_ENTER
                    | entry::CRITICAL_LEAVE
                    | entry::MODE_QUERY
                    | entry::USB_CONFIG
                    | entry::DMA_SETUP
                    | entry::MEM_CONTROL
                    | entry::FRAME_TICK
            ) {
                return Some(ResidentFetch::Service(pc));
            }
            // IRQ vector slots are dispatched natively and never fetched.
            if !((0x0280..0x0300).contains(&pc)) {
                return Some(ResidentFetch::Unhandled(pc));
            }
            return Some(ResidentFetch::Unhandled(pc));
        }
        None
    }

    /// Run the native replacement for a service call.
    pub fn handle_service(
        &mut self,
        cpu: &mut Score7,
        bus: &mut Bus,
        disc: Option<&DiscImage>,
        entry: u32,
    ) -> Result<(), EmulatorError> {
        if self.log {
            eprintln!(
                "[svc] entry={entry:04x} ret={:08x}",
                cpu.register(3).unwrap_or(0)
            );
        }
        match entry {
            entry::OS_VERSION => {
                cpu.set_register(4, OS_VERSION)?;
                self.return_register(cpu, entry)
            }
            entry::MODE_SET => {
                let mode = cpu.register(4).unwrap_or(0);
                bus.write_u32(bios_field::MODE_FIELD, mode)?;
                bus.write_u32(bios_field::MODE_FIELD_2, 0)?;
                bus.write_u32(bios_field::MODE_FLAG, 0)?;
                cpu.set_register(4, 0)?;
                self.return_register(cpu, entry)
            }
            entry::VBLANK_REGISTER => {
                let handler = cpu.register(4).unwrap_or(0);
                bus.write_u32(bios_field::VBLANK_HANDLER, handler)?;
                self.irq_handlers
                    .push((crate::PPU_INTERRUPT_SOURCE, handler));
                self.return_register(cpu, entry)
            }
            entry::HANDLER_REGISTER => {
                let handler = cpu.register(4).unwrap_or(0);
                bus.write_u32(bios_field::HANDLER_SLOT_2, handler)?;
                self.irq_handlers.push((TIMER_SOURCE, handler));
                self.return_register(cpu, entry)
            }
            entry::TICKS_GET | entry::TICKS_GET2 => {
                let ticks = bus.read_u32(bios_field::TICKS)?;
                cpu.set_register(4, ticks)?;
                self.return_register(cpu, entry)
            }
            entry::FILE_OPEN => {
                let path_pointer = cpu.register(4).unwrap_or(0);
                let handle = self.open_file(bus, disc, path_pointer);
                cpu.set_register(4, handle)?;
                self.return_register(cpu, entry)
            }
            entry::FILE_READ => {
                let handle = cpu.register(4).unwrap_or(0);
                let buffer = cpu.register(5).unwrap_or(0);
                let length = cpu.register(6).unwrap_or(0);
                let read = self.read_file(bus, disc, entry, handle, buffer, length)?;
                cpu.set_register(4, read)?;
                self.return_register(cpu, entry)
            }
            entry::FILE_CLOSE => {
                let handle = cpu.register(4).unwrap_or(0);
                self.close_file(entry, handle)?;
                cpu.set_register(4, 0)?;
                self.return_register(cpu, entry)
            }
            // File-info services return success with conservative values
            // until a title is observed to depend on richer results.
            entry::FILE_INFO => {
                cpu.set_register(4, 0)?;
                cpu.set_register(5, 2)?;
                self.return_register(cpu, entry)
            }
            // Frame-pacing delay: the measured body snapshots the delay
            // counter and sleeps until ten ticks have passed.
            entry::DELAY => {
                let saved = bus.read_u32(DELAY_TICKS)?;
                let return_addr = cpu.register(3).unwrap_or(0);
                if return_addr < GAME_IMAGE_START {
                    return Err(EmulatorError::Resident(ResidentError::BadReturn {
                        entry,
                        target: return_addr,
                    }));
                }
                self.blocked = Some(BlockedDelay { saved, return_addr });
                if self.log {
                    eprintln!(
                        "[delay] parked saved={saved} ret={return_addr:08x} frames={} psr={:08x}",
                        self.irq_frames.len(),
                        cpu.control_register(crate::cpu::CR_PSR).unwrap_or(0)
                    );
                }
                cpu.set_pc(BLOCKED_MAGIC);
                Ok(())
            }
            // Filesystem mode configuration: measured body stores the
            // context value and the clamped mode into firmware fields.
            entry::FS_CONFIGURE => {
                let mode = cpu.register(4).unwrap_or(0).clamp(1, 4);
                let value = cpu.register(5).unwrap_or(0);
                bus.write_u32(0xa004_12a0, value)?;
                bus.write_u32(0xa004_129c, mode)?;
                cpu.set_register(4, 0)?;
                self.return_register(cpu, entry)
            }
            // USB-controller configuration: the measured body stores 256
            // into the controller's register window.
            entry::USB_CONFIG => {
                bus.write_u32(0x081b_00dc, 256)?;
                cpu.set_register(4, 0)?;
                self.return_register(cpu, entry)
            }
            // Mode-field query: measured body returns 1 when the mode
            // field equals -1, else 0 (stack-convention return).
            entry::MODE_QUERY => {
                // The polled asynchronous operation completes instantly:
                // the firmware's deferred pump (counter increments plus a
                // completion write) has no native equivalent yet, so report
                // completion and let the title proceed.
                let _ = bus.write_u32(bios_field::MODE_FIELD_2, u32::MAX);
                let _ = bus.write_u32(bios_field::MODE_FIELD, u32::MAX);
                cpu.set_register(4, 1)?;
                self.return_auto(cpu, bus, entry)
            }
            // Critical-section pair: the measured bodies toggle a firmware
            // flag with waits; serving them immediately preserves the flag
            // semantics the games rely on.
            entry::CRITICAL_ENTER | entry::CRITICAL_LEAVE => {
                cpu.set_register(4, 0)?;
                self.return_register(cpu, entry)
            }
            // DMA setup: measured body writes the argument triple into the
            // MIU window plus a PPU control register.
            entry::DMA_SETUP => {
                let first = cpu.register(4).unwrap_or(0);
                let second = cpu.register(5).unwrap_or(0);
                let third = cpu.register(6).unwrap_or(0);
                bus.write_u32(0x0807_0044, first)?;
                bus.write_u32(0x0807_0048, second)?;
                bus.write_u32(0x0807_0064, third)?;
                bus.write_u32(0x0801_00c4, first)?;
                self.return_register(cpu, entry)
            }
            // Memory-control unlock: measured body pokes the system
            // configuration register and returns a status bit.
            entry::MEM_CONTROL => {
                let current = bus.read_u32(0x0820_0048)?;
                bus.write_u32(0x0820_0048, current & !0x0000_ffff | 0x0000_ff00)?;
                let current = bus.read_u32(0x0820_0048)?;
                bus.write_u32(0x0820_0048, current | 0x0000_00ff)?;
                let current = bus.read_u32(0x0820_0048)?;
                bus.write_u32(0x0820_0048, current & 0xffff_ff00)?;
                bus.write_u32(0x0820_0048, 0xff00_0000)?;
                let status = bus.read_u32(0x0820_0078)? & 1;
                cpu.set_register(4, status)?;
                self.return_register(cpu, entry)
            }
            // Filesystem status: measured returns are pointers into the
            // firmware data area plus a status word; the area exists in
            // guest DRAM (zeroed until a title's use is understood).
            entry::FS_STATUS => {
                cpu.set_register(4, 0xa004_903c)?;
                cpu.set_register(5, 2)?;
                cpu.set_register(6, 0xa004_9048)?;
                cpu.set_register(7, 0xa004_9044)?;
                self.return_register(cpu, entry)
            }
            entry::FILE_QUERY => {
                cpu.set_register(4, 1)?;
                self.return_register(cpu, entry)
            }
            entry::FRAME_TICK => {
                let ticks = bus.read_u32(bios_field::FRAME_TICKS)?;
                bus.write_u32(bios_field::FRAME_TICKS, ticks.wrapping_add(1))?;
                bus.write_u32(PPU_FRAME_SERVICE_REG, 1)?;
                self.return_via_stack(cpu, bus, entry)
            }
            other => {
                self.log_unhandled(other);
                Err(EmulatorError::Resident(
                    ResidentError::UnhandledResidentEntry { pc: other },
                ))
            }
        }
    }

    /// Saved program counters of the in-flight delivery frames.
    pub fn irq_frame_pcs(&self) -> Vec<u32> {
        self.irq_frames.iter().map(|frame| frame.pc).collect()
    }

    /// Whether any delivery frame is in flight.
    pub fn irq_frames(&self) -> bool {
        !self.irq_frames.is_empty()
    }

    /// Whether a blocking service call is parked.
    pub fn blocked_delay(&self) -> bool {
        self.blocked.is_some()
    }

    /// Wake check for a parked blocking delay: completed once ten delay
    /// ticks have passed since entry. Restores the return address and the
    /// measured return value.
    pub fn poll_blocked(&mut self, cpu: &mut Score7, bus: &mut Bus) -> bool {
        let Some(blocked) = self.blocked else {
            return false;
        };
        let now = bus.read_u32(DELAY_TICKS).unwrap_or(0);
        if self.log {
            static ONCE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let seen = ONCE.load(std::sync::atomic::Ordering::Relaxed);
            if now != seen {
                ONCE.store(now, std::sync::atomic::Ordering::Relaxed);
                eprintln!(
                    "[delay] parked poll: now={now} saved={} target=+{} carry={:x} psr={:08x}",
                    blocked.saved,
                    now.wrapping_sub(blocked.saved),
                    self.carry,
                    cpu.control_register(crate::cpu::CR_PSR).unwrap_or(0)
                );
            }
        }
        if now.wrapping_sub(blocked.saved) < DELAY_TICKS_PER_SLEEP {
            return false;
        }
        self.blocked = None;
        if self.log {
            eprintln!(
                "[delay] woke after {} ticks",
                now.wrapping_sub(blocked.saved)
            );
        }
        let _ = cpu.set_register(4, 0);
        let _ = cpu.set_register(28, FIRMWARE_CONTEXT);
        cpu.set_pc(blocked.return_addr);
        true
    }

    /// Interrupt sources deferred from earlier steps.
    pub fn carry(&self) -> u64 {
        self.carry
    }

    /// Store deferred interrupt sources for the next step.
    pub fn set_carry(&mut self, pending: u64) {
        self.carry = pending;
    }

    /// Whether native delivery may run now: the CPU only takes interrupts
    /// when the program-status word enables them, and the dispatcher keeps
    /// them disabled while a handler runs.
    pub fn delivery_allowed(&self, cpu: &Score7) -> bool {
        cpu.control_register(crate::cpu::CR_PSR)
            .is_some_and(|psr| psr & 1 != 0)
    }

    /// Deliver a device interrupt natively: call the registered handler with
    /// the firmware dispatcher's register conventions. Returns `false` when
    /// no handler is registered (the interrupt is dropped, matching the
    /// firmware's default path).
    pub fn deliver_irq(&mut self, cpu: &mut Score7, bus: &mut Bus, source: u8) -> bool {
        self.delivery_counts.entry(u32::from(source)).or_insert(0);
        *self.delivery_counts.get_mut(&u32::from(source)).unwrap() += 1;
        let count = self.delivery_counts[&u32::from(source)];
        if self.log && (count <= 5 || count % 5000 == 1) {
            eprintln!(
                "[irq-count] source={source} n={}",
                self.delivery_counts[&u32::from(source)]
            );
        }
        if source == TIMER_SOURCE {
            // Mirror the firmware timer ISR, which runs whether or not the
            // game registered a handler: acknowledge the overflow and
            // advance the counters the firmware and the delay service read.
            const TIMER_ACK: u32 = 0x8c00_0000;
            const TIMER1_CONTROL: u32 = 0x0816_1000;
            let _ = bus.write_u32(TIMER1_CONTROL, TIMER_ACK);
            const TICKS_PER_TIMER_IRQ: u32 = 1;
            let ticks = bus.read_u32(bios_field::TICKS).unwrap_or(0);
            let _ = bus.write_u32(bios_field::TICKS, ticks.wrapping_add(TICKS_PER_TIMER_IRQ));
            let delay_ticks = bus.read_u32(DELAY_TICKS).unwrap_or(0);
            let _ = bus.write_u32(DELAY_TICKS, delay_ticks.wrapping_add(1));
        }
        let Some(&handler) = self
            .irq_handlers
            .iter()
            .rev()
            .find(|(registered, _)| *registered == source)
            .map(|(_, handler)| handler)
        else {
            return false;
        };
        if handler < GAME_IMAGE_START {
            return false;
        }
        if self.log {
            let captured = cpu.register(0).unwrap_or(0);
            let stack_like = (0xa0f0_0000..0xa100_0000).contains(&captured)
                || captured == 0x2000_0000
                || captured == 0;
            if !stack_like {
                eprintln!(
                    "[irq-badr0] source={source} captured-r0={captured:08x} pc={:08x} depth={}",
                    cpu.pc(),
                    self.irq_frames.len()
                );
            }
        }
        let return_magic = if source == crate::PPU_INTERRUPT_SOURCE {
            VBLANK_CONTINUATION_MAGIC
        } else {
            IRQ_RETURN_MAGIC
        };
        self.irq_frames.push(IrqFrame::capture(cpu));
        if self.log {
            eprintln!(
                "[irq-frame] depth={} pc->{:08x} r3={:08x} psr={:08x}",
                self.irq_frames.len(),
                handler,
                return_magic,
                cpu.control_register(crate::cpu::CR_PSR).unwrap_or(0)
            );
        }
        let _ = cpu.set_register(3, return_magic);
        let _ = cpu.set_register(28, FIRMWARE_CONTEXT);
        let stack = cpu.register(0).unwrap_or(0x00ff_fff0);
        let _ = cpu.set_register(0, stack.wrapping_sub(0x100));
        let cause = cpu.control_register(crate::cpu::CR_ECR).unwrap_or(0);
        let _ = cpu.set_control_register(
            crate::cpu::CR_ECR,
            (cause & !0x00fc_0000) | (u32::from(source) << 18),
        );
        // Mirror the CPU's exception-entry flag shuffling.
        let psr = cpu.control_register(crate::cpu::CR_PSR).unwrap_or(0);
        let _ = cpu.set_control_register(crate::cpu::CR_PSR, (psr & !0x0f) | ((psr << 2) & 0x0c));
        let flags = cpu.control_register(crate::cpu::CR_FLAGS).unwrap_or(0);
        let _ = cpu.set_control_register(
            crate::cpu::CR_FLAGS,
            (flags & !0x03ff) | ((flags << 5) & 0x03e0),
        );
        cpu.set_pc(handler);
        if self.log {
            eprintln!("[irq-deliver] source={source} handler={handler:05x}");
        }
        true
    }

    /// Restore the interrupted state when a native handler returns. The
    /// VBlank continuation first performs the firmware's post-handler
    /// bookkeeping: the frame-tick increment and the PPU acknowledge.
    pub fn return_from_irq(&mut self, cpu: &mut Score7, bus: &mut Bus) {
        if self.log {
            eprintln!(
                "[irq-return] magic={:04x} depth={}",
                cpu.pc() & crate::ADDRESS_MASK,
                self.irq_frames.len()
            );
        }
        let vblank_continuation = cpu.pc() & crate::ADDRESS_MASK == VBLANK_CONTINUATION_MAGIC;
        if vblank_continuation {
            let ticks = bus.read_u32(bios_field::FRAME_TICKS).unwrap_or(0);
            let _ = bus.write_u32(bios_field::FRAME_TICKS, ticks.wrapping_add(1));
            let _ = bus.write_u32(PPU_FRAME_SERVICE_REG, 1);
            // The firmware sprite manager runs in the per-frame chain: it
            // copies the game's sprite list (0xa00476b0) into the PPU
            // sprite RAM window (0x8801_4000), bounded by the limit field
            // (measured: 14 words per frame at pc 0x15220/0x15244), and
            // publishes the hardware config (0x1ff at pc 0x15268).
            let limit = bus.read_u32(bios_field::SPRITE_LIST_LIMIT).unwrap_or(0);
            let words = limit.min(SPRITE_RAM_WORDS);
            for index in 0..words {
                let word = bus
                    .read_u32(bios_field::SPRITE_LIST + index * 4)
                    .unwrap_or(0);
                let _ = bus.write_u32(0x8801_4000 + index * 4, word);
            }
            let _ = bus.write_u32(SPRITE_CONFIG, SPRITE_CONFIG_VALUE);
        }
        if let Some(frame) = self.irq_frames.pop() {
            frame.restore(cpu);
        }
    }

    fn log_unhandled(&mut self, pc: u32) {
        if self.log && !self.unhandled_logged.contains(&pc) {
            self.unhandled_logged.push(pc);
            eprintln!("[svc-unhandled] pc={pc:04x}");
        }
    }

    /// Return from a register-convention service: `r3` holds the address.
    fn return_register(&self, cpu: &mut Score7, entry: u32) -> Result<(), EmulatorError> {
        let target = cpu.register(3).unwrap_or(0);
        let _ = cpu.set_register(28, FIRMWARE_CONTEXT);
        if target >= GAME_IMAGE_START {
            cpu.set_pc(target);
            Ok(())
        } else {
            Err(EmulatorError::Resident(ResidentError::BadReturn {
                entry,
                target,
            }))
        }
    }

    /// Return using whichever convention the caller used: a valid `r3`
    /// wins, otherwise the return address is read from the caller's stack
    /// slots (services whose epilogue pops both `r2` and `r3`).
    fn return_auto(
        &self,
        cpu: &mut Score7,
        bus: &mut Bus,
        entry: u32,
    ) -> Result<(), EmulatorError> {
        let register_target = cpu.register(3).unwrap_or(0);
        if register_target >= GAME_IMAGE_START {
            return self.return_register(cpu, entry);
        }
        self.return_via_stack(cpu, bus, entry)
    }

    /// Return from a stack-convention service (frame tick): the caller
    /// pushed `r2` and the return address above the entry stack pointer.
    fn return_via_stack(
        &self,
        cpu: &mut Score7,
        bus: &mut Bus,
        entry: u32,
    ) -> Result<(), EmulatorError> {
        let stack = cpu.register(0).unwrap_or(0);
        let saved_r2 = bus.read_u32(stack)?;
        let target = bus.read_u32(stack.wrapping_add(4))?;
        let _ = cpu.set_register(2, saved_r2);
        let _ = cpu.set_register(3, target);
        let _ = cpu.set_register(0, stack.wrapping_add(8));
        let _ = cpu.set_register(28, FIRMWARE_CONTEXT);
        if target >= GAME_IMAGE_START {
            cpu.set_pc(target);
            Ok(())
        } else {
            Err(EmulatorError::Resident(ResidentError::BadReturn {
                entry,
                target,
            }))
        }
    }

    fn open_file(&mut self, bus: &mut Bus, disc: Option<&DiscImage>, path_pointer: u32) -> u32 {
        let mut path = String::new();
        for offset in 0..260u32 {
            let Ok(byte) = bus.read_u8(path_pointer.wrapping_add(offset)) else {
                break;
            };
            if byte == 0 {
                break;
            }
            path.push(byte as char);
        }
        let normalized: String = path
            .chars()
            .map(|character| if character == '\\' { '/' } else { character })
            .collect();
        let opened = match disc {
            Some(disc) => match iso9660::find_file(disc, &normalized) {
                Ok(file) => Some(OpenFile {
                    lba: file.lba,
                    size: file.size,
                    pos: 0,
                }),
                Err(error) => {
                    if self.log {
                        eprintln!("[svc-file] find '{normalized}' error: {error}");
                    }
                    None
                }
            },
            None => None,
        };
        match opened {
            Some(file) => {
                let handle = (self.files.len() + 1) as u32;
                self.files.push(Some(file));
                if self.log {
                    eprintln!("[svc-file] open \"{path}\" -> {handle:04x}");
                }
                handle
            }
            None => {
                if self.log {
                    eprintln!("[svc-file] open \"{path}\" failed");
                }
                u32::MAX
            }
        }
    }

    fn read_file(
        &mut self,
        bus: &mut Bus,
        disc: Option<&DiscImage>,
        entry: u32,
        handle: u32,
        buffer: u32,
        length: u32,
    ) -> Result<u32, EmulatorError> {
        let Some(slot) = self.files.get_mut(handle.wrapping_sub(1) as usize) else {
            return Err(EmulatorError::Resident(ResidentError::BadFileHandle {
                entry,
                handle,
            }));
        };
        let Some(file) = slot.as_mut() else {
            return Err(EmulatorError::Resident(ResidentError::BadFileHandle {
                entry,
                handle,
            }));
        };
        let Some(disc) = disc else {
            return Ok(0);
        };
        let remaining = file.size - file.pos.min(file.size);
        let count = length.min(remaining);
        let mut copied = 0_u32;
        let mut chunk = Vec::new();
        while copied < count {
            let sector_offset = (file.pos + copied) % crate::USER_DATA_SIZE as u32;
            let space = crate::USER_DATA_SIZE as u32 - sector_offset;
            let take = space.min(count - copied);
            let lba = file.lba + (file.pos + copied) / crate::USER_DATA_SIZE as u32;
            let sector = disc.read_sector(lba)?;
            let start = sector_offset as usize;
            chunk.extend_from_slice(&sector[start..start + take as usize]);
            copied += take;
        }
        if !chunk.is_empty() {
            bus.load_dram(buffer, &chunk)?;
        }
        file.pos += copied;
        if self.log && copied > 0 {
            eprintln!(
                "[svc-file] read {handle:04x} {buffer:08x} {copied}/{length} (pos {} of {})",
                file.pos, file.size
            );
        }
        Ok(copied)
    }

    fn close_file(&mut self, entry: u32, handle: u32) -> Result<(), EmulatorError> {
        let slot =
            self.files
                .get_mut(handle.wrapping_sub(1) as usize)
                .ok_or(EmulatorError::Resident(ResidentError::BadFileHandle {
                    entry,
                    handle,
                }))?;
        if slot.is_none() {
            return Err(EmulatorError::Resident(ResidentError::BadFileHandle {
                entry,
                handle,
            }));
        }
        *slot = None;
        Ok(())
    }
}

/// What the emulator should do with a fetch inside the resident region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentFetch {
    /// A native service call.
    Service(u32),
    /// A native interrupt handler returning to the interrupted state.
    IrqReturn,
    /// A resident address with no native replacement (structured stop).
    Unhandled(u32),
}

impl std::fmt::Display for ResidentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnhandledResidentEntry { pc } => {
                write!(
                    formatter,
                    "no native replacement for resident entry 0x{pc:04x}"
                )
            }
            Self::BadReturn { entry, target } => {
                write!(
                    formatter,
                    "service 0x{entry:04x} returned to invalid 0x{target:08x}"
                )
            }
            Self::BadFileHandle { entry, handle } => {
                write!(
                    formatter,
                    "service 0x{entry:04x} used closed handle {handle:04x}"
                )
            }
        }
    }
}

impl std::error::Error for ResidentError {}
