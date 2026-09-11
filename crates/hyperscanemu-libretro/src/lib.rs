use std::ffi::{c_char, c_void, CStr};
use std::fs;
use std::path::PathBuf;
use std::ptr;
use std::sync::Mutex;

use hyperscanemu_core::{
    ControllerButton, ControllerState, DiscImage, Emulator, Firmware, InputState, DISPLAY_HEIGHT,
    DISPLAY_WIDTH,
};

const RETRO_API_VERSION: u32 = 1;
const RETRO_ENVIRONMENT_GET_SYSTEM_DIRECTORY: u32 = 9;
const RETRO_ENVIRONMENT_SET_PIXEL_FORMAT: u32 = 10;
const RETRO_PIXEL_FORMAT_XRGB8888: u32 = 1;
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

#[no_mangle]
pub extern "C" fn retro_api_version() -> u32 {
    RETRO_API_VERSION
}

#[no_mangle]
pub extern "C" fn retro_set_environment(callback: Option<EnvironmentCallback>) {
    STATE.lock().unwrap().environment = callback;
}

#[no_mangle]
pub extern "C" fn retro_set_video_refresh(callback: Option<VideoCallback>) {
    STATE.lock().unwrap().video = callback;
}

#[no_mangle]
pub extern "C" fn retro_set_audio_sample(callback: Option<AudioCallback>) {
    STATE.lock().unwrap().audio = callback;
}

#[no_mangle]
pub extern "C" fn retro_set_audio_sample_batch(callback: Option<AudioBatchCallback>) {
    STATE.lock().unwrap().audio_batch = callback;
}

#[no_mangle]
pub extern "C" fn retro_set_input_poll(callback: Option<InputPollCallback>) {
    STATE.lock().unwrap().input_poll = callback;
}

#[no_mangle]
pub extern "C" fn retro_set_input_state(callback: Option<InputStateCallback>) {
    STATE.lock().unwrap().input_state = callback;
}

#[no_mangle]
pub extern "C" fn retro_init() {
    let environment = STATE.lock().unwrap().environment;
    if let Some(callback) = environment {
        let mut format = RETRO_PIXEL_FORMAT_XRGB8888;
        unsafe {
            callback(
                RETRO_ENVIRONMENT_SET_PIXEL_FORMAT,
                (&mut format as *mut u32).cast(),
            );
        }
    }
}

#[no_mangle]
pub extern "C" fn retro_deinit() {
    STATE.lock().unwrap().emulator = None;
}

#[no_mangle]
/// # Safety
/// `info` must be null or point to writable `RetroSystemInfo` storage.
pub unsafe extern "C" fn retro_get_system_info(info: *mut RetroSystemInfo) {
    if let Some(info) = info.as_mut() {
        *info = RetroSystemInfo {
            library_name: c"HyperScanEmu".as_ptr(),
            library_version: c"0.1.0".as_ptr(),
            valid_extensions: c"bin".as_ptr(),
            need_fullpath: true,
            block_extract: false,
        };
    }
}

#[no_mangle]
/// # Safety
/// `info` must be null or point to writable `RetroSystemAvInfo` storage.
pub unsafe extern "C" fn retro_get_system_av_info(info: *mut RetroSystemAvInfo) {
    if let Some(info) = info.as_mut() {
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
    }
}

#[no_mangle]
pub extern "C" fn retro_set_controller_port_device(_port: u32, _device: u32) {}

#[no_mangle]
pub extern "C" fn retro_reset() {
    if let Some(emulator) = &mut STATE.lock().unwrap().emulator {
        emulator.reset();
    }
}

#[no_mangle]
pub extern "C" fn retro_run() {
    let mut state = STATE.lock().unwrap();
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
}

#[no_mangle]
pub extern "C" fn retro_serialize_size() -> usize {
    0
}

#[no_mangle]
pub extern "C" fn retro_serialize(_data: *mut c_void, _size: usize) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn retro_unserialize(_data: *const c_void, _size: usize) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn retro_cheat_reset() {}

#[no_mangle]
pub extern "C" fn retro_cheat_set(_index: u32, _enabled: bool, _code: *const c_char) {}

#[no_mangle]
/// # Safety
/// `game` and its path must remain valid for the duration of this call.
pub unsafe extern "C" fn retro_load_game(game: *const RetroGameInfo) -> bool {
    let Some(game) = game.as_ref() else {
        return false;
    };
    let Some(path) = c_path(game.path) else {
        return false;
    };
    let Some(system_directory) = system_directory() else {
        return false;
    };

    let result = (|| {
        let internal = fs::read(system_directory.join("spg290.bin")).ok()?;
        let bios = fs::read(system_directory.join("hyperscan.bin")).ok()?;
        let firmware = Firmware::from_parts(&internal, &bios).ok()?;
        let disc = DiscImage::from_mode1_2352(fs::read(path).ok()?).ok()?;
        let mut emulator = Emulator::new(firmware);
        emulator.attach_disc(disc);
        Some(emulator)
    })();

    let Some(emulator) = result else {
        return false;
    };
    STATE.lock().unwrap().emulator = Some(emulator);
    true
}

#[no_mangle]
pub extern "C" fn retro_load_game_special(
    _game_type: u32,
    _info: *const RetroGameInfo,
    _num_info: usize,
) -> bool {
    false
}

#[no_mangle]
pub extern "C" fn retro_unload_game() {
    STATE.lock().unwrap().emulator = None;
}

#[no_mangle]
pub extern "C" fn retro_get_region() -> u32 {
    RETRO_REGION_NTSC
}

#[no_mangle]
pub extern "C" fn retro_get_memory_data(_id: u32) -> *mut c_void {
    ptr::null_mut()
}

#[no_mangle]
pub extern "C" fn retro_get_memory_size(_id: u32) -> usize {
    0
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
        controller.analog_x = analog_to_u8(callback(
            port,
            RETRO_DEVICE_ANALOG,
            RETRO_DEVICE_INDEX_ANALOG_LEFT,
            RETRO_DEVICE_ID_ANALOG_X,
        ));
        controller.analog_y = analog_to_u8(callback(
            port,
            RETRO_DEVICE_ANALOG,
            RETRO_DEVICE_INDEX_ANALOG_LEFT,
            RETRO_DEVICE_ID_ANALOG_Y,
        ));
        input.controllers[port as usize] = controller;
    }
    input
}

fn analog_to_u8(value: i16) -> u8 {
    ((i32::from(value) + 32_768) * 255 / 65_535) as u8
}

unsafe fn c_path(path: *const c_char) -> Option<PathBuf> {
    if path.is_null() {
        return None;
    }
    Some(PathBuf::from(
        CStr::from_ptr(path).to_string_lossy().as_ref(),
    ))
}

unsafe fn system_directory() -> Option<PathBuf> {
    let environment = STATE.lock().unwrap().environment?;
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

    #[test]
    fn reports_supported_api_version() {
        assert_eq!(retro_api_version(), RETRO_API_VERSION);
    }

    #[test]
    fn analog_range_maps_to_controller_bytes() {
        assert_eq!(analog_to_u8(i16::MIN), 0);
        assert_eq!(analog_to_u8(0), 127);
        assert_eq!(analog_to_u8(i16::MAX), 255);
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
            "bin"
        );
    }
}
