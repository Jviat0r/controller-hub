//! Output reports: light bar colour, rumble, flashing and DualSense player
//! LEDs. Layouts follow the Linux hid-sony / hid-playstation drivers.

use crate::crc::append_bt_crc;
use crate::ids::Transport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Output {
    /// Large (left) motor.
    pub rumble_left: u8,
    /// Small (right) motor.
    pub rumble_right: u8,
    pub color: [u8; 3],
    /// Light-bar blink on/off durations in 10 ms units; 0/0 = solid.
    pub flash_on: u8,
    pub flash_off: u8,
    /// Seat index 0..=3 for DualSense player LEDs; None leaves them off.
    pub player: Option<u8>,
    pub mute_led: bool,
}

impl Default for Output {
    fn default() -> Self {
        Output {
            rumble_left: 0,
            rumble_right: 0,
            color: [0, 0, 0],
            flash_on: 0,
            flash_off: 0,
            player: None,
            mute_led: false,
        }
    }
}

/// Default light-bar colours per seat, matching what a PlayStation does.
pub const SEAT_COLORS: [[u8; 3]; 4] = [[0, 0, 255], [255, 0, 0], [0, 255, 0], [255, 0, 255]];

/// DualSense player-LED bit masks per seat (five LEDs, centre-out pattern).
pub const PLAYER_LED_MASKS: [u8; 4] = [0x04, 0x0A, 0x15, 0x1B];

pub fn seat_color(seat: usize) -> [u8; 3] {
    SEAT_COLORS[seat.min(3)]
}

/// DualShock 4 output report.
/// USB: id 0x05, 32 bytes. Bluetooth: id 0x11, 78 bytes with CRC-32 tail.
pub fn ds4_report(o: &Output, transport: Transport) -> Vec<u8> {
    match transport {
        Transport::Bluetooth => {
            let mut b = vec![0u8; 78];
            b[0] = 0x11;
            b[1] = 0xC0 | 0x04; // HID + CRC, 4 ms poll interval
            b[3] = 0x07; // rumble + light bar + blink enable
            b[6] = o.rumble_right;
            b[7] = o.rumble_left;
            b[8..11].copy_from_slice(&o.color);
            b[11] = o.flash_on;
            b[12] = o.flash_off;
            append_bt_crc(&mut b);
            b
        }
        _ => {
            let mut b = vec![0u8; 32];
            b[0] = 0x05;
            b[1] = 0x07;
            b[4] = o.rumble_right;
            b[5] = o.rumble_left;
            b[6..9].copy_from_slice(&o.color);
            b[9] = o.flash_on;
            b[10] = o.flash_off;
            b
        }
    }
}

const DS_VALID0_COMPATIBLE_VIBRATION: u8 = 0x01;
const DS_VALID0_HAPTICS_SELECT: u8 = 0x02;
const DS_VALID1_MIC_MUTE_LED: u8 = 0x01;
const DS_VALID1_LIGHTBAR: u8 = 0x04;
const DS_VALID1_PLAYER_LEDS: u8 = 0x10;
const DS_VALID2_LIGHTBAR_SETUP: u8 = 0x02;
const DS_LIGHTBAR_SETUP_LIGHT_OUT: u8 = 0x02;

fn dualsense_common(o: &Output, first: bool) -> [u8; 47] {
    let mut c = [0u8; 47];
    c[0] = DS_VALID0_COMPATIBLE_VIBRATION | DS_VALID0_HAPTICS_SELECT;
    c[1] = DS_VALID1_LIGHTBAR | DS_VALID1_MIC_MUTE_LED;
    c[2] = o.rumble_right;
    c[3] = o.rumble_left;
    c[8] = o.mute_led as u8;
    if first {
        // Stop the controller's own connect fade so our colour takes over cleanly.
        c[38] = DS_VALID2_LIGHTBAR_SETUP;
        c[41] = DS_LIGHTBAR_SETUP_LIGHT_OUT;
    }
    if let Some(p) = o.player {
        c[1] |= DS_VALID1_PLAYER_LEDS;
        c[43] = PLAYER_LED_MASKS[(p as usize).min(3)];
    }
    c[44..47].copy_from_slice(&o.color);
    c
}

/// DualSense output report. `seq` is a 0..=15 counter the caller increments
/// per Bluetooth report; `first` requests the one-time light-bar setup.
/// USB: id 0x02, 48 bytes. Bluetooth: id 0x31, 78 bytes with CRC-32 tail.
pub fn dualsense_report(o: &Output, transport: Transport, seq: u8, first: bool) -> Vec<u8> {
    let common = dualsense_common(o, first);
    match transport {
        Transport::Bluetooth => {
            let mut b = vec![0u8; 78];
            b[0] = 0x31;
            b[1] = (seq & 0x0F) << 4;
            b[2] = 0x10; // DS_OUTPUT_TAG
            b[3..50].copy_from_slice(&common);
            append_bt_crc(&mut b);
            b
        }
        _ => {
            let mut b = vec![0u8; 48];
            b[0] = 0x02;
            b[1..48].copy_from_slice(&common);
            b
        }
    }
}

