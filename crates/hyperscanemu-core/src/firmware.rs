use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};

use crate::EmulatorError;

pub const INTERNAL_ROM_SIZE: usize = 0x8_000;
pub const BIOS_ROM_SIZE: usize = 0x10_0000;

/// Folder below the frontend system directory that holds both firmware files.
pub const FIRMWARE_DIR: &str = "hyperscanemu";
/// File names are matched byte-for-byte inside the fixed folder.
pub const INTERNAL_ROM_NAME: &str = "spg290.bin";
pub const BIOS_ROM_NAME: &str = "hyperscan.bin";

/// Why the fixed firmware set could not be loaded.
#[derive(Debug)]
pub enum FirmwareLoadError {
    /// `<system>/hyperscanemu` does not exist or is not a directory.
    DirectoryNotFound(PathBuf),
    /// The folder exists but does not contain every required file.
    MissingFiles {
        directory: PathBuf,
        names: Vec<&'static str>,
    },
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Invalid(EmulatorError),
}

impl Display for FirmwareLoadError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DirectoryNotFound(path) => write!(
                formatter,
                "firmware directory not found: {}",
                path.display()
            ),
            Self::MissingFiles { directory, names } => write!(
                formatter,
                "firmware is incomplete in {}: missing {}",
                directory.display(),
                names.join(", ")
            ),
            Self::Io { path, source } => {
                write!(formatter, "cannot read {}: {source}", path.display())
            }
            Self::Invalid(error) => error.fmt(formatter),
        }
    }
}

impl Error for FirmwareLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            Self::Invalid(error) => Some(error),
            _ => None,
        }
    }
}

impl From<EmulatorError> for FirmwareLoadError {
    fn from(error: EmulatorError) -> Self {
        Self::Invalid(error)
    }
}

/// Return the entry of `dir` whose name equals `name` exactly.
///
/// Directory entries are compared instead of opening `dir/name` directly
/// because Windows would match names that differ only in case.
pub fn exact_child(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|entry| entry.file_name().to_str() == Some(name))
        .map(|entry| entry.path())
}

/// Fixed firmware folder below the frontend system directory.
pub fn firmware_directory(system_dir: &Path) -> Option<PathBuf> {
    exact_child(system_dir, FIRMWARE_DIR).filter(|path| path.is_dir())
}

/// Load `hyperscanemu/spg290.bin` and `hyperscanemu/hyperscan.bin`.
pub fn load(system_dir: &Path) -> Result<Firmware, FirmwareLoadError> {
    let directory = firmware_directory(system_dir)
        .ok_or_else(|| FirmwareLoadError::DirectoryNotFound(system_dir.join(FIRMWARE_DIR)))?;

    let mut missing = Vec::new();
    let mut path_of = |name: &'static str| -> Option<PathBuf> {
        match exact_child(&directory, name).filter(|path| path.is_file()) {
            Some(path) => Some(path),
            None => {
                missing.push(name);
                None
            }
        }
    };
    let internal_path = path_of(INTERNAL_ROM_NAME);
    let bios_path = path_of(BIOS_ROM_NAME);
    if !missing.is_empty() {
        return Err(FirmwareLoadError::MissingFiles {
            directory,
            names: missing,
        });
    }

    let internal = read(&internal_path.expect("checked above"))?;
    let bios = read(&bios_path.expect("checked above"))?;
    Ok(Firmware::from_parts(&internal, &bios)?)
}

