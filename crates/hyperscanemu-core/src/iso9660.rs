//! Minimal ISO 9660 reader used to locate the game executable on a disc
//! image. The disc loader only validates the volume descriptors; this module
//! walks the directory tree and reads file extents so the HLE boot path can
//! load HYPER.EXE without firmware.

use crate::{DiscImage, EmulatorError, USER_DATA_SIZE};

const PRIMARY_VOLUME_DESCRIPTOR_LBA: u32 = 16;
const ROOT_DIRECTORY_OFFSET: usize = 156;
const RECORD_FIXED_HEADER: usize = 33;
const RECORD_LENGTH_FIELD: usize = 0;
const RECORD_LBA_FIELD: usize = 2;
const RECORD_SIZE_FIELD: usize = 10;
const RECORD_FLAGS_FIELD: usize = 25;
const RECORD_NAME_LENGTH_FIELD: usize = 32;
const DIRECTORY_FLAG: u8 = 1;

/// Location of a file on the disc image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscFile {
    pub lba: u32,
    pub size: u32,
}

#[derive(Debug)]
struct DirectoryRecord {
    lba: u32,
    size: u32,
    is_directory: bool,
    name: Vec<u8>,
}

/// Locate a file by its slash-separated path, starting at the volume root
/// directory. Component matching is case-insensitive and ignores the ISO 9660
/// version suffix (`HYPER.EXE;1` matches `hyper.exe`).
pub fn find_file(disc: &DiscImage, path: &str) -> Result<DiscFile, EmulatorError> {
    let primary = disc.read_sector(PRIMARY_VOLUME_DESCRIPTOR_LBA)?;
    let primary: &[u8; USER_DATA_SIZE] = primary
        .try_into()
        .map_err(|_| EmulatorError::InvalidDisc("primary volume descriptor is truncated"))?;
    let root = parse_record(&primary[ROOT_DIRECTORY_OFFSET..])?.ok_or(
        EmulatorError::InvalidDisc("the primary volume descriptor has no root directory record"),
    )?;

    let mut directory = root;
    let mut components = path
        .split('/')
        .filter(|component| !component.is_empty())
        .peekable();
    while let Some(component) = components.next() {
        let last = components.peek().is_none();
        let record = find_directory_entry(disc, &directory, component)?;
        if last {
            if record.is_directory {
                return Err(EmulatorError::InvalidDisc(
                    "the requested disc path is a directory, not a file",
                ));
            }
            return Ok(DiscFile {
                lba: record.lba,
                size: record.size,
            });
        }
        if !record.is_directory {
            return Err(EmulatorError::InvalidDisc(
                "a non-final disc path component is not a directory",
            ));
        }
        directory = record;
    }
    Err(EmulatorError::InvalidDisc("the disc path is empty"))
}

/// Read the user data of a located file extent.
pub fn read_file(disc: &DiscImage, file: DiscFile) -> Result<Vec<u8>, EmulatorError> {
    let sector_count = file.size.div_ceil(USER_DATA_SIZE as u32);
    let mut data = Vec::with_capacity(file.size as usize);
    for index in 0..sector_count {
        let sector = disc.read_sector(file.lba + index)?;
        data.extend_from_slice(sector);
    }
    data.truncate(file.size as usize);
    Ok(data)
}

/// Scan one directory extent for a component. Returns `Ok(None)` when the
/// directory has no matching entry.
fn find_directory_entry(
    disc: &DiscImage,
    directory: &DirectoryRecord,
    component: &str,
) -> Result<DirectoryRecord, EmulatorError> {
    let sector_count = directory.size.div_ceil(USER_DATA_SIZE as u32);
    for sector_index in 0..sector_count {
        let sector = disc.read_sector(directory.lba + sector_index)?;
        let mut offset = 0usize;
        while offset < USER_DATA_SIZE {
            let remaining = &sector[offset..];
            match parse_record(remaining)? {
                Some(record) => {
                    let length = remaining[RECORD_LENGTH_FIELD] as usize;
                    if name_matches(&record.name, component) {
                        return Ok(record);
                    }
                    offset += length;
                }
                // A zero length marks the padding to the next sector.
                None => offset = (offset / USER_DATA_SIZE + 1) * USER_DATA_SIZE,
            }
        }
    }
    Err(EmulatorError::InvalidDisc(
        "the disc directory has no such entry",
    ))
}

