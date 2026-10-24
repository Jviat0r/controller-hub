//! Map a Sony `Input` onto an XInput-shaped gamepad. The virtual pad the
//! game sees is an Xbox 360 controller, so this is the whole translation.

use crate::input::{btn, Input};

pub mod xbtn {
    pub const UP: u16 = 0x0001;
    pub const DOWN: u16 = 0x0002;
    pub const LEFT: u16 = 0x0004;
    pub const RIGHT: u16 = 0x0008;
    pub const START: u16 = 0x0010;
    pub const BACK: u16 = 0x0020;
    pub const LTHUMB: u16 = 0x0040;
    pub const RTHUMB: u16 = 0x0080;
    pub const LB: u16 = 0x0100;
    pub const RB: u16 = 0x0200;
    pub const GUIDE: u16 = 0x0400;
    pub const A: u16 = 0x1000;
    pub const B: u16 = 0x2000;
    pub const X: u16 = 0x4000;
    pub const Y: u16 = 0x8000;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct XPad {
    pub buttons: u16,
    pub lt: u8,
    pub rt: u8,
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
}

/// 0..=255 (128 centre) to a full-range signed axis: 0 -> -32768, 255 -> 32767.
pub fn axis(v: u8) -> i16 {
    if v >= 128 {
        ((v as i32 - 128) * 32767 / 127) as i16
    } else {
        ((v as i32 - 128) * 32768 / 128) as i16
    }
}

/// HID sticks report Y positive downwards; XInput reports Y positive upwards.
pub fn axis_inverted(v: u8) -> i16 {
    axis(v).saturating_neg()
}

fn hat_bits(dpad: u8) -> u16 {
    use xbtn::*;
    match dpad {
        0 => UP,
        1 => UP | RIGHT,
        2 => RIGHT,
        3 => DOWN | RIGHT,
        4 => DOWN,
        5 => DOWN | LEFT,
        6 => LEFT,
        7 => UP | LEFT,
        _ => 0,
    }
}

const BUTTON_MAP: [(u32, u16); 11] = [
    (btn::CROSS, xbtn::A),
    (btn::CIRCLE, xbtn::B),
    (btn::SQUARE, xbtn::X),
    (btn::TRIANGLE, xbtn::Y),
    (btn::L1, xbtn::LB),
    (btn::R1, xbtn::RB),
    (btn::L3, xbtn::LTHUMB),
    (btn::R3, xbtn::RTHUMB),
    (btn::SHARE, xbtn::BACK),
    (btn::OPTIONS, xbtn::START),
    (btn::PS, xbtn::GUIDE),
];

/// Touchpad click and mute have no Xbox 360 equivalent and are dropped.
pub fn to_xpad(i: &Input) -> XPad {
    let mut buttons = hat_bits(i.dpad);
    for (from, to) in BUTTON_MAP {
        if i.buttons & from != 0 {
            buttons |= to;
        }
    }
    XPad {
        buttons,
        lt: i.l2,
        rt: i.r2,
        lx: axis(i.lx),
        ly: axis_inverted(i.ly),
        rx: axis(i.rx),
        ry: axis_inverted(i.ry),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::DPAD_NONE;

    #[test]
    fn axis_range() {
        assert_eq!(axis(0), -32768);
        assert_eq!(axis(128), 0);
        assert_eq!(axis(255), 32767);
        assert!(axis(64) < 0 && axis(192) > 0);
        assert_eq!(axis_inverted(0), 32767);
        assert_eq!(axis_inverted(255), -32767);
        assert_eq!(axis_inverted(128), 0);
    }

    #[test]
    fn face_buttons_follow_xbox_positions() {
        let i = Input { buttons: btn::CROSS | btn::TRIANGLE | btn::PS, dpad: DPAD_NONE, ..Default::default() };
        let x = to_xpad(&i);
        assert_eq!(x.buttons, xbtn::A | xbtn::Y | xbtn::GUIDE);
    }

    #[test]
    fn hat_and_triggers() {
        let i = Input { dpad: 5, l2: 30, r2: 255, ..Default::default() };
        let x = to_xpad(&i);
        assert_eq!(x.buttons, xbtn::DOWN | xbtn::LEFT);
        assert_eq!((x.lt, x.rt), (30, 255));
    }

    #[test]
    fn touchpad_and_mute_are_dropped() {
        let i = Input { buttons: btn::TOUCHPAD | btn::MUTE, dpad: DPAD_NONE, ..Default::default() };
        assert_eq!(to_xpad(&i).buttons, 0);
    }

    #[test]
    fn sticks_invert_y_only() {
        let i = Input { lx: 255, ly: 255, rx: 0, ry: 0, dpad: DPAD_NONE, ..Default::default() };
        let x = to_xpad(&i);
        assert_eq!(x.lx, 32767);
        assert_eq!(x.ly, -32767);
        assert_eq!(x.rx, -32768);
        assert_eq!(x.ry, 32767);
    }
}
