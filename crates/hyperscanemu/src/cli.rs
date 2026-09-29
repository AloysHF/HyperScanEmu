// CLI: command-line interface for the standalone HyperScan emulator.
//
// Matches sibling desktop emulators (Playdia / Dingoo / SPMP / Native32):
// media path is the first positional argument and there are no subcommands.
// Firmware and diagnostic modes use named flags.

use std::path::PathBuf;

use clap::Parser;

/// Mattel HyperScan emulator
#[derive(Debug, Parser)]
#[command(name = "hyperscanemu", version, about = "Mattel HyperScan emulator")]
pub(crate) struct Cli {
    /// Path to game media (BIN, CUE, or ZIP)
    pub(crate) media: Option<PathBuf>,

    /// Path to the 32 KiB SPG290 internal ROM
    #[arg(short = 'r', long = "internal-rom", value_name = "PATH")]
    pub(crate) internal_rom: Option<PathBuf>,

    /// Path to the 1 MiB HyperScan BIOS
    #[arg(short = 'b', long = "bios", value_name = "PATH")]
    pub(crate) bios: Option<PathBuf>,

    /// Initial integer window scale
    #[arg(short, long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=8))]
    pub(crate) scale: u32,

    /// Open a borderless desktop-sized window
    #[arg(short, long)]
    pub(crate) fullscreen: bool,

    /// Audio volume in percent
    #[arg(short, long, default_value_t = 100, value_parser = clap::value_parser!(u16).range(0..=100))]
    pub(crate) volume: u16,

    /// Run without a window or audio device
    #[arg(long)]
    pub(crate) headless: bool,

    /// Frames to run in headless mode
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..))]
    pub(crate) frames: u64,

    /// Save the final native-resolution frame as PNG and exit
    #[arg(short = 'S', long = "screenshot", value_name = "PATH")]
    pub(crate) screenshot: Option<PathBuf>,

    /// Frames to run before taking a screenshot
    #[arg(long = "screenshot-frames", default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..))]
    pub(crate) screenshot_frames: u64,

    /// Write the final headless frame as a binary PPM
    #[arg(long = "output", value_name = "PATH")]
    pub(crate) output: Option<PathBuf>,

    /// Validate the media package and exit
    #[arg(long)]
    pub(crate) inspect: bool,

    /// Execute a bounded firmware instruction trace (media not required)
    #[arg(long = "trace", value_name = "STEPS", value_parser = clap::value_parser!(u64).range(1..))]
    pub(crate) trace: Option<u64>,
}

/// Windowed frontend options resolved from [`Cli`].
#[derive(Debug)]
pub(crate) struct PlayOptions {
    pub(crate) internal_rom: PathBuf,
    pub(crate) bios: PathBuf,
    pub(crate) media: PathBuf,
    pub(crate) scale: u32,
    pub(crate) fullscreen: bool,
    pub(crate) volume: u16,
    pub(crate) headless: bool,
    pub(crate) frames: u64,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) screenshot_frames: u64,
    pub(crate) output: Option<PathBuf>,
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn parses_media_and_firmware_flags() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "game.zip",
            "--internal-rom",
            "spg290.bin",
            "--bios",
            "hyperscan.bin",
            "--scale",
            "2",
            "--fullscreen",
            "--volume",
            "40",
        ])
        .unwrap();

        assert_eq!(cli.media.as_deref(), Some(Path::new("game.zip")));
        assert_eq!(cli.internal_rom.as_deref(), Some(Path::new("spg290.bin")));
        assert_eq!(cli.bios.as_deref(), Some(Path::new("hyperscan.bin")));
        assert_eq!(cli.scale, 2);
        assert!(cli.fullscreen);
        assert_eq!(cli.volume, 40);
        assert!(!cli.inspect);
        assert!(!cli.headless);
        assert!(cli.trace.is_none());
    }

    #[test]
    fn short_firmware_flags_are_accepted() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "game.cue",
            "-r",
            "spg290.bin",
            "-b",
            "hyperscan.bin",
        ])
        .unwrap();
        assert_eq!(cli.internal_rom.as_deref(), Some(Path::new("spg290.bin")));
        assert_eq!(cli.bios.as_deref(), Some(Path::new("hyperscan.bin")));
    }

    #[test]
    fn inspect_does_not_require_firmware() {
        let cli = Cli::try_parse_from(["hyperscanemu", "media.zip", "--inspect"]).unwrap();
        assert!(cli.inspect);
        assert!(cli.internal_rom.is_none());
        assert!(cli.bios.is_none());
        assert_eq!(cli.media.as_deref(), Some(Path::new("media.zip")));
    }

    #[test]
    fn trace_accepts_step_count_without_media() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "-r",
            "spg290.bin",
            "-b",
            "hyperscan.bin",
            "--trace",
            "200000",
        ])
        .unwrap();
        assert_eq!(cli.trace, Some(200_000));
        assert!(cli.media.is_none());
    }

    #[test]
    fn headless_screenshot_and_output_are_flag_based() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "game.zip",
            "-r",
            "spg290.bin",
            "-b",
            "hyperscan.bin",
            "--headless",
            "--frames",
            "300",
            "--output",
            "frame.ppm",
        ])
        .unwrap();
        assert!(cli.headless);
        assert_eq!(cli.frames, 300);
        assert_eq!(cli.output.as_deref(), Some(Path::new("frame.ppm")));
        assert!(cli.screenshot.is_none());

        let shot = Cli::try_parse_from([
            "hyperscanemu",
            "game.zip",
            "-r",
            "spg290.bin",
            "-b",
            "hyperscan.bin",
            "-S",
            "startup.png",
            "--screenshot-frames",
            "1800",
        ])
        .unwrap();
        assert_eq!(shot.screenshot.as_deref(), Some(Path::new("startup.png")));
        assert_eq!(shot.screenshot_frames, 1800);
    }

    #[test]
    fn rejects_zero_trace_steps_and_zero_scale() {
        assert!(Cli::try_parse_from([
            "hyperscanemu",
            "-r",
            "spg290.bin",
            "-b",
            "hyperscan.bin",
            "--trace",
            "0"
        ])
        .is_err());
        assert!(Cli::try_parse_from([
            "hyperscanemu",
            "game.zip",
            "-r",
            "spg290.bin",
            "-b",
            "hyperscan.bin",
            "--scale",
            "0"
        ])
        .is_err());
    }
}