/// A short "here I am" pattern: bright white blink plus a rumble pulse.
pub fn identify_pattern(base: &Output) -> Output {
    Output {
        rumble_left: 160,
        rumble_right: 0,
        color: [255, 255, 255],
        flash_on: 10,
        flash_off: 10,
        ..*base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crc::bt_output_crc;

    fn sample() -> Output {
        Output {
            rumble_left: 0x40,
            rumble_right: 0x20,
            color: [1, 2, 3],
            flash_on: 5,
            flash_off: 6,
            player: Some(1),
            mute_led: true,
        }
    }

    #[test]
    fn ds4_usb_layout() {
        let b = ds4_report(&sample(), Transport::Usb);
        assert_eq!(b.len(), 32);
        assert_eq!(b[0], 0x05);
        assert_eq!(b[1], 0x07);
        assert_eq!((b[4], b[5]), (0x20, 0x40));
        assert_eq!(&b[6..9], &[1, 2, 3]);
        assert_eq!((b[9], b[10]), (5, 6));
    }

    #[test]
    fn ds4_bt_layout_and_crc() {
        let b = ds4_report(&sample(), Transport::Bluetooth);
        assert_eq!(b.len(), 78);
        assert_eq!(b[0], 0x11);
        assert_eq!(b[1], 0xC4);
        assert_eq!(b[3], 0x07);
        assert_eq!((b[6], b[7]), (0x20, 0x40));
        assert_eq!(&b[8..11], &[1, 2, 3]);
        assert_eq!((b[11], b[12]), (5, 6));
        let crc = u32::from_le_bytes([b[74], b[75], b[76], b[77]]);
        assert_eq!(crc, bt_output_crc(&b[..74]));
    }

    #[test]
    fn dualsense_usb_layout() {
        let b = dualsense_report(&sample(), Transport::Usb, 0, true);
        assert_eq!(b.len(), 48);
        assert_eq!(b[0], 0x02);
        assert_eq!(b[1], 0x03);
        assert_eq!(b[2], 0x04 | 0x01 | 0x10);
        assert_eq!((b[3], b[4]), (0x20, 0x40));
        assert_eq!(b[9], 1); // mute LED
        assert_eq!(b[39], 0x02); // lightbar setup valid flag (first report only)
        assert_eq!(b[42], 0x02);
        assert_eq!(b[44], 0x0A); // player 2 LEDs
        assert_eq!(&b[45..48], &[1, 2, 3]);
        let later = dualsense_report(&sample(), Transport::Usb, 0, false);
        assert_eq!(later[39], 0);
        assert_eq!(later[42], 0);
    }

    #[test]
    fn dualsense_without_player_leaves_leds_alone() {
        let o = Output { player: None, ..sample() };
        let b = dualsense_report(&o, Transport::Usb, 0, false);
        assert_eq!(b[2] & 0x10, 0);
        assert_eq!(b[44], 0);
    }

    #[test]
    fn dualsense_bt_layout_seq_and_crc() {
        let b = dualsense_report(&sample(), Transport::Bluetooth, 5, false);
        assert_eq!(b.len(), 78);
        assert_eq!(b[0], 0x31);
        assert_eq!(b[1], 0x50);
        assert_eq!(b[2], 0x10);
        assert_eq!(b[3], 0x03);
        assert_eq!(&b[47..50], &[1, 2, 3]);
        let crc = u32::from_le_bytes([b[74], b[75], b[76], b[77]]);
        assert_eq!(crc, bt_output_crc(&b[..74]));
        assert_eq!(dualsense_report(&sample(), Transport::Bluetooth, 16, false)[1], 0x00);
    }

    #[test]
    fn seat_defaults() {
        assert_eq!(seat_color(0), [0, 0, 255]);
        assert_eq!(seat_color(9), SEAT_COLORS[3]);
        assert_eq!(PLAYER_LED_MASKS[0].count_ones(), 1);
        assert_eq!(PLAYER_LED_MASKS[3].count_ones(), 4);
    }

    #[test]
    fn identify_keeps_player_and_mute() {
        let p = identify_pattern(&sample());
        assert_eq!(p.player, Some(1));
        assert!(p.mute_led);
        assert_eq!(p.color, [255, 255, 255]);
        assert!(p.flash_on > 0);
    }
}
