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
            Self::ExecutionNotImplemented => {
                formatter.write_str("target execution is not implemented")
            }
        }
    }
}

impl Error for EmulatorError {}
