use std::ffi::{c_char, c_void, CStr, CString};
use std::path::PathBuf;
use std::ptr;
use std::sync::Mutex;

use hyperscanemu_core::{
    firmware, ControllerButton, ControllerState, Emulator, InputState, BIOS_ROM_SIZE,
    DISPLAY_HEIGHT, DISPLAY_WIDTH, INTERNAL_ROM_SIZE,
};
use hyperscanemu_media::load_disc;

const RETRO_API_VERSION: u32 = 1;
const RETRO_ENVIRONMENT_SET_MESSAGE: u32 = 6;
#[cfg(not(target_os = "emscripten"))]
const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: u32 = 9;
const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: u32 = 10;
#[cfg(not(target_os = "emscripten"))]
const RETRO_ENVIRONMENT_GET_LOG_INTERFACE: u32 = 27;
const RETRO_PIXEL_FORMAT_XRGB8888: u32 = 1;
const RETRO_LOG_INFO: u32 = 1;
const RETRO_LOG_WARN: u32 = 2;
const RETRO_LOG_ERROR: u32 = 3;
/// Save-state envelope: 4-byte magic followed by a little-endian format
/// version. Bump the version whenever the core's state layout changes so stale
/// states are rejected instead of restoring into the wrong fields.
const SAVE_STATE_MAGIC: [u8; 4] = *b"HSEM";
const SAVE_STATE_FORMAT: u32 = 1;
const SAVE_STATE_HEADER_LEN: usize = SAVE_STATE_MAGIC.len() + std::mem::size_of::<u32>();
const RETRO_REGION_NTSC: u32 = 0;
const RETRO_DEVICE_JOYPAD: u32 = 1;
const RETRO_DEVICE_ANALOG: u32 = 5;
const RETRO_DEVICE_INDEX_ANALOG_LEFT: u32 = 0;
const RETRO_DEVICE_ID_ANALOG_X: u32 = 0;
const RETRO_DEVICE_ID_ANALOG_Y: u32 = 1;
const JOYPAD_B: u32 = 0;
const JOYPAD_Y: u32 = 1;
const JOYPAD_SELECT: u32 = 2;
const JOYPAD_START: u32 = 3;
const JOYPAD_UP: u32 = 4;
const JOYPAD_DOWN: u32 = 5;
const JOYPAD_LEFT: u32 = 6;
const JOYPAD_RIGHT: u32 = 7;
const JOYPAD_A: u32 = 8;
const JOYPAD_X: u32 = 9;
const JOYPAD_L: u32 = 10;
const JOYPAD_R: u32 = 11;
const JOYPAD_L2: u32 = 12;
const JOYPAD_R2: u32 = 13;

type EnvironmentCallback = unsafe extern "C" fn(command: u32, data: *mut c_void) -> bool;
type VideoCallback =
    unsafe extern "C" fn(data: *const c_void, width: u32, height: u32, pitch: usize);
type AudioCallback = unsafe extern "C" fn(left: i16, right: i16);
type AudioBatchCallback = unsafe extern "C" fn(data: *const i16, frames: usize) -> usize;
type InputPollCallback = unsafe extern "C" fn();
type InputStateCallback = unsafe extern "C" fn(port: u32, device: u32, index: u32, id: u32) -> i16;
/// Frontend logging callback.
///
/// libretro declares it C-variadic, but over wasm the callee signature carries
/// a hidden varargs pointer, so calling it from Rust traps with "function
/// signature mismatch". Native builds call it with the message as the format
/// string and no arguments; Emscripten builds log to stderr instead.
#[cfg(not(target_os = "emscripten"))]
type LogCallback = unsafe extern "C" fn(level: u32, format: *const c_char);

/// RETRO_ENVIRONMENT_GET_LOG_INTERFACE payload.
#[cfg(not(target_os = "emscripten"))]
#[repr(C)]
pub struct RetroLogCallback {
    log: Option<LogCallback>,
}

