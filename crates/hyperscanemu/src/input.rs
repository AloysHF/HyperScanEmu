use hyperscanemu_core::{ControllerButton, ControllerState, InputState};
use minifb::{Key, Window};

const PLAYER_ONE_BUTTONS: [(ControllerButton, Key); 10] = [
    (ControllerButton::Green, Key::Z),
    (ControllerButton::Red, Key::X),
    (ControllerButton::Blue, Key::A),
    (ControllerButton::Yellow, Key::S),
    (ControllerButton::Start, Key::Enter),
    (ControllerButton::Select, Key::Backspace),
    (ControllerButton::LeftShoulder, Key::Q),
    (ControllerButton::RightShoulder, Key::W),
    (ControllerButton::LeftTrigger, Key::E),
    (ControllerButton::RightTrigger, Key::R),
];

const PLAYER_TWO_BUTTONS: [(ControllerButton, Key); 10] = [
    (ControllerButton::Green, Key::F),
    (ControllerButton::Red, Key::G),
    (ControllerButton::Blue, Key::T),
    (ControllerButton::Yellow, Key::Y),
    (ControllerButton::Start, Key::Key1),
    (ControllerButton::Select, Key::Key2),
    (ControllerButton::LeftShoulder, Key::Key3),
    (ControllerButton::RightShoulder, Key::Key4),
    (ControllerButton::LeftTrigger, Key::Key5),
    (ControllerButton::RightTrigger, Key::Key6),
];

pub(crate) fn poll(window: &Window) -> InputState {
    state_from_keys(|key| window.is_key_down(key))
}

fn state_from_keys(mut is_down: impl FnMut(Key) -> bool) -> InputState {
    InputState {
        controllers: [
            controller_from_keys(
                &mut is_down,
                &PLAYER_ONE_BUTTONS,
                [Key::Left, Key::Right, Key::Up, Key::Down],
            ),
            controller_from_keys(
                &mut is_down,
                &PLAYER_TWO_BUTTONS,
                [Key::J, Key::L, Key::I, Key::K],
            ),
        ],
    }
}

fn controller_from_keys(
    is_down: &mut impl FnMut(Key) -> bool,
    buttons: &[(ControllerButton, Key)],
    axes: [Key; 4],
) -> ControllerState {
    let mut controller = ControllerState::default();
    for &(button, key) in buttons {
        controller.set_button(button, is_down(key));
    }
    controller.analog_x = axis_value(is_down(axes[0]), is_down(axes[1]));
    controller.analog_y = axis_value(is_down(axes[2]), is_down(axes[3]));
    controller
}

fn axis_value(negative: bool, positive: bool) -> u8 {
    match (negative, positive) {
        (true, false) => 0,
        (false, true) => u8::MAX,
        _ => 0x7f,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_both_players_and_centers_unpressed_axes() {
        let state = state_from_keys(|key| matches!(key, Key::Left | Key::Z | Key::I | Key::F));

        assert_eq!(state.controllers[0].analog_x, 0);
        assert_eq!(state.controllers[0].analog_y, 0x7f);
        assert!(state.controllers[0].button_pressed(ControllerButton::Green));
        assert_eq!(state.controllers[1].analog_y, 0);
        assert!(state.controllers[1].button_pressed(ControllerButton::Green));
    }

    #[test]
    fn opposing_axis_keys_cancel_to_center() {
        assert_eq!(axis_value(true, true), 0x7f);
    }
}
