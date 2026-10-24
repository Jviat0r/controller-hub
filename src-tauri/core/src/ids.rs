use serde::{Deserialize, Serialize};

pub const SONY_VID: u16 = 0x054C;

/// Controller families the hub understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Ds4,
    Dualsense,
    DualsenseEdge,
    Xbox,
}

impl Kind {
    /// Sony pads are identified by USB vendor/product id. Xbox pads never come
    /// through here; Windows exposes them via XInput directly.
    pub fn from_vid_pid(vid: u16, pid: u16) -> Option<Kind> {
        if vid != SONY_VID {
            return None;
        }
        match pid {
            0x05C4 | 0x09CC | 0x0BA0 => Some(Kind::Ds4), // v1, v2, official USB wireless adaptor
            0x0CE6 => Some(Kind::Dualsense),
            0x0DF2 => Some(Kind::DualsenseEdge),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Ds4 => "DualShock 4",
            Kind::Dualsense => "DualSense",
            Kind::DualsenseEdge => "DualSense Edge",
            Kind::Xbox => "Xbox controller",
        }
    }

    pub fn is_sony(self) -> bool {
        !matches!(self, Kind::Xbox)
    }

    pub fn has_light(self) -> bool {
        self.is_sony()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Usb,
    Bluetooth,
    /// Wireless but not Bluetooth (Xbox wireless adaptor, 360 receiver).
    Wireless,
    Unknown,
}

impl Transport {
    pub fn label(self) -> &'static str {
        match self {
            Transport::Usb => "USB",
            Transport::Bluetooth => "Bluetooth",
            Transport::Wireless => "wireless",
            Transport::Unknown => "an unknown link",
        }
    }
}

/// Guess the transport from a Windows HID symbolic-link path when the HID
/// library cannot tell us. Bluetooth HID devices enumerate under BTHENUM/BTHLE.
pub fn transport_from_path(path: &str) -> Transport {
    let p = path.to_ascii_lowercase();
    // 00001124-... is the Bluetooth HID service class; BTHENUM/BTHLE are the
    // Bluetooth enumerators.
    if p.contains("bthenum") || p.contains("bthle") || p.contains("00001124-0000-1000-8000-00805f9b34fb") {
        Transport::Bluetooth
    } else if p.contains("usb#") || p.contains("usb\\") || p.contains("hid#vid_") {
        Transport::Usb
    } else {
        Transport::Unknown
    }
}

/// Normalise a MAC address however it arrives ("1c:66:6d:aa:bb:cc",
/// "1C666DAABBCC", raw bytes) to lowercase hex without separators.
pub fn normalize_mac(s: &str) -> Option<String> {
    let hex: String = s
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    if hex.len() == 12 {
        Some(hex)
    } else {
        None
    }
}

/// Sony reports the MAC little-endian in feature reports; flip it for display.
pub fn mac_from_le_bytes(b: &[u8]) -> Option<String> {
    if b.len() < 6 {
        return None;
    }
    Some(b[..6].iter().rev().map(|x| format!("{x:02x}")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_pids() {
        assert_eq!(Kind::from_vid_pid(SONY_VID, 0x05C4), Some(Kind::Ds4));
        assert_eq!(Kind::from_vid_pid(SONY_VID, 0x09CC), Some(Kind::Ds4));
        assert_eq!(Kind::from_vid_pid(SONY_VID, 0x0CE6), Some(Kind::Dualsense));
        assert_eq!(Kind::from_vid_pid(SONY_VID, 0x0DF2), Some(Kind::DualsenseEdge));
        assert_eq!(Kind::from_vid_pid(SONY_VID, 0x1234), None);
        assert_eq!(Kind::from_vid_pid(0x045E, 0x0CE6), None);
    }

    #[test]
    fn serde_names_match_ui() {
        assert_eq!(serde_json::to_string(&Kind::DualsenseEdge).unwrap(), "\"dualsense_edge\"");
        assert_eq!(serde_json::to_string(&Kind::Ds4).unwrap(), "\"ds4\"");
        assert_eq!(serde_json::to_string(&Transport::Bluetooth).unwrap(), "\"bluetooth\"");
    }

    #[test]
    fn transport_guess() {
        assert_eq!(
            transport_from_path("\\\\?\\hid#{00001124-0000-1000-8000-00805f9b34fb}_vid&0002054c_pid&09cc#8&2ab&0&0000#{4d1e...}"),
            Transport::Bluetooth // the Bluetooth HID service-class GUID form
        );
        assert_eq!(
            transport_from_path("\\\\?\\bthenum#{00001124-...}_vid&0002054c_pid&09cc#8&2ab&0&0000#{4d1e...}"),
            Transport::Bluetooth
        );
        assert_eq!(transport_from_path("\\\\?\\hid#vid_054c&pid_0ce6&mi_03#9&1#{...}"), Transport::Usb);
        assert_eq!(transport_from_path("weird"), Transport::Unknown);
    }

    #[test]
    fn mac_normalisation() {
        assert_eq!(normalize_mac("1C:66:6D:AA:BB:CC").as_deref(), Some("1c666daabbcc"));
        assert_eq!(normalize_mac("1c666daabbcc").as_deref(), Some("1c666daabbcc"));
        assert_eq!(normalize_mac("nope"), None);
        assert_eq!(mac_from_le_bytes(&[0xcc, 0xbb, 0xaa, 0x6d, 0x66, 0x1c, 0x00]).as_deref(), Some("1c666daabbcc"));
        assert_eq!(mac_from_le_bytes(&[1, 2]), None);
    }
}
