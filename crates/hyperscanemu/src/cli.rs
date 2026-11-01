// CLI: command-line interface for the standalone HyperScan emulator.
//
// Matches sibling desktop emulators (Playdia / Dingoo / SPMP / Native32):
// media path is the first positional argument and there are no subcommands.
// Firmware comes from the fixed system-directory layout; diagnostic modes use
// named flags.

use std::path::PathBuf;

use clap::Parser;

/// Mattel HyperScan emulator
#[derive(Debug, Parser)]
#[command(name = "hyperscanemu", version, about = "Mattel HyperScan emulator")]
pub(crate) struct Cli {
    /// Path to game media (BIN, CUE, or ZIP)
    pub(crate) media: Option<PathBuf>,

    /// Root that contains the hyperscanemu/ firmware folder
    #[arg(long, value_name = "DIR", default_value = "system")]
    pub(crate) system_dir: PathBuf,

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
    pub(crate) system_dir: PathBuf,
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
    use std::path::{Path, PathBuf};

    use super::*;

    #[test]
    fn parses_media_and_display_flags() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "game.zip",
            "--system-dir",
            "tmp/system",
            "--scale",
            "2",
            "--fullscreen",
            "--volume",
            "40",
        ])
        .unwrap();

        assert_eq!(cli.media.as_deref(), Some(Path::new("game.zip")));
        assert_eq!(cli.system_dir, PathBuf::from("tmp/system"));
        assert_eq!(cli.scale, 2);
        assert!(cli.fullscreen);
        assert_eq!(cli.volume, 40);
        assert!(!cli.inspect);
        assert!(!cli.headless);
        assert!(cli.trace.is_none());
    }

    #[test]
    fn system_dir_defaults_to_system() {
        let cli = Cli::try_parse_from(["hyperscanemu", "game.cue"]).unwrap();
        assert_eq!(cli.system_dir, PathBuf::from("system"));
    }

    #[test]
    fn inspect_does_not_require_firmware_or_media_flags() {
        let cli = Cli::try_parse_from(["hyperscanemu", "media.zip", "--inspect"]).unwrap();
        assert!(cli.inspect);
        assert_eq!(cli.media.as_deref(), Some(Path::new("media.zip")));
    }

    #[test]
    fn trace_accepts_step_count_without_media() {
        let cli = Cli::try_parse_from(["hyperscanemu", "--trace", "200000"]).unwrap();
        assert_eq!(cli.trace, Some(200_000));
        assert!(cli.media.is_none());
    }

    #[test]
    fn headless_screenshot_and_output_are_flag_based() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "game.zip",
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
        assert!(Cli::try_parse_from(["hyperscanemu", "--trace", "0"]).is_err());
        assert!(Cli::try_parse_from(["hyperscanemu", "game.zip", "--scale", "0"]).is_err());
    }
}
