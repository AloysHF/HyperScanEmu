use crate::EmulatorError;

pub const RAW_SECTOR_SIZE: usize = 2352;
pub const USER_DATA_SIZE: usize = 2048;
const USER_DATA_OFFSET: usize = 16;
const MAX_DISC_SIZE: usize = 900 * 1024 * 1024;
const MIN_DESCRIPTOR_SECTORS: usize = 22;
const SYNC_PATTERN: [u8; 12] = [
    0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x00,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscImage {
    raw_sectors: Box<[u8]>,
    sector_count: u32,
    fingerprint: u64,
}

impl DiscImage {
    pub fn from_mode1_2352(raw_sectors: Vec<u8>) -> Result<Self, EmulatorError> {
        if raw_sectors.len() > MAX_DISC_SIZE {
            return Err(EmulatorError::InvalidDisc("image exceeds the size limit"));
        }
        if !raw_sectors.len().is_multiple_of(RAW_SECTOR_SIZE) {
            return Err(EmulatorError::InvalidDisc(
                "image size is not a multiple of 2352 bytes",
            ));
        }
        if raw_sectors.len() < MIN_DESCRIPTOR_SECTORS * RAW_SECTOR_SIZE {
            return Err(EmulatorError::InvalidDisc(
                "image is too small to contain volume descriptors",
            ));
        }

        for (lba, sector) in raw_sectors
            .as_chunks::<RAW_SECTOR_SIZE>()
            .0
            .iter()
            .enumerate()
        {
            validate_sector(lba as u32, sector)?;
        }
        validate_volume_descriptors(&raw_sectors)?;

        let sector_count = (raw_sectors.len() / RAW_SECTOR_SIZE) as u32;
        let fingerprint = fingerprint_bytes(&raw_sectors);
        Ok(Self {
            raw_sectors: raw_sectors.into_boxed_slice(),
            sector_count,
            fingerprint,
        })
    }

    pub fn sector_count(&self) -> u32 {
        self.sector_count
    }

    pub fn fingerprint(&self) -> u64 {
        self.fingerprint
    }

    pub fn read_sector(&self, lba: u32) -> Result<&[u8], EmulatorError> {
        if lba >= self.sector_count {
            return Err(EmulatorError::DiscSectorOutOfRange {
                lba,
                sector_count: self.sector_count,
            });
        }

        let start = lba as usize * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
        Ok(&self.raw_sectors[start..start + USER_DATA_SIZE])
    }
}

fn validate_sector(lba: u32, sector: &[u8]) -> Result<(), EmulatorError> {
    if sector[..SYNC_PATTERN.len()] != SYNC_PATTERN {
        return Err(EmulatorError::InvalidDiscSector {
            lba,
            reason: "invalid sync pattern",
        });
    }
    if sector[15] != 1 {
        return Err(EmulatorError::InvalidDiscSector {
            lba,
            reason: "track is not MODE1/2352",
        });
    }

    let absolute_frame = lba + 150;
    let expected = [
        to_bcd(absolute_frame / (60 * 75)),
        to_bcd((absolute_frame / 75) % 60),
        to_bcd(absolute_frame % 75),
    ];
    if sector[12..15] != expected {
        return Err(EmulatorError::InvalidDiscSector {
            lba,
            reason: "invalid absolute MSF address",
        });
    }

    Ok(())
}

fn validate_volume_descriptors(raw: &[u8]) -> Result<(), EmulatorError> {
    let descriptor = |lba: usize| {
        let start = lba * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
        &raw[start..start + USER_DATA_SIZE]
    };

    let primary = descriptor(16);
    if primary[0] != 1 || &primary[1..6] != b"CD001" || primary[6] != 1 {
        return Err(EmulatorError::InvalidDisc(
            "missing ISO 9660 primary volume descriptor at LBA 16",
        ));
    }

    let scan_end = (raw.len() / RAW_SECTOR_SIZE).min(32);
    let mut terminator = false;
    let mut udf_bridge = false;
    for lba in 17..scan_end {
        let data = descriptor(lba);
        terminator |= data[0] == 0xff && &data[1..6] == b"CD001" && data[6] == 1;
        udf_bridge |=
            data[0] == 0 && (&data[1..6] == b"NSR02" || &data[1..6] == b"NSR03") && data[6] == 1;
    }
    if !terminator {
        return Err(EmulatorError::InvalidDisc(
            "missing ISO 9660 volume descriptor terminator",
        ));
    }
    if !udf_bridge {
        return Err(EmulatorError::InvalidDisc(
            "missing UDF volume recognition descriptor",
        ));
    }

    Ok(())
}

fn to_bcd(value: u32) -> u8 {
    ((value / 10) << 4 | (value % 10)) as u8
}

fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x100_0000_01b3;
    bytes.iter().fold(OFFSET_BASIS, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> Vec<u8> {
        let mut image = vec![0; MIN_DESCRIPTOR_SECTORS * RAW_SECTOR_SIZE];
        for (lba, sector) in image
            .as_chunks_mut::<RAW_SECTOR_SIZE>()
            .0
            .iter_mut()
            .enumerate()
        {
            sector[..12].copy_from_slice(&SYNC_PATTERN);
            let absolute_frame = lba as u32 + 150;
            sector[12] = to_bcd(absolute_frame / (60 * 75));
            sector[13] = to_bcd((absolute_frame / 75) % 60);
            sector[14] = to_bcd(absolute_frame % 75);
            sector[15] = 1;
        }

        let descriptor = |image: &mut [u8], lba: usize, kind: u8, id: &[u8; 5]| {
            let start = lba * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
            image[start] = kind;
            image[start + 1..start + 6].copy_from_slice(id);
            image[start + 6] = 1;
        };
        descriptor(&mut image, 16, 1, b"CD001");
        descriptor(&mut image, 17, 0xff, b"CD001");
        descriptor(&mut image, 18, 0, b"BEA01");
        descriptor(&mut image, 19, 0, b"NSR02");
        descriptor(&mut image, 20, 0, b"TEA01");
        image
    }

    #[test]
    fn accepts_mode1_iso_udf_bridge_image() {
        let image = DiscImage::from_mode1_2352(test_image()).unwrap();

        assert_eq!(image.sector_count(), MIN_DESCRIPTOR_SECTORS as u32);
        assert_eq!(&image.read_sector(16).unwrap()[1..6], b"CD001");
        assert_ne!(image.fingerprint(), 0);
    }

    #[test]
    fn rejects_truncated_sector() {
        let mut bytes = test_image();
        bytes.pop();

        assert_eq!(
            DiscImage::from_mode1_2352(bytes),
            Err(EmulatorError::InvalidDisc(
                "image size is not a multiple of 2352 bytes"
            ))
        );
    }

    #[test]
    fn rejects_bad_sync_and_msf() {
        let mut bad_sync = test_image();
        bad_sync[RAW_SECTOR_SIZE] = 1;
        assert_eq!(
            DiscImage::from_mode1_2352(bad_sync),
            Err(EmulatorError::InvalidDiscSector {
                lba: 1,
                reason: "invalid sync pattern",
            })
        );

        let mut bad_msf = test_image();
        bad_msf[RAW_SECTOR_SIZE + 14] = 0;
        assert_eq!(
            DiscImage::from_mode1_2352(bad_msf),
            Err(EmulatorError::InvalidDiscSector {
                lba: 1,
                reason: "invalid absolute MSF address",
            })
        );
    }

    #[test]
    fn rejects_missing_volume_descriptors() {
        let mut bytes = test_image();
        let start = 16 * RAW_SECTOR_SIZE + USER_DATA_OFFSET;
        bytes[start + 1] = b'X';

        assert_eq!(
            DiscImage::from_mode1_2352(bytes),
            Err(EmulatorError::InvalidDisc(
                "missing ISO 9660 primary volume descriptor at LBA 16"
            ))
        );
    }

    #[test]
    fn rejects_out_of_range_sector_read() {
        let image = DiscImage::from_mode1_2352(test_image()).unwrap();

        assert_eq!(
            image.read_sector(MIN_DESCRIPTOR_SECTORS as u32),
            Err(EmulatorError::DiscSectorOutOfRange {
                lba: MIN_DESCRIPTOR_SECTORS as u32,
                sector_count: MIN_DESCRIPTOR_SECTORS as u32,
            })
        );
    }
}