/// Parse one directory record. Returns `Ok(None)` for the zero-length
/// end-of-sector marker and errors on impossible records.
fn parse_record(data: &[u8]) -> Result<Option<DirectoryRecord>, EmulatorError> {
    if data.len() < RECORD_FIXED_HEADER {
        return Err(EmulatorError::InvalidDisc(
            "a disc directory record is truncated",
        ));
    }
    let length = data[RECORD_LENGTH_FIELD] as usize;
    if length == 0 {
        return Ok(None);
    }
    if length < RECORD_FIXED_HEADER || length > data.len() {
        return Err(EmulatorError::InvalidDisc(
            "a disc directory record exceeds its sector",
        ));
    }
    let name_length = data[RECORD_NAME_LENGTH_FIELD] as usize;
    if RECORD_FIXED_HEADER + name_length > length {
        return Err(EmulatorError::InvalidDisc(
            "a disc directory record name is truncated",
        ));
    }
    Ok(Some(DirectoryRecord {
        lba: u32::from_le_bytes(
            data[RECORD_LBA_FIELD..RECORD_LBA_FIELD + 4]
                .try_into()
                .expect("checked above"),
        ),
        size: u32::from_le_bytes(
            data[RECORD_SIZE_FIELD..RECORD_SIZE_FIELD + 4]
                .try_into()
                .expect("checked above"),
        ),
        is_directory: data[RECORD_FLAGS_FIELD] & DIRECTORY_FLAG != 0,
        name: data[RECORD_FIXED_HEADER..RECORD_FIXED_HEADER + name_length].to_vec(),
    }))
}

/// Case-insensitive comparison that drops the `;version` suffix. The two
/// single-byte names `\x00` and `\x01` are the dot entries.
fn name_matches(name: &[u8], component: &str) -> bool {
    let base = match name.iter().position(|byte| *byte == b';') {
        Some(position) => &name[..position],
        None => name,
    };
    let component = component.as_bytes();
    base.eq_ignore_ascii_case(component)
}

#[cfg(test)]
pub(crate) mod test_fixtures {
    use crate::{DiscImage, RAW_SECTOR_SIZE, USER_DATA_SIZE};

    const USER_DATA_OFFSET: usize = 16;
    const SYNC_PATTERN: [u8; 12] = [
        0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00,
    ];
    const ROOT_LBA: usize = 22;
    const FIRST_FILE_LBA: usize = 23;

    fn to_bcd(value: u32) -> u8 {
        ((value / 10) << 4 | (value % 10)) as u8
    }

    fn write_mode1_header(image: &mut [u8], lba: usize) {
        let sector = &mut image[lba * RAW_SECTOR_SIZE..(lba + 1) * RAW_SECTOR_SIZE];
        sector[..12].copy_from_slice(&SYNC_PATTERN);
        let absolute_frame = lba as u32 + 150;
        sector[12] = to_bcd(absolute_frame / (60 * 75));
        sector[13] = to_bcd((absolute_frame / 75) % 60);
        sector[14] = to_bcd(absolute_frame % 75);
        sector[15] = 1;
    }

    fn descriptor(image: &mut [u8], lba: usize, kind: u8, id: &[u8; 5]) {
        let start = lba * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
        image[start] = kind;
        image[start + 1..start + 6].copy_from_slice(id);
        image[start + 6] = 1;
    }