/// RETRO_ENVIRONMENT_SET_MESSAGE payload.
#[repr(C)]
pub struct RetroMessage {
    msg: *const c_char,
    /// Duration in frames.
    frames: u32,
}

#[repr(C)]
pub struct RetroSystemInfo {
    library_name: *const c_char,
    library_version: *const c_char,
    valid_extensions: *const c_char,
    need_fullpath: bool,
    block_extract: bool,
}

#[repr(C)]
pub struct RetroGameGeometry {
    base_width: u32,
    base_height: u32,
    max_width: u32,
    max_height: u32,
    aspect_ratio: f32,
}

#[repr(C)]
pub struct RetroSystemTiming {
    fps: f64,
    sample_rate: f64,
}

#[repr(C)]
pub struct RetroSystemAvInfo {
    geometry: RetroGameGeometry,
    timing: RetroSystemTiming,
}

#[repr(C)]
pub struct RetroGameInfo {
    path: *const c_char,
    data: *const c_void,
    size: usize,
    meta: *const c_char,
}

struct RetroState {
    environment: Option<EnvironmentCallback>,
    video: Option<VideoCallback>,
    audio: Option<AudioCallback>,
    audio_batch: Option<AudioBatchCallback>,
    input_poll: Option<InputPollCallback>,
    input_state: Option<InputStateCallback>,
    emulator: Option<Emulator>,
}

impl RetroState {
    const fn new() -> Self {
        Self {
            environment: None,
            video: None,
            audio: None,
            audio_batch: None,
            input_poll: None,
            input_state: None,
            emulator: None,
        }
    }
}

static STATE: Mutex<RetroState> = Mutex::new(RetroState::new());

/// The frontend log callback lives in its own lock so diagnostics can be
/// emitted while `STATE` is held (for example while validating a save state).
#[cfg(not(target_os = "emscripten"))]
static LOG_CALLBACK: Mutex<Option<LogCallback>> = Mutex::new(None);

fn lock_state() -> std::sync::MutexGuard<'static, RetroState> {
    STATE.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[cfg(not(target_os = "emscripten"))]
fn lock_log_callback() -> std::sync::MutexGuard<'static, Option<LogCallback>> {
    LOG_CALLBACK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

/// Write a diagnostic to the frontend's log. Frontends without a log
/// interface simply have nothing to write to.
fn log_message(level: u32, message: &str) {
    // EmulatorJS forwards stderr to the browser console.
    #[cfg(target_os = "emscripten")]
    {
        let _ = level;
        eprintln!("{message}");
    }

    // The message itself is the format string, so percent signs have to be
    // escaped before reaching the frontend's printf.
    #[cfg(not(target_os = "emscripten"))]
    unsafe {
        let Ok(text) = CString::new(message.replace('%', "%%")) else {
            return;
        };
        if let Some(callback) = *lock_log_callback() {
            callback(level, text.as_ptr());
        }
    }
}

fn log_info(message: &str) {
    log_message(RETRO_LOG_INFO, message);
}

fn log_warn(message: &str) {
    log_message(RETRO_LOG_WARN, message);
}

fn log_error(message: &str) {
    log_message(RETRO_LOG_ERROR, message);
}

/// Ask the frontend to show a short on-screen message. The text is mirrored to
/// the log by the callers so headless frontends still see the reason.
fn show_message(message: &str, frames: u32) {
    let Ok(text) = CString::new(message) else {
        return;
    };
    let payload = RetroMessage {
        msg: text.as_ptr(),
        frames,
    };
    let environment = lock_state().environment;
    if let Some(callback) = environment {
        unsafe {
            callback(
                RETRO_ENVIRONMENT_SET_MESSAGE,
                (&payload as *const RetroMessage).cast_mut().cast(),
            );
        }
    }
}

const MISSING_CONTENT_PATH: &str =
    "The frontend passed no content path; this core loads content from a file path only.";

/// Report a failure the user can act on, both on screen and in the log.
fn fail(message: &str) -> bool {
    log_error(message);
    show_message(message, 240);
    false
}

