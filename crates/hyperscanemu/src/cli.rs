use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "hyperscanemu", version, about = "Mattel HyperScan emulator")]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Open an interactive emulator window.
    Play(PlayOptions),
    /// Run a deterministic number of frames without a window.
    Run {
        internal_rom: PathBuf,
        bios: PathBuf,
        media: PathBuf,
        #[arg(value_parser = clap::value_parser!(u64).range(1..))]
        frames: Option<u64>,
        output: Option<PathBuf>,
    },
    /// Execute a bounded firmware instruction trace.
    Trace {
        internal_rom: PathBuf,
        bios: PathBuf,
        #[arg(default_value_t = 1_000, value_parser = clap::value_parser!(u64).range(1..))]
        steps: u64,
    },
    /// Validate a BIN, CUE, or ZIP disc package.
    InspectDisc { media: PathBuf },
}

#[derive(Debug, Args)]
pub(crate) struct PlayOptions {
    /// Path to the 32 KiB SPG290 internal ROM.
    pub(crate) internal_rom: PathBuf,
    /// Path to the 1 MiB HyperScan BIOS.
    pub(crate) bios: PathBuf,
    /// Path to BIN, CUE, or ZIP game media.
    pub(crate) media: PathBuf,
    /// Initial integer window scale.
    #[arg(short, long, default_value_t = 1, value_parser = clap::value_parser!(u32).range(1..=8))]
    pub(crate) scale: u32,
    /// Open a borderless desktop-sized window.
    #[arg(short, long)]
    pub(crate) fullscreen: bool,
    /// Audio volume in percent.
    #[arg(short, long, default_value_t = 100, value_parser = clap::value_parser!(u16).range(0..=100))]
    pub(crate) volume: u16,
    /// Run without a window or audio device.
    #[arg(long)]
    pub(crate) headless: bool,
    /// Frames to run in headless mode.
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..))]
    pub(crate) frames: u64,
    /// Save the final native-resolution frame as PNG and exit.
    #[arg(short = 'S', long)]
    pub(crate) screenshot: Option<PathBuf>,
    /// Frames to run before taking a screenshot.
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(1..))]
    pub(crate) screenshot_frames: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_interactive_options() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "play",
            "internal.bin",
            "bios.bin",
            "game.zip",
            "--scale",
            "2",
            "--fullscreen",
            "--volume",
            "40",
        ])
        .unwrap();

        let Command::Play(options) = cli.command else {
            panic!("expected play command");
        };
        assert_eq!(options.scale, 2);
        assert!(options.fullscreen);
        assert_eq!(options.volume, 40);
    }

    #[test]
    fn preserves_positional_headless_run_arguments() {
        let cli = Cli::try_parse_from([
            "hyperscanemu",
            "run",
            "internal.bin",
            "bios.bin",
            "game.cue",
            "300",
            "frame.ppm",
        ])
        .unwrap();

        let Command::Run { frames, output, .. } = cli.command else {
            panic!("expected run command");
        };
        assert_eq!(frames, Some(300));
        assert_eq!(output, Some(PathBuf::from("frame.ppm")));
    }
}
