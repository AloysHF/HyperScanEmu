use hyperscanemu_core::{DISPLAY_HEIGHT, DISPLAY_WIDTH};

const RETRO_API_VERSION: u32 = 1;

#[no_mangle]
pub extern "C" fn retro_api_version() -> u32 {
    RETRO_API_VERSION
}
#[no_mangle]
pub extern "C" fn retro_init() {
    let _ = (DISPLAY_WIDTH, DISPLAY_HEIGHT);
}

#[no_mangle]
pub extern "C" fn retro_deinit() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_supported_api_version() {
        assert_eq!(retro_api_version(), RETRO_API_VERSION);
    }
}