/// Ask the frontend to accept XRGB8888 before any content is read.
fn declare_pixel_format() -> bool {
    let mut format = RETRO_PIXEL_FORMAT_XRGB8888;
    let Some(callback) = lock_state().environment else {
        return false;
    };
    unsafe {
        callback(
            RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
            (&mut format as *mut u32).cast(),
        )
    }
}

/// Run `body`, turning a panic into `fallback`.
///
/// A panic must never unwind across the C ABI: the frontend is a host process
/// (RetroDroid, RetroArch) that would be torn down with us. Swallowing the
/// panic keeps the session recoverable and leaves a log entry behind.
fn guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_owned());
            log_error(&format!(
                "HyperScanEmu libretro entry point panicked: {detail}"
            ));
            fallback
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_api_version() -> u32 {
    RETRO_API_VERSION
}

#[no_mangle]
pub extern "C" fn retro_set_environment(callback: Option<EnvironmentCallback>) {
    guard((), || lock_state().environment = callback);
}

#[no_mangle]
pub extern "C" fn retro_set_video_refresh(callback: Option<VideoCallback>) {
    guard((), || lock_state().video = callback);
}

#[no_mangle]
pub extern "C" fn retro_set_audio_sample(callback: Option<AudioCallback>) {
    guard((), || lock_state().audio = callback);
}

#[no_mangle]
pub extern "C" fn retro_set_audio_sample_batch(callback: Option<AudioBatchCallback>) {
    guard((), || lock_state().audio_batch = callback);
}

#[no_mangle]
pub extern "C" fn retro_set_input_poll(callback: Option<InputPollCallback>) {
    guard((), || lock_state().input_poll = callback);
}

#[no_mangle]
pub extern "C" fn retro_set_input_state(callback: Option<InputStateCallback>) {
    guard((), || lock_state().input_state = callback);
}

#[no_mangle]
pub extern "C" fn retro_init() {
    guard((), || {
        // The log interface is queried natively only; over wasm the callback is
        // unusable (see `LogCallback`) and stderr carries the message instead.
        #[cfg(not(target_os = "emscripten"))]
        {
            let environment = lock_state().environment;
            let Some(callback) = environment else {
                return;
            };

            let mut log = RetroLogCallback { log: None };
            if unsafe {
                callback(
                    RETRO_ENVIRONMENT_GET_LOG_INTERFACE,
                    (&mut log as *mut RetroLogCallback).cast(),
                )
            } {
                *lock_log_callback() = log.log;
            }
        }

        log_info("HyperScanEmu: core initialised (44100 Hz stereo)");
    });
}

#[no_mangle]
pub extern "C" fn retro_deinit() {
    guard((), || lock_state().emulator = None);
}

#[no_mangle]
/// # Safety
/// `info` must be null or point to writable `RetroSystemInfo` storage.
pub unsafe extern "C" fn retro_get_system_info(info: *mut RetroSystemInfo) {
    guard((), || {
        let Some(info) = info.as_mut() else {
            return;
        };
        *info = RetroSystemInfo {
            library_name: c"HyperScanEmu".as_ptr(),
            library_version: concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast(),
            valid_extensions: c"bin|cue|zip".as_ptr(),
            need_fullpath: true,
            block_extract: false,
        };
    });
}

#[no_mangle]
/// # Safety
/// `info` must be null or point to writable `RetroSystemAvInfo` storage.
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut RetroSystemAvInfo) {
    guard((), || {
        let Some(info) = info.as_mut() else {
            return;
        };
        *info = RetroSystemAvInfo {
            geometry: RetroGameGeometry {
                base_width: 320,
                base_height: 240,
                max_width: DISPLAY_WIDTH as u32,
                max_height: DISPLAY_HEIGHT as u32,
                aspect_ratio: 4.0 / 3.0,
            },
            timing: RetroSystemTiming {
                fps: 60.0,
                sample_rate: 44_100.0,
            },
        };
    });
}

#[no_mangle]
pub extern "C" fn retro_set_controller_port_device(_port: u32, _device: u32) {
    guard((), || {});
}

