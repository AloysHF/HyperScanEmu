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
    DiscSectorOutOfRange {
        lba: u32,
        sector_count: u32,
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
            Self::ExecutionNotImplemented => {
                formatter.write_str("target execution is not implemented")
            }
        }
    }
}

impl Error for EmulatorError {}
