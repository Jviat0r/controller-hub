//! Parsing of Sony input reports into one neutral `Input` struct.
//!
//! Layouts follow the Linux hid-sony / hid-playstation drivers, which are the
//! best-documented public references for these controllers.

use crate::ids::Transport;

pub mod btn {
    pub const SQUARE: u32 = 1 << 0;
    pub const CROSS: u32 = 1 << 1;
    pub const CIRCLE: u32 = 1 << 2;
    pub const TRIANGLE: u32 = 1 << 3;
    pub const L1: u32 = 1 << 4;
    pub const R1: u32 = 1 << 5;
    pub const L2: u32 = 1 << 6;
    pub const R2: u32 = 1 << 7;
    pub const SHARE: u32 = 1 << 8; // "Create" on DualSense
    pub const OPTIONS: u32 = 1 << 9;
    pub const L3: u32 = 1 << 10;
    pub const R3: u32 = 1 << 11;
    pub const PS: u32 = 1 << 12;
    pub const TOUCHPAD: u32 = 1 << 13;
    pub const MUTE: u32 = 1 << 14;
}

/// Hat value for "no direction pressed".
pub const DPAD_NONE: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Input {
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
    pub l2: u8,
    pub r2: u8,
    /// 0 = up, 1 = up-right, ... 7 = up-left, 8 = released.
    pub dpad: u8,
    pub buttons: u32,
    /// Percent, when the report carries it.
    pub battery: Option<u8>,
    pub charging: Option<bool>,
}

impl Input {
    pub fn pressed(&self, mask: u32) -> bool {
        self.buttons & mask != 0
    }
}

fn hat(nibble: u8) -> u8 {
    if nibble <= 7 {
        nibble
    } else {
        DPAD_NONE
    }
}

fn sony_face_and_hat(b0: u8) -> (u8, u32) {
    let mut buttons = 0;
    if b0 & 0x10 != 0 {
        buttons |= btn::SQUARE;
    }
    if b0 & 0x20 != 0 {
        buttons |= btn::CROSS;
    }
    if b0 & 0x40 != 0 {
        buttons |= btn::CIRCLE;
    }
    if b0 & 0x80 != 0 {
        buttons |= btn::TRIANGLE;
    }
    (hat(b0 & 0x0F), buttons)
}

fn sony_shoulders(b1: u8) -> u32 {
    let mut buttons = 0;
    for (bit, mask) in [
        (0x01, btn::L1),
        (0x02, btn::R1),
        (0x04, btn::L2),
        (0x08, btn::R2),
        (0x10, btn::SHARE),
        (0x20, btn::OPTIONS),
        (0x40, btn::L3),
        (0x80, btn::R3),
    ] {
        if b1 & bit != 0 {
            buttons |= mask;
        }
    }
    buttons
}

/// DualShock 4.
/// USB: report id 0x01, 64 bytes, data starts at byte 1.
/// Bluetooth full mode: report id 0x11, 78 bytes, data starts at byte 3.
/// Bluetooth basic mode (before calibration is read): id 0x01, 10 bytes, no battery.
pub fn parse_ds4(report: &[u8], transport: Transport) -> Option<Input> {
    let off = match (report.first()?, transport) {
        (0x11, Transport::Bluetooth) => 3,
        (0x01, _) => 1,
        _ => return None,
    };
    let d = report.get(off..)?;
    if d.len() < 9 {
        return None;
    }
    let (dpad, mut buttons) = sony_face_and_hat(d[4]);
    buttons |= sony_shoulders(d[5]);
    if d[6] & 0x01 != 0 {
        buttons |= btn::PS;
    }
    if d[6] & 0x02 != 0 {
        buttons |= btn::TOUCHPAD;
    }
    let (battery, charging) = match d.get(29) {
        Some(&status) => ds4_battery(status),
        None => (None, None),
    };
    Some(Input {
        lx: d[0],
        ly: d[1],
        rx: d[2],
        ry: d[3],
        l2: d[7],
        r2: d[8],
        dpad,
        buttons,
        battery,
        charging,
    })
}

/// DS4 status byte: low nibble is level (0..9 on battery, 0..11 on cable,
/// 11 meaning charge complete), bit 4 is "cable connected".
fn ds4_battery(status: u8) -> (Option<u8>, Option<bool>) {
    let level = status & 0x0F;
    let cable = status & 0x10 != 0;
    if cable {
        if level >= 10 {
            (Some(100), Some(false))
        } else {
            (Some(level * 10 + 5), Some(true))
        }
    } else {
        (Some((level * 10 + 5).min(100)), Some(false))
    }
}

/// DualSense / DualSense Edge.
/// USB: id 0x01, 64 bytes, data at byte 1.
/// Bluetooth full mode: id 0x31, 78 bytes, data at byte 2.
/// Bluetooth basic mode: id 0x01, 10 bytes, no battery.
pub fn parse_dualsense(report: &[u8], transport: Transport) -> Option<Input> {
    let off = match (report.first()?, transport) {
        (0x31, Transport::Bluetooth) => 2,
        (0x01, _) => 1,
        _ => return None,
    };
    let d = report.get(off..)?;
    if d.len() < 10 {
        return None;
    }
    let (dpad, mut buttons) = sony_face_and_hat(d[7]);
    buttons |= sony_shoulders(d[8]);
    if d[9] & 0x01 != 0 {
        buttons |= btn::PS;
    }
    if d[9] & 0x02 != 0 {
        buttons |= btn::TOUCHPAD;
    }
    if d[9] & 0x04 != 0 {
        buttons |= btn::MUTE;
    }
    let (battery, charging) = match d.get(52) {
        Some(&status) => dualsense_battery(status),
        None => (None, None),
    };
    Some(Input {
        lx: d[0],
        ly: d[1],
        rx: d[2],
        ry: d[3],
        l2: d[4],
        r2: d[5],
        dpad,
        buttons,
        battery,
        charging,
    })
}