#[no_mangle]
pub extern "C" fn retro_reset() {
    guard((), || {
        if let Some(emulator) = &mut lock_state().emulator {
            emulator.reset();
        }
    });
}

#[no_mangle]
pub extern "C" fn retro_run() {
    guard((), || {
        let mut state = lock_state();
        if let Some(callback) = state.input_poll {
            unsafe { callback() };
        }
        let input = state
            .input_state
            .map(|callback| unsafe { poll_input(callback) })
            .unwrap_or_default();
        let video = state.video;
        let audio = state.audio;
        let audio_batch = state.audio_batch;
        let Some(emulator) = &mut state.emulator else {
            return;
        };
        emulator.set_input(input);
        if emulator.run_frame().is_err() {
            return;
        }

        if let Some(callback) = video {
            let (width, height) = emulator.display_size();
            unsafe {
                callback(
                    emulator.framebuffer().as_ptr().cast(),
                    width as u32,
                    height as u32,
                    width * size_of::<u32>(),
                );
            }
        }
        if let Some(callback) = audio_batch {
            let samples = emulator.audio_samples();
            unsafe {
                callback(samples.as_ptr(), samples.len() / 2);
            }
        } else if let Some(callback) = audio {
            for sample in emulator.audio_samples().as_chunks::<2>().0 {
                unsafe { callback(sample[0], sample[1]) };
            }
        }
    });
}

fn write_save_state_header(buffer: &mut [u8]) {
    buffer[..SAVE_STATE_MAGIC.len()].copy_from_slice(&SAVE_STATE_MAGIC);
    buffer[SAVE_STATE_MAGIC.len()..SAVE_STATE_HEADER_LEN]
        .copy_from_slice(&SAVE_STATE_FORMAT.to_le_bytes());
}

/// Validate the envelope and return the core payload. The caller logs the
/// reason so an incompatible state is rejected with a visible warning.
fn read_save_state_payload(data: &[u8]) -> Result<&[u8], String> {
    if data.len() < SAVE_STATE_HEADER_LEN {
        return Err("the buffer is shorter than the save-state header".to_owned());
    }
    if data[..SAVE_STATE_MAGIC.len()] != SAVE_STATE_MAGIC {
        return Err("the buffer is not a HyperScanEmu save state".to_owned());
    }
    let version = u32::from_le_bytes(
        data[SAVE_STATE_MAGIC.len()..SAVE_STATE_HEADER_LEN]
            .try_into()
            .unwrap(),
    );
    if version != SAVE_STATE_FORMAT {
        return Err(format!(
            "format version {version} is not supported (this core writes version {SAVE_STATE_FORMAT})"
        ));
    }
    Ok(&data[SAVE_STATE_HEADER_LEN..])
}

#[no_mangle]
pub extern "C" fn retro_serialize_size() -> usize {
    guard(0, || {
        lock_state()
            .emulator
            .as_ref()
            .map_or(0, |emulator| SAVE_STATE_HEADER_LEN + emulator.state_size())
    })
}

#[no_mangle]
pub extern "C" fn retro_serialize(data: *mut c_void, size: usize) -> bool {
    guard(false, || {
        if data.is_null() {
            return false;
        }
        let state = lock_state();
        let Some(emulator) = state.emulator.as_ref() else {
            return false;
        };
        let payload = emulator.serialize();
        let total = SAVE_STATE_HEADER_LEN + payload.len();
        if payload.len() != emulator.state_size() || size < total {
            log_error("HyperScanEmu: the frontend supplied too small a save-state buffer");
            return false;
        }
        // SAFETY: the frontend guarantees `size` writable bytes at `data`.
        let buffer = unsafe { std::slice::from_raw_parts_mut(data.cast::<u8>(), total) };
        write_save_state_header(buffer);
        buffer[SAVE_STATE_HEADER_LEN..].copy_from_slice(&payload);
        true
    })
}

