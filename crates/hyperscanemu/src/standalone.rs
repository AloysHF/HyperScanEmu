use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use hyperscanemu_core::{Emulator, Firmware, DISPLAY_HEIGHT, DISPLAY_WIDTH};
use hyperscanemu_media::load_disc;
use image::{ImageFormat, RgbImage};
use minifb::{Key, KeyRepeat, ScaleMode, Window, WindowOptions};

use crate::audio_output::AudioOutput;
use crate::cli::PlayOptions;

const FRAMES_PER_SECOND: usize = 60;

pub(crate) fn run(options: PlayOptions) -> Result<()> {
    let internal = fs::read(&options.internal_rom)
        .with_context(|| format!("failed to read {}", options.internal_rom.display()))?;
    let bios = fs::read(&options.bios)
        .with_context(|| format!("failed to read {}", options.bios.display()))?;
    let firmware = Firmware::from_parts(&internal, &bios)?;
    let disc = load_disc(&options.media)?;
    let mut emulator = Emulator::new(firmware);
    emulator.attach_disc(disc);

    if options.headless || options.screenshot.is_some() {
        let frames = options
            .screenshot
            .as_ref()
            .map_or(options.frames, |_| options.screenshot_frames);
        run_frames(&mut emulator, frames)?;
        if let Some(path) = options.screenshot {
            save_png(&path, &emulator)?;
            println!("wrote screenshot to {}", path.display());
        }
        println!(
            "completed {} frame(s): {}x{}, PC {:08x}",
            frames,
            emulator.display_size().0,
            emulator.display_size().1,
            emulator.cpu().pc()
        );
        return Ok(());
    }

    run_window(emulator, &options)
}

fn run_frames(emulator: &mut Emulator, frames: u64) -> Result<()> {
    for _ in 0..frames {
        emulator.run_frame()?;
    }
    Ok(())
}

fn run_window(mut emulator: Emulator, options: &PlayOptions) -> Result<()> {
    let initial_width = DISPLAY_WIDTH * options.scale as usize;
    let initial_height = DISPLAY_HEIGHT * options.scale as usize;
    let (window_width, window_height) = if options.fullscreen {
        screen_size()
    } else {
        (initial_width, initial_height)
    };
    let media_name = options
        .media
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("media");
    let base_title = format!("HyperScanEmu - {media_name}");
    let mut window = Window::new(
        &base_title,
        window_width,
        window_height,
        WindowOptions {
            resize: !options.fullscreen,
            borderless: options.fullscreen,
            scale_mode: ScaleMode::AspectRatioStretch,
            ..WindowOptions::default()
        },
    )
    .context("failed to create emulator window")?;
    if options.fullscreen {
        window.topmost(true);
        window.set_position(0, 0);
    }
    window.set_target_fps(FRAMES_PER_SECOND);

    let audio = if options.volume == 0 {
        None
    } else {
        match AudioOutput::new(options.volume) {
            Ok(output) => Some(output),
            Err(error) => {
                eprintln!("audio output unavailable; continuing silently: {error:#}");
                None
            }
        }
    };
    let mut paused = false;
    let mut title_timer = Instant::now();
    let mut title_frames = 0_u64;

    while window.is_open() && !window.is_key_down(Key::Escape) {
        if window.is_key_pressed(Key::P, KeyRepeat::No) {
            paused = !paused;
            if paused {
                window.set_title(&format!("{base_title} [Paused]"));
            } else {
                window.set_title(&base_title);
                title_frames = 0;
                title_timer = Instant::now();
            }
        }
        if window.is_key_down(Key::LeftCtrl) && window.is_key_pressed(Key::R, KeyRepeat::No) {
            emulator.reset();
            if let Some(audio) = &audio {
                audio.clear();
            }
        }
        if !paused {
            emulator.set_input(crate::input::poll(&window));
            emulator.run_frame()?;
            title_frames += 1;
            if let Some(audio) = &audio {
                audio.submit(emulator.audio_samples());
            }
            let uart = emulator.drain_uart_output();
            if !uart.is_empty() {
                eprint!("{}", String::from_utf8_lossy(&uart));
            }
        }
        let (width, height) = emulator.display_size();
        window
            .update_with_buffer(emulator.framebuffer(), width, height)
            .context("failed to update emulator window")?;

        if window.is_key_pressed(Key::F12, KeyRepeat::No) {
            let path = format!("hyperscanemu-frame-{}.png", emulator.frame_index());
            save_png(Path::new(&path), &emulator)?;
            eprintln!("saved screenshot to {path}");
        }
        if !paused && title_timer.elapsed() >= Duration::from_secs(1) {
            let fps = title_frames as f64 / title_timer.elapsed().as_secs_f64();
            window.set_title(&format!("{base_title} - {fps:.1} FPS"));
            title_frames = 0;
            title_timer = Instant::now();
        }
    }
    Ok(())
}

