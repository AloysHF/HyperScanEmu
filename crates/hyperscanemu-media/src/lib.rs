use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use hyperscanemu_core::{DiscImage, EmulatorError};
use zip::ZipArchive;

const MAX_DISC_SIZE: u64 = 900 * 1024 * 1024;
const MAX_CUE_SIZE: u64 = 64 * 1024;

#[derive(Debug)]
pub enum MediaError {
    Io(io::Error),
    Zip(zip::result::ZipError),
    Core(EmulatorError),
    InvalidCue(&'static str),
    UnsupportedExtension,
}

impl Display for MediaError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "media I/O error: {error}"),
            Self::Zip(error) => write!(formatter, "invalid ZIP archive: {error}"),
            Self::Core(error) => Display::fmt(error, formatter),
            Self::InvalidCue(reason) => write!(formatter, "invalid CUE sheet: {reason}"),
            Self::UnsupportedExtension => formatter.write_str("unsupported media extension"),
        }
    }
}

impl Error for MediaError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Zip(error) => Some(error),
            Self::Core(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for MediaError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<zip::result::ZipError> for MediaError {
    fn from(error: zip::result::ZipError) -> Self {
        Self::Zip(error)
    }
}

impl From<EmulatorError> for MediaError {
    fn from(error: EmulatorError) -> Self {
        Self::Core(error)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CueSheet {
    track_file: PathBuf,
}

impl CueSheet {
    pub fn parse(text: &str) -> Result<Self, MediaError> {
        let mut track_file = None;
        let mut track_count = 0;
        let mut index_count = 0;
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            let upper = line.to_ascii_uppercase();
            if upper.starts_with("FILE ") {
                if track_file.is_some() {
                    return Err(MediaError::InvalidCue("multiple FILE entries"));
                }
                let (name, kind) = parse_file_line(line)?;
                if !kind.eq_ignore_ascii_case("BINARY") {
                    return Err(MediaError::InvalidCue("track file is not BINARY"));
                }
                let path = PathBuf::from(name);
                validate_relative_path(&path)?;
                track_file = Some(path);
            } else if upper.starts_with("TRACK ") {
                track_count += 1;
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.len() != 3
                    || fields[1] != "01"
                    || !fields[2].eq_ignore_ascii_case("MODE1/2352")
                {
                    return Err(MediaError::InvalidCue(
                        "only TRACK 01 MODE1/2352 is supported",
                    ));
                }
            } else if upper.starts_with("INDEX ") {
                index_count += 1;
                let fields: Vec<_> = line.split_whitespace().collect();
                if fields.as_slice() != ["INDEX", "01", "00:00:00"] {
                    return Err(MediaError::InvalidCue(
                        "only INDEX 01 00:00:00 is supported",
                    ));
                }
            } else if !upper.starts_with("REM ") && !upper.starts_with("TITLE ") {
                return Err(MediaError::InvalidCue("unsupported directive"));
            }
        }

        if track_count != 1 || index_count != 1 {
            return Err(MediaError::InvalidCue(
                "exactly one TRACK and INDEX are required",
            ));
        }
        Ok(Self {
            track_file: track_file.ok_or(MediaError::InvalidCue("missing FILE entry"))?,
        })
    }

    pub fn track_file(&self) -> &Path {
        &self.track_file
    }
}

pub fn load_disc(path: impl AsRef<Path>) -> Result<DiscImage, MediaError> {
    let path = path.as_ref();
    match extension(path).as_deref() {
        Some("bin") => load_raw(path),
        Some("cue") => load_cue(path),
        Some("zip") => load_zip(path),
        _ => Err(MediaError::UnsupportedExtension),
    }
}

fn load_raw(path: &Path) -> Result<DiscImage, MediaError> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_DISC_SIZE {
        return Err(MediaError::Core(EmulatorError::InvalidDisc(
            "image exceeds the size limit",
        )));
    }
    Ok(DiscImage::from_mode1_2352(fs::read(path)?)?)
}

fn load_cue(path: &Path) -> Result<DiscImage, MediaError> {
    let text = read_limited(fs::File::open(path)?, MAX_CUE_SIZE)?;
    let text =
        std::str::from_utf8(&text).map_err(|_| MediaError::InvalidCue("sheet is not UTF-8"))?;
    let cue = CueSheet::parse(text)?;
    load_raw(
        &path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(cue.track_file()),
    )
}

fn load_zip(path: &Path) -> Result<DiscImage, MediaError> {
    let mut archive = ZipArchive::new(fs::File::open(path)?)?;
    let mut cue_indices = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if extension(Path::new(entry.name())).as_deref() == Some("cue") {
            cue_indices.push(index);
        }
    }
    if cue_indices.len() != 1 {
        return Err(MediaError::InvalidCue(
            "ZIP must contain exactly one CUE sheet",
        ));
    }

    let cue_index = cue_indices[0];
    let (cue_name, cue_bytes) = {
        let entry = archive.by_index(cue_index)?;
        let name = entry
            .enclosed_name()
            .ok_or(MediaError::InvalidCue("unsafe CUE path"))?
            .to_path_buf();
        (name, read_limited(entry, MAX_CUE_SIZE)?)
    };
    let cue_text = std::str::from_utf8(&cue_bytes)
        .map_err(|_| MediaError::InvalidCue("sheet is not UTF-8"))?;
    let cue = CueSheet::parse(cue_text)?;
    let expected = cue_name
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(cue.track_file());
    validate_relative_path(&expected)?;

    let mut track_index = None;
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        if path_eq_ignore_ascii_case(&name, &expected) && track_index.replace(index).is_some() {
            return Err(MediaError::InvalidCue("duplicate track entry"));
        }
    }
    let index = track_index.ok_or(MediaError::InvalidCue("referenced track is missing"))?;
    let entry = archive.by_index(index)?;
    if entry.size() > MAX_DISC_SIZE {
        return Err(MediaError::Core(EmulatorError::InvalidDisc(
            "image exceeds the size limit",
        )));
    }
    let raw = read_limited(entry, MAX_DISC_SIZE)?;
    Ok(DiscImage::from_mode1_2352(raw)?)
}

