//! The JSON the UI renders and the settings file on disk. Serde names here
//! are the contract with `ui/state.js`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{Kind, Transport};

pub const SEAT_COUNT: usize = 4;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct Snapshot {
    pub slots: Vec<SlotView>,
    /// Sony pads we can see but could not seat (no virtual pad driver, or all
    /// four seats are taken).
    pub unseated: Vec<ControllerView>,
    pub drivers: DriverStatus,
    pub bluetooth: BluetoothView,
    pub settings: SettingsView,
    pub log: Vec<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SlotView {
    pub index: usize,
    /// "empty" | "held" | "placeholder" | "controller"
    pub seat: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_for: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held_seconds_left: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub controller: Option<ControllerView>,
}

impl SlotView {
    pub fn empty(index: usize) -> Self {
        SlotView { index, seat: "empty", held_for: None, held_seconds_left: None, controller: None }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ControllerView {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub transport: Transport,
    pub battery: Option<u8>,
    pub charging: Option<bool>,
    /// Coarse text when only a level (low/medium/full) is known, never a
    /// made-up percentage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub battery_label: Option<String>,
    pub color: Option<[u8; 3]>,
    /// Whether this controller has a light we can drive at all, and whether
    /// the user switched it off.
    pub has_light: bool,
    pub light_off: bool,
    /// Seats this controller may be moved to right now (engine-decided).
    pub moves: Vec<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub is_virtual: bool,
    pub hidden: bool,
}

/// `None` means "not checked yet"; the UI shows that as unknown, never as
/// "not installed".
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct DriverStatus {
    pub vigem: Option<bool>,
    pub hidhide: Option<bool>,
    pub hidhide_active: Option<bool>,
    /// The installers shipped inside this install, as verified at run time.
    pub bundled: Vec<BundledInstaller>,
    /// Key of the installer currently running, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installing: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BundledInstaller {
    pub key: String,
    pub name: String,
    pub version: String,
    /// "verified" | "missing" | "corrupt" | "unchecked"
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct BluetoothView {
    pub scanning: bool,
    pub devices: Vec<BtDevice>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BtDevice {
    pub id: String,
    pub name: String,
    pub paired: Option<bool>,
}

/// Persisted settings. Known controllers are keyed by their MAC so the same
/// pad keeps its name and colour whether it arrives over USB or Bluetooth.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    pub hide_from_games: bool,
    pub hold_seat_seconds: u64,
    pub start_minimized: bool,
    pub known: BTreeMap<String, KnownController>,
    /// HidHide instance paths this app hid and has not yet unhidden. Only
    /// non-empty after a crash; the next start unhides them.
    pub hidden_devices: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            hide_from_games: true,
            hold_seat_seconds: 30,
            start_minimized: false,
            known: BTreeMap::new(),
            hidden_devices: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct KnownController {
    pub name: Option<String>,
    pub color: Option<[u8; 3]>,
    /// The user turned this controller's light bar off for good.
    pub light_off: bool,
}

/// The subset of settings the UI edits.
#[derive(Debug, Clone, Serialize, Default, PartialEq)]
pub struct SettingsView {
    pub hide_from_games: bool,
    pub hold_seat_seconds: u64,
    pub start_minimized: bool,
}

impl From<&Settings> for SettingsView {
    fn from(s: &Settings) -> Self {
        SettingsView {
            hide_from_games: s.hide_from_games,
            hold_seat_seconds: s.hold_seat_seconds,
            start_minimized: s.start_minimized,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip_and_defaults() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Settings::default());
        let s: Settings = serde_json::from_str(r#"{"hold_seat_seconds": 5, "known": {"aabbccddeeff": {"color": [1,2,3]}}}"#).unwrap();
        assert_eq!(s.hold_seat_seconds, 5);
        assert!(s.hide_from_games);
        assert_eq!(s.known["aabbccddeeff"].color, Some([1, 2, 3]));
        assert_eq!(s.known["aabbccddeeff"].name, None);
        assert!(!s.known["aabbccddeeff"].light_off, "an old settings file means the light stays on");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap(), s);
    }

    #[test]
    fn slot_view_omits_absent_fields() {
        let json = serde_json::to_string(&SlotView::empty(2)).unwrap();
        assert_eq!(json, r#"{"index":2,"seat":"empty"}"#);
    }

    #[test]
    fn driver_status_unknown_is_null() {
        let json = serde_json::to_string(&DriverStatus::default()).unwrap();
        assert_eq!(json, r#"{"vigem":null,"hidhide":null,"hidhide_active":null,"bundled":[]}"#);
    }
}