fn save_png(path: &Path, emulator: &Emulator) -> Result<()> {
    let (width, height) = emulator.display_size();
    let mut rgb = Vec::with_capacity(width * height * 3);
    for pixel in emulator.framebuffer() {
        rgb.extend_from_slice(&[(pixel >> 16) as u8, (pixel >> 8) as u8, *pixel as u8]);
    }
    let image = RgbImage::from_raw(width as u32, height as u32, rgb)
        .context("framebuffer dimensions do not match its pixel data")?;
    image
        .save_with_format(path, ImageFormat::Png)
        .with_context(|| format!("failed to save {}", path.display()))
}

#[cfg(target_os = "windows")]
fn screen_size() -> (usize, usize) {
    unsafe extern "system" {
        fn GetSystemMetrics(index: i32) -> i32;
    }
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    unsafe {
        (
            GetSystemMetrics(SM_CXSCREEN) as usize,
            GetSystemMetrics(SM_CYSCREEN) as usize,
        )
    }
}

#[cfg(target_os = "linux")]
fn screen_size() -> (usize, usize) {
    type Display = *mut core::ffi::c_void;
    #[link(name = "X11")]
    unsafe extern "system" {
        fn XOpenDisplay(display_name: *const u8) -> Display;
        fn XCloseDisplay(display: Display) -> i32;
        fn XDisplayWidth(display: Display, screen_number: i32) -> i32;
        fn XDisplayHeight(display: Display, screen_number: i32) -> i32;
    }
    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return (DISPLAY_WIDTH, DISPLAY_HEIGHT);
        }
        let size = (
            XDisplayWidth(display, 0) as usize,
            XDisplayHeight(display, 0) as usize,
        );
        let _ = XCloseDisplay(display);
        size
    }
}

#[cfg(target_os = "macos")]
fn screen_size() -> (usize, usize) {
    type DirectDisplayId = u32;
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGMainDisplayID() -> DirectDisplayId;
        fn CGDisplayPixelsWide(display: DirectDisplayId) -> usize;
        fn CGDisplayPixelsHigh(display: DirectDisplayId) -> usize;
    }
    unsafe {
        let display = CGMainDisplayID();
        (CGDisplayPixelsWide(display), CGDisplayPixelsHigh(display))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
fn screen_size() -> (usize, usize) {
    (DISPLAY_WIDTH, DISPLAY_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperscanemu_core::{BIOS_ROM_SIZE, INTERNAL_ROM_SIZE};

    #[test]
    fn png_writer_preserves_native_dimensions() {
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[INTERNAL_ROM_SIZE - 1] = 1;
        bios[BIOS_ROM_SIZE - 1] = 1;
        let firmware = Firmware::from_parts(&internal, &bios).unwrap();
        let emulator = Emulator::new(firmware);
        let path = std::env::temp_dir().join(format!(
            "hyperscanemu-standalone-test-{}.png",
            std::process::id()
        ));

        save_png(&path, &emulator).unwrap();
        let image = image::open(&path).unwrap();
        assert_eq!(image.width(), 320);
        assert_eq!(image.height(), 240);
        fs::remove_file(path).unwrap();
    }
}