fn read(path: &Path) -> Result<Vec<u8>, FirmwareLoadError> {
    std::fs::read(path).map_err(|source| FirmwareLoadError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Firmware {
    internal_rom: Box<[u8]>,
    bios_rom: Box<[u8]>,
    fingerprint: u64,
}

impl Firmware {
    pub fn from_parts(internal_rom: &[u8], bios_rom: &[u8]) -> Result<Self, EmulatorError> {
        validate_component("internal ROM", internal_rom, INTERNAL_ROM_SIZE)?;
        validate_component("BIOS ROM", bios_rom, BIOS_ROM_SIZE)?;

        let mut fingerprint = FNV_OFFSET_BASIS;
        fingerprint = fingerprint_bytes(fingerprint, internal_rom);
        fingerprint = fingerprint_bytes(fingerprint, bios_rom);

        Ok(Self {
            internal_rom: internal_rom.into(),
            bios_rom: bios_rom.into(),
            fingerprint,
        })
    }

    pub fn internal_rom(&self) -> &[u8] {
        &self.internal_rom
    }

    pub fn bios_rom(&self) -> &[u8] {
        &self.bios_rom
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

fn validate_component(
    component: &'static str,
    bytes: &[u8],
    expected: usize,
) -> Result<(), EmulatorError> {
    if bytes.len() != expected {
        return Err(EmulatorError::InvalidFirmwareSize {
            component,
            expected,
            actual: bytes.len(),
        });
    }

    if bytes.iter().all(|byte| *byte == bytes[0]) {
        return Err(EmulatorError::BlankFirmware { component });
    }

    Ok(())
}

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;

fn fingerprint_bytes(initial: u64, bytes: &[u8]) -> u64 {
    bytes.iter().fold(initial, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_component(size: usize, marker: u8) -> Vec<u8> {
        let mut bytes = vec![0xff; size];
        bytes[size / 2] = marker;
        bytes
    }

    #[test]
    fn accepts_exact_non_blank_components() {
        let internal = valid_component(INTERNAL_ROM_SIZE, 0x42);
        let bios = valid_component(BIOS_ROM_SIZE, 0x24);
        let firmware = Firmware::from_parts(&internal, &bios).unwrap();

        assert_eq!(firmware.internal_rom(), internal);
        assert_eq!(firmware.bios_rom(), bios);
        assert_ne!(firmware.fingerprint(), 0);
    }

    #[test]
    fn rejects_wrong_component_size() {
        let internal = valid_component(INTERNAL_ROM_SIZE - 1, 0x42);
        let bios = valid_component(BIOS_ROM_SIZE, 0x24);

        assert_eq!(
            Firmware::from_parts(&internal, &bios),
            Err(EmulatorError::InvalidFirmwareSize {
                component: "internal ROM",
                expected: INTERNAL_ROM_SIZE,
                actual: INTERNAL_ROM_SIZE - 1,
            })
        );
    }

    #[test]
    fn rejects_uniform_dump() {
        let internal = vec![0xff; INTERNAL_ROM_SIZE];
        let bios = valid_component(BIOS_ROM_SIZE, 0x24);

        assert_eq!(
            Firmware::from_parts(&internal, &bios),
            Err(EmulatorError::BlankFirmware {
                component: "internal ROM",
            })
        );
    }

    #[test]
    fn fingerprint_covers_both_components() {
        let internal = valid_component(INTERNAL_ROM_SIZE, 0x42);
        let bios_a = valid_component(BIOS_ROM_SIZE, 0x24);
        let bios_b = valid_component(BIOS_ROM_SIZE, 0x25);

        let a = Firmware::from_parts(&internal, &bios_a).unwrap();
        let b = Firmware::from_parts(&internal, &bios_b).unwrap();

        assert_ne!(a.fingerprint(), b.fingerprint());
    }

    #[test]
    fn loads_the_fixed_directory_and_names() {
        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(FIRMWARE_DIR);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join(INTERNAL_ROM_NAME),
            valid_component(INTERNAL_ROM_SIZE, 0x42),
        )
        .unwrap();
        std::fs::write(
            directory.join(BIOS_ROM_NAME),
            valid_component(BIOS_ROM_SIZE, 0x24),
        )
        .unwrap();
        std::fs::write(directory.join("notes.txt"), b"ignored").unwrap();

        let firmware = load(root.path()).unwrap();
        assert_eq!(firmware.internal_rom()[INTERNAL_ROM_SIZE / 2], 0x42);
        assert_eq!(firmware.bios_rom()[BIOS_ROM_SIZE / 2], 0x24);
    }

    #[test]
    fn rejects_a_folder_or_file_name_with_the_wrong_case() {
        let root = tempfile::tempdir().unwrap();
        let wrong = root.path().join("HyperScanEmu");
        std::fs::create_dir_all(&wrong).unwrap();
        std::fs::write(
            wrong.join(INTERNAL_ROM_NAME),
            valid_component(INTERNAL_ROM_SIZE, 1),
        )
        .unwrap();
        std::fs::write(wrong.join(BIOS_ROM_NAME), valid_component(BIOS_ROM_SIZE, 2)).unwrap();

        assert!(matches!(
            load(root.path()),
            Err(FirmwareLoadError::DirectoryNotFound(_))
        ));

        let root = tempfile::tempdir().unwrap();
        let directory = root.path().join(FIRMWARE_DIR);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(
            directory.join("SPG290.BIN"),
            valid_component(INTERNAL_ROM_SIZE, 1),
        )
        .unwrap();
        std::fs::write(
            directory.join(BIOS_ROM_NAME),
            valid_component(BIOS_ROM_SIZE, 2),
        )
        .unwrap();

        let error = load(root.path()).unwrap_err();
        assert!(error.to_string().contains("spg290.bin"), "{error}");
    }

    #[test]
    fn reports_every_missing_file() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join(FIRMWARE_DIR)).unwrap();

        let error = load(root.path()).unwrap_err();
        let message = error.to_string();
        assert!(message.contains(INTERNAL_ROM_NAME), "{message}");
        assert!(message.contains(BIOS_ROM_NAME), "{message}");
    }
}
