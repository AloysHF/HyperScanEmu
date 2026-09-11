use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmulatorError {
    InvalidFirmwareSize {
        component: &'static str,
        expected: usize,
        actual: usize,
    },
    BlankFirmware {
        component: &'static str,
    },
    MemoryFault {
        access: &'static str,
        address: u32,
        width: u8,
    },
    InvalidDisc(&'static str),
    InvalidDiscSector {
        lba: u32,
        reason: &'static str,
    },
    InvalidCardSize {
        expected: usize,
        actual: usize,
    },
    InvalidCdSectorSize {
        size: usize,
        maximum: usize,
    },
    UnsupportedCdAudio,
    DiscSectorOutOfRange {
        lba: u32,
        sector_count: u32,
    },
    InvalidRegister {
        index: usize,
    },
    InvalidControlRegister {
        index: usize,
    },
    InvalidInterruptSource {
        source: u8,
    },
    UnsupportedInstruction {
        pc: u32,
        instruction: u32,
        width: u8,
    },
    DivisionByZero {
        pc: u32,
    },
    UnknownMmio {
        access: &'static str,
        address: u32,
    },
    UnsupportedTimerMode {
        mode: u8,
    },
    ExecutionNotImplemented,
}

impl Display for EmulatorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidFirmwareSize {
                component,
                expected,
                actual,
            } => write!(
                formatter,
                "invalid {component} size: expected {expected} bytes, got {actual}"
            ),
            Self::BlankFirmware { component } => {
                write!(formatter, "{component} is a uniform blank dump")
            }
            Self::MemoryFault {
                access,
                address,
                width,
            } => write!(
                formatter,
                "invalid {access} of {width} byte(s) at 0x{address:08x}"
            ),
            Self::InvalidDisc(reason) => write!(formatter, "invalid disc image: {reason}"),
            Self::InvalidDiscSector { lba, reason } => {
                write!(formatter, "invalid disc sector at LBA {lba}: {reason}")
            }
            Self::DiscSectorOutOfRange { lba, sector_count } => write!(
                formatter,
                "disc sector LBA {lba} is outside the {sector_count}-sector image"
            ),
            Self::InvalidCardSize { expected, actual } => {
                write!(
                    formatter,
                    "invalid card size: expected {expected} bytes, got {actual}"
                )
            }
            Self::InvalidCdSectorSize { size, maximum } => {
                write!(
                    formatter,
                    "invalid CD sector size {size}: maximum is {maximum}"
                )
            }
            Self::UnsupportedCdAudio => formatter.write_str("CD audio transfer is not implemented"),
            Self::InvalidRegister { index } => write!(formatter, "invalid CPU register r{index}"),
            Self::InvalidControlRegister { index } => {
                write!(formatter, "invalid CPU control register cr{index}")
            }
            Self::InvalidInterruptSource { source } => {
                write!(formatter, "invalid interrupt source {source}")
            }
            Self::UnsupportedInstruction {
                pc,
                instruction,
                width,
            } => write!(
                formatter,
                "unsupported {width}-byte instruction 0x{instruction:08x} at 0x{pc:08x}"
            ),
            Self::DivisionByZero { pc } => {
                write!(formatter, "division by zero at 0x{pc:08x}")
            }
            Self::UnknownMmio { access, address } => {
                write!(formatter, "unknown MMIO {access} at 0x{address:08x}")
            }
            Self::UnsupportedTimerMode { mode } => {
                write!(formatter, "unsupported timer mode {mode}")
            }
            Self::ExecutionNotImplemented => {
                formatter.write_str("target execution is not implemented")
            }
        }
    }
}

impl Error for EmulatorError {}