fn parse_file_line(line: &str) -> Result<(&str, &str), MediaError> {
    let rest = line
        .get(5..)
        .ok_or(MediaError::InvalidCue("malformed FILE entry"))?
        .trim();
    if let Some(quoted) = rest.strip_prefix('"') {
        let end = quoted
            .find('"')
            .ok_or(MediaError::InvalidCue("unterminated FILE name"))?;
        let name = &quoted[..end];
        let kind = quoted[end + 1..].trim();
        if name.is_empty() || kind.is_empty() {
            return Err(MediaError::InvalidCue("malformed FILE entry"));
        }
        Ok((name, kind))
    } else {
        let mut fields = rest.rsplitn(2, char::is_whitespace);
        let kind = fields.next().unwrap_or_default();
        let name = fields.next().unwrap_or_default().trim();
        if name.is_empty() || kind.is_empty() {
            return Err(MediaError::InvalidCue("malformed FILE entry"));
        }
        Ok((name, kind))
    }
}

fn validate_relative_path(path: &Path) -> Result<(), MediaError> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(MediaError::InvalidCue("unsafe track path"));
    }
    Ok(())
}

fn path_eq_ignore_ascii_case(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .replace('\\', "/")
        .eq_ignore_ascii_case(&right.to_string_lossy().replace('\\', "/"))
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
}

fn read_limited(reader: impl Read, maximum: u64) -> Result<Vec<u8>, MediaError> {
    let mut bytes = Vec::new();
    reader.take(maximum + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > maximum {
        return Err(MediaError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "media entry exceeds the size limit",
        )));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_strict_single_track_cue() {
        let cue = CueSheet::parse(
            "FILE \"Game Track.bin\" BINARY\n  TRACK 01 MODE1/2352\n    INDEX 01 00:00:00\n",
        )
        .unwrap();

        assert_eq!(cue.track_file(), Path::new("Game Track.bin"));
    }

    #[test]
    fn rejects_multitrack_and_path_traversal() {
        assert!(CueSheet::parse(
            "FILE \"track.bin\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\nTRACK 02 AUDIO\n"
        )
        .is_err());
        assert!(CueSheet::parse(
            "FILE \"../track.bin\" BINARY\nTRACK 01 MODE1/2352\nINDEX 01 00:00:00\n"
        )
        .is_err());
    }
}
