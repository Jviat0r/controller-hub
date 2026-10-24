//! Convert a Windows HID symbolic-link path (what hidapi gives us) into the
//! device instance path HidHide expects.
//!
//! `\\?\hid#vid_054c&pid_09cc#7&2b3f1a2&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}`
//! becomes `HID\VID_054C&PID_09CC\7&2B3F1A2&0&0000`.

pub fn instance_path_from_hid_path(p: &str) -> Option<String> {
    let s = p.strip_prefix("\\\\?\\").or_else(|| p.strip_prefix("\\\\.\\")).unwrap_or(p);
    // Drop the trailing interface-class GUID. Bluetooth paths also carry a
    // GUID near the front (the HID service class), which must stay.
    let s = match s.rfind("#{") {
        Some(i) => &s[..i],
        None => s,
    };
    if s.is_empty() {
        return None;
    }
    let out = s.replace('#', "\\").to_ascii_uppercase();
    if out.matches('\\').count() < 2 {
        return None;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_ds4() {
        assert_eq!(
            instance_path_from_hid_path("\\\\?\\hid#vid_054c&pid_09cc#7&2b3f1a2&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}").as_deref(),
            Some("HID\\VID_054C&PID_09CC\\7&2B3F1A2&0&0000")
        );
    }

    #[test]
    fn bluetooth_dualsense() {
        assert_eq!(
            instance_path_from_hid_path("\\\\?\\hid#{00001124-0000-1000-8000-00805f9b34fb}_vid&0002054c_pid&0ce6#9&1e2f3a4&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}").as_deref(),
            Some("HID\\{00001124-0000-1000-8000-00805F9B34FB}_VID&0002054C_PID&0CE6\\9&1E2F3A4&0&0000")
        );
    }

    #[test]
    fn junk_is_rejected() {
        assert_eq!(instance_path_from_hid_path(""), None);
        assert_eq!(instance_path_from_hid_path("\\\\?\\#{guid}"), None);
        assert_eq!(instance_path_from_hid_path("hid#only"), None);
    }
}