    fn directory_record(lba: u32, size: u32, flags: u8, name: &[u8]) -> Vec<u8> {
        let unpadded = 33 + name.len();
        let length = unpadded + (unpadded % 2);
        let mut record = vec![0_u8; length];
        record[0] = length as u8;
        record[2..6].copy_from_slice(&lba.to_le_bytes());
        record[10..14].copy_from_slice(&size.to_le_bytes());
        record[25] = flags;
        record[32] = name.len() as u8;
        record[33..33 + name.len()].copy_from_slice(name);
        record
    }

    /// MODE1/2352 image with an ISO 9660 volume and a root directory holding
    /// `files` in order, laid out like the retail discs (PVD, terminator, UDF
    /// bridge descriptors).
    pub(crate) fn synthetic_disc(files: &[(&str, &[u8])]) -> DiscImage {
        let file_sectors: usize = files
            .iter()
            .map(|(_, data)| data.len().div_ceil(USER_DATA_SIZE))
            .sum();
        let sector_count = FIRST_FILE_LBA + file_sectors.max(1);
        let mut image = vec![0_u8; sector_count * RAW_SECTOR_SIZE];
        for lba in 0..sector_count {
            write_mode1_header(&mut image, lba);
        }
        descriptor(&mut image, 16, 1, b"CD001");
        descriptor(&mut image, 17, 0xff, b"CD001");
        descriptor(&mut image, 18, 0, b"BEA01");
        descriptor(&mut image, 19, 0, b"NSR02");
        descriptor(&mut image, 20, 0, b"TEA01");

        // Primary volume descriptor: root directory record at offset 156.
        let root_record = directory_record(ROOT_LBA as u32, USER_DATA_SIZE as u32, 2, b"\x00");
        let pvd = 16 * RAW_SECTOR_SIZE + USER_DATA_OFFSET + 156;
        image[pvd..pvd + root_record.len()].copy_from_slice(&root_record);

        // Root directory: dot entries followed by one record per file.
        let mut directory: Vec<u8> = Vec::new();
        directory.extend_from_slice(&directory_record(
            ROOT_LBA as u32,
            USER_DATA_SIZE as u32,
            2,
            b"\x00",
        ));
        directory.extend_from_slice(&directory_record(
            ROOT_LBA as u32,
            USER_DATA_SIZE as u32,
            2,
            b"\x01",
        ));
        let mut file_lba = FIRST_FILE_LBA as u32;
        for (name, data) in files {
            directory.extend_from_slice(&directory_record(
                file_lba,
                data.len() as u32,
                0,
                name.as_bytes(),
            ));
            file_lba += data.len().div_ceil(USER_DATA_SIZE) as u32;
        }
        let directory_start = ROOT_LBA * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
        image[directory_start..directory_start + directory.len()].copy_from_slice(&directory);

        // File extents.
        let mut file_lba = FIRST_FILE_LBA;
        for (_, data) in files {
            let start = file_lba * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
            image[start..start + data.len()].copy_from_slice(data);
            file_lba += data.len().div_ceil(USER_DATA_SIZE);
        }

        DiscImage::from_mode1_2352(image).expect("fixture builds a valid disc")
    }
}

#[cfg(test)]
mod tests {
    use super::test_fixtures::synthetic_disc;
    use super::*;
    use crate::EmulatorError;

    #[test]
    fn finds_and_reads_a_root_file_case_insensitively() {
        let data: Vec<u8> = (0..40_u32).map(|index| index as u8).collect();
        let disc = synthetic_disc(&[("HYPER.EXE;1", &data)]);

        let file = find_file(&disc, "hyper.exe").unwrap();
        assert_eq!(file.size, data.len() as u32);
        assert_eq!(read_file(&disc, file).unwrap(), data);
    }

    #[test]
    fn reports_a_missing_entry_as_an_invalid_disc() {
        let disc = synthetic_disc(&[("OTHER.BIN", &[1, 2, 3])]);

        assert_eq!(
            find_file(&disc, "HYPER.EXE"),
            Err(EmulatorError::InvalidDisc(
                "the disc directory has no such entry"
            ))
        );
    }
}