/// DualSense status byte: low nibble level 0..10, high nibble charging state
/// (0 discharging, 1 charging, 2 full, 0xA/0xB fault, 0xF unknown).
fn dualsense_battery(status: u8) -> (Option<u8>, Option<bool>) {
    let level = status & 0x0F;
    match status >> 4 {
        0x0 => (Some((level * 10 + 5).min(100)), Some(false)),
        0x1 => (Some((level * 10 + 5).min(100)), Some(true)),
        0x2 => (Some(100), Some(false)),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ds4_usb(mutate: impl FnOnce(&mut [u8])) -> Vec<u8> {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[1] = 128;
        r[2] = 128;
        r[3] = 128;
        r[4] = 128;
        r[5] = 0x08; // hat released
        mutate(&mut r);
        r
    }

    #[test]
    fn ds4_usb_neutral() {
        let i = parse_ds4(&ds4_usb(|_| {}), Transport::Usb).unwrap();
        assert_eq!(i.dpad, DPAD_NONE);
        assert_eq!(i.buttons, 0);
        assert_eq!((i.lx, i.ly), (128, 128));
        assert_eq!(i.battery, Some(5)); // status byte 0 => level 0 on battery
    }

    #[test]
    fn ds4_usb_buttons_and_triggers() {
        let r = ds4_usb(|r| {
            r[5] = 0x20 | 0x02; // cross + hat right
            r[6] = 0x01 | 0x20; // L1 + options
            r[7] = 0x01; // PS
            r[8] = 200;
            r[9] = 10;
        });
        let i = parse_ds4(&r, Transport::Usb).unwrap();
        assert!(i.pressed(btn::CROSS));
        assert!(i.pressed(btn::L1));
        assert!(i.pressed(btn::OPTIONS));
        assert!(i.pressed(btn::PS));
        assert!(!i.pressed(btn::SQUARE));
        assert_eq!(i.dpad, 2);
        assert_eq!((i.l2, i.r2), (200, 10));
    }

    #[test]
    fn ds4_battery_decoding() {
        // On battery, level 7
        assert_eq!(ds4_battery(0x07), (Some(75), Some(false)));
        // Cable, level 4 => charging
        assert_eq!(ds4_battery(0x14), (Some(45), Some(true)));
        // Cable, level 11 => full, not charging
        assert_eq!(ds4_battery(0x1B), (Some(100), Some(false)));
        let r = ds4_usb(|r| r[30] = 0x1B);
        assert_eq!(parse_ds4(&r, Transport::Usb).unwrap().battery, Some(100));
    }

    #[test]
    fn ds4_bt_full_mode_is_offset_by_two() {
        let mut r = vec![0u8; 78];
        r[0] = 0x11;
        r[3] = 0; // lx hard left
        r[7] = 0x08 | 0x80; // triangle, hat none
        r[32] = 0x09; // battery 9 on battery
        let i = parse_ds4(&r, Transport::Bluetooth).unwrap();
        assert_eq!(i.lx, 0);
        assert!(i.pressed(btn::TRIANGLE));
        assert_eq!(i.battery, Some(95));
        // The same bytes over USB are not a valid 0x11 report.
        assert!(parse_ds4(&r, Transport::Usb).is_none());
    }

    #[test]
    fn ds4_bt_basic_mode_has_no_battery() {
        let mut r = vec![0u8; 10];
        r[0] = 0x01;
        r[5] = 0x08;
        let i = parse_ds4(&r, Transport::Bluetooth).unwrap();
        assert_eq!(i.battery, None);
        assert_eq!(i.charging, None);
    }

    #[test]
    fn dualsense_usb() {
        let mut r = vec![0u8; 64];
        r[0] = 0x01;
        r[1] = 255; // lx
        r[5] = 99; // l2
        r[8] = 0x08 | 0x10; // hat none + square
        r[9] = 0x10; // create
        r[10] = 0x04; // mute
        r[53] = 0x18; // charging, level 8
        let i = parse_dualsense(&r, Transport::Usb).unwrap();
        assert_eq!(i.lx, 255);
        assert_eq!(i.l2, 99);
        assert!(i.pressed(btn::SQUARE));
        assert!(i.pressed(btn::SHARE));
        assert!(i.pressed(btn::MUTE));
        assert_eq!(i.dpad, DPAD_NONE);
        assert_eq!(i.battery, Some(85));
        assert_eq!(i.charging, Some(true));
    }

    #[test]
    fn dualsense_bt_full_mode_is_offset_by_one() {
        let mut r = vec![0u8; 78];
        r[0] = 0x31;
        r[2] = 7; // lx
        r[9] = 0x08 | 0x20; // cross
        r[54] = 0x2A; // full
        let i = parse_dualsense(&r, Transport::Bluetooth).unwrap();
        assert_eq!(i.lx, 7);
        assert!(i.pressed(btn::CROSS));
        assert_eq!(i.battery, Some(100));
        assert_eq!(i.charging, Some(false));
    }

    #[test]
    fn dualsense_battery_faults_are_unknown() {
        assert_eq!(dualsense_battery(0xA3), (None, None));
        assert_eq!(dualsense_battery(0xF0), (None, None));
        assert_eq!(dualsense_battery(0x0A), (Some(100), Some(false)));
    }

    #[test]
    fn unknown_reports_are_ignored() {
        assert!(parse_ds4(&[0x05, 0, 0], Transport::Usb).is_none());
        assert!(parse_dualsense(&[], Transport::Usb).is_none());
        assert!(parse_dualsense(&[0x01, 1, 2], Transport::Usb).is_none());
    }
}
