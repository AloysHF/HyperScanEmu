#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputState {
    pub buttons: u64,
    pub pointer_x: i32,
    pub pointer_y: i32,
    pub pointer_pressed: bool,
}