#[no_mangle]
pub extern "C" fn retro_unserialize(data: *const c_void, size: usize) -> bool {
    guard(false, || {
        if data.is_null() {
            return false;
        }
        // SAFETY: the frontend guarantees `size` readable bytes at `data`.
        let buffer = unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) };
        let payload = match read_save_state_payload(buffer) {
            Ok(payload) => payload,
            Err(reason) => {
                log_warn(&format!("HyperScanEmu: rejecting save state: {reason}"));
                return false;
            }
        };
        let mut state = lock_state();
        let Some(emulator) = state.emulator.as_mut() else {
            log_warn("HyperScanEmu: rejecting save state: no content is loaded");
            return false;
        };
        match emulator.deserialize(payload) {
            Ok(()) => {
                log_info("HyperScanEmu: save state restored");
                true
            }
            Err(error) => {
                log_warn(&format!("HyperScanEmu: rejecting save state: {error}"));
                false
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn retro_cheat_reset() {
    guard((), || {});
}

#[no_mangle]
pub extern "C" fn retro_cheat_set(_index: u32, _enabled: bool, _code: *const c_char) {
    guard((), || {});
}

#[no_mangle]
/// # Safety
/// `game` and its path must remain valid for the duration of this call.
pub unsafe extern "C" fn retro_load_game(game: *const RetroGameInfo) -> bool {
    guard(false, || unsafe {
        let Some(game) = game.as_ref() else {
            return fail(MISSING_CONTENT_PATH);
        };
        if game.path.is_null() {
            return fail(MISSING_CONTENT_PATH);
        }
        let Ok(path) = CStr::from_ptr(game.path).to_str() else {
            return fail("HyperScan: the content path is not valid UTF-8.");
        };
        let path = PathBuf::from(path);

        // Frames are emitted as XRGB8888; declare it before reading firmware
        // or the disc so a frontend that cannot show it fails fast.
        if !declare_pixel_format() {
            return fail("HyperScan: this frontend does not support XRGB8888 video.");
        }

        let root = match firmware_root() {
            Ok(root) => root,
            Err(error) => return fail(&firmware_location_error(&error)),
        };
        let firmware = match firmware::load(&root) {
            Ok(firmware) => firmware,
            Err(error) => return fail(&firmware_location_error(&error.to_string())),
        };
        let disc = match load_disc(&path) {
            Ok(disc) => disc,
            Err(error) => {
                return fail(&format!(
                    "HyperScan: failed to load this content.\n{error}\n\n\
                     Supported content: a .bin disc image, a single-track .cue sheet or a .zip package."
                ))
            }
        };

        let mut emulator = Emulator::new(firmware);
        emulator.attach_disc(disc);
        lock_state().emulator = Some(emulator);
        log_info(&format!("HyperScanEmu: loaded {}", path.display()));
        true
    })
}

/// Explain where the core looked for its firmware and what it needs.
fn firmware_location_error(detail: &str) -> String {
    let location = match firmware_root() {
        Ok(root) => root.join(firmware::FIRMWARE_DIR).display().to_string(),
        Err(_) => "(frontend did not report a system directory)".to_owned(),
    };
    format!(
        "HyperScan: cannot load firmware.\n{detail}\n\n\
         Copy spg290.bin ({INTERNAL_ROM_SIZE} bytes) and \
         hyperscan.bin ({BIOS_ROM_SIZE} bytes) into:\n{location}"
    )
}

#[no_mangle]
pub extern "C" fn retro_load_game_special(
    _game_type: u32,
    _info: *const RetroGameInfo,
    _num_info: usize,
) -> bool {
    guard(false, || false)
}

#[no_mangle]
pub extern "C" fn retro_unload_game() {
    guard((), || lock_state().emulator = None);
}

#[no_mangle]
pub extern "C" fn retro_get_region() -> u32 {
    guard(RETRO_REGION_NTSC, || RETRO_REGION_NTSC)
}

#[no_mangle]
pub extern "C" fn retro_get_memory_data(_id: u32) -> *mut c_void {
    guard(ptr::null_mut(), ptr::null_mut)
}

#[no_mangle]
pub extern "C" fn retro_get_memory_size(_id: u32) -> usize {
    guard(0, || 0)
}

unsafe fn poll_input(callback: InputStateCallback) -> InputState {
    let mut input = InputState::default();
    for port in 0..2 {
        let mut controller = ControllerState::default();
        for (id, button) in [
            (JOYPAD_X, ControllerButton::Blue),
            (JOYPAD_START, ControllerButton::Start),
            (JOYPAD_SELECT, ControllerButton::Select),
            (JOYPAD_L, ControllerButton::LeftShoulder),
            (JOYPAD_R, ControllerButton::RightShoulder),
            (JOYPAD_L2, ControllerButton::LeftTrigger),
            (JOYPAD_R2, ControllerButton::RightTrigger),
            (JOYPAD_Y, ControllerButton::Yellow),
            (JOYPAD_B, ControllerButton::Red),
            (JOYPAD_A, ControllerButton::Green),
        ] {
            controller.set_button(button, callback(port, RETRO_DEVICE_JOYPAD, 0, id) != 0);
        }
        let analog_x = callback(
            port,
            RETRO_DEVICE_ANALOG,
            RETRO_DEVICE_INDEX_ANALOG_LEFT,
            RETRO_DEVICE_ID_ANALOG_X,
        );
        let analog_y = callback(
            port,
            RETRO_DEVICE_ANALOG,
            RETRO_DEVICE_INDEX_ANALOG_LEFT,
            RETRO_DEVICE_ID_ANALOG_Y,
        );
        let left = callback(port, RETRO_DEVICE_JOYPAD, 0, JOYPAD_LEFT) != 0;
        let right = callback(port, RETRO_DEVICE_JOYPAD, 0, JOYPAD_RIGHT) != 0;
        let up = callback(port, RETRO_DEVICE_JOYPAD, 0, JOYPAD_UP) != 0;
        let down = callback(port, RETRO_DEVICE_JOYPAD, 0, JOYPAD_DOWN) != 0;
        controller.analog_x = if analog_x == 0 {
            dpad_axis(left, right)
        } else {
            analog_to_u8(analog_x)
        };
        controller.analog_y = if analog_y == 0 {
            dpad_axis(up, down)
        } else {
            analog_to_u8(analog_y)
        };
        input.controllers[port as usize] = controller;
    }
    input
}

/// The HyperScan controller reports analog axes, but keyboards and touch
/// controls only produce RetroPad directions. Fold a held direction into the
/// axis when no analog stick is active.
fn dpad_axis(negative: bool, positive: bool) -> u8 {
    match (negative, positive) {
        (true, false) => 0,
        (false, true) => u8::MAX,
        _ => 0x7f,
    }
}

fn analog_to_u8(value: i16) -> u8 {
    ((i32::from(value) + 32_768) * 255 / 65_535) as u8
}

#[cfg(not(target_os = "emscripten"))]
unsafe fn c_path(path: *const c_char) -> Option<PathBuf> {
    if path.is_null() {
        return None;
    }
    Some(PathBuf::from(
        CStr::from_ptr(path).to_string_lossy().as_ref(),
    ))
}

/// Root below which every frontend keeps its firmware.
///
/// EmulatorJS extracts archives into the wasm filesystem root and does not
/// report a RetroArch system directory, so `/` is the root there.
fn firmware_root() -> Result<PathBuf, String> {
    #[cfg(target_os = "emscripten")]
    {
        Ok(PathBuf::from("/"))
    }
    #[cfg(not(target_os = "emscripten"))]
    {
        // SAFETY: the environment callback is only called with the documented
        // GET_SYSTEM_DIRECTORY payload.
        unsafe { system_directory() }
            .ok_or_else(|| "the frontend did not report a system directory".to_owned())
    }
}

#[cfg(not(target_os = "emscripten"))]
unsafe fn system_directory() -> Option<PathBuf> {
    let environment = lock_state().environment?;
    let mut directory: *const c_char = ptr::null();
    if !environment(
        RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY,
        (&mut directory as *mut *const c_char).cast(),
    ) || directory.is_null()
    {
        return None;
    }
    c_path(directory)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperscanemu_core::Firmware;

    #[test]
    fn reports_supported_api_version() {
        assert_eq!(retro_api_version(), RETRO_API_VERSION);
    }

    fn test_emulator() -> Emulator {
        let mut internal = vec![0; INTERNAL_ROM_SIZE];
        let mut bios = vec![0; BIOS_ROM_SIZE];
        internal[INTERNAL_ROM_SIZE - 1] = 1;
        bios[BIOS_ROM_SIZE - 1] = 1;
        Emulator::new(Firmware::from_parts(&internal, &bios).unwrap())
    }

    #[test]
    fn save_state_envelope_rejects_other_versions() {
        let mut buffer = [0_u8; SAVE_STATE_HEADER_LEN + 3];
        buffer[SAVE_STATE_HEADER_LEN..].copy_from_slice(&[1, 2, 3]);
        write_save_state_header(&mut buffer);
        assert_eq!(
            read_save_state_payload(&buffer).unwrap(),
            &[1_u8, 2, 3][..],
            "a freshly written envelope must be accepted"
        );

        buffer[SAVE_STATE_MAGIC.len()] = 0xFF;
        assert!(
            read_save_state_payload(&buffer).is_err(),
            "a state from another format version must be rejected"
        );

        let truncated = [0_u8; SAVE_STATE_HEADER_LEN - 1];
        assert!(
            read_save_state_payload(&truncated).is_err(),
            "a buffer shorter than the envelope must be rejected"
        );
    }

    #[test]
    fn abi_round_trip_rejects_unknown_versions() {
        let mut emulator = test_emulator();
        emulator.run_instructions(4).unwrap();
        lock_state().emulator = Some(emulator);

        let size = retro_serialize_size();
        assert_eq!(size, SAVE_STATE_HEADER_LEN + Emulator::STATE_SIZE);
        let mut buffer = vec![0_u8; size];
        assert!(retro_serialize(buffer.as_mut_ptr().cast(), size));
        assert_eq!(&buffer[..SAVE_STATE_MAGIC.len()], b"HSEM");

        let mut from_the_future = buffer.clone();
        from_the_future[SAVE_STATE_MAGIC.len()] = SAVE_STATE_FORMAT as u8 + 1;
        assert!(!retro_unserialize(
            from_the_future.as_ptr().cast(),
            from_the_future.len()
        ));

        assert!(retro_unserialize(buffer.as_ptr().cast(), buffer.len()));
        lock_state().emulator = None;
    }

    #[test]
    fn panics_are_contained_at_the_c_abi_boundary() {
        let recovered = guard(false, || -> bool { panic!("boom") });

        assert!(
            !recovered,
            "a panicking entry point must return its fallback"
        );
    }

    #[test]
    fn analog_range_maps_to_controller_bytes() {
        assert_eq!(analog_to_u8(i16::MIN), 0);
        assert_eq!(analog_to_u8(0), 127);
        assert_eq!(analog_to_u8(i16::MAX), 255);
    }

    #[test]
    fn dpad_directions_fill_centered_axes() {
        assert_eq!(dpad_axis(true, false), 0);
        assert_eq!(dpad_axis(false, true), u8::MAX);
        assert_eq!(dpad_axis(false, false), 0x7f);
        assert_eq!(dpad_axis(true, true), 0x7f);
    }

    #[test]
    fn system_info_declares_raw_track_loading() {
        let mut info = RetroSystemInfo {
            library_name: ptr::null(),
            library_version: ptr::null(),
            valid_extensions: ptr::null(),
            need_fullpath: false,
            block_extract: true,
        };
        unsafe { retro_get_system_info(&mut info) };

        assert!(info.need_fullpath);
        assert!(!info.block_extract);
        assert_eq!(
            unsafe { CStr::from_ptr(info.valid_extensions) }
                .to_str()
                .unwrap(),
            "bin|cue|zip"
        );
    }
}
