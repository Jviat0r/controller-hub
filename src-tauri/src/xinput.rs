//! Physical Xbox controllers (360, One, Series) come through Windows' own
//! XInput driver. We only need to know which seats they occupy, roughly how
//! their battery is doing, and how to buzz them for "Identify".

use controller_hub_core::{ids::Transport, xmap::XPad};
use windows::Win32::UI::Input::XboxController::*;

/// Read a slot exactly as a game does. This is the ground truth for "what
/// will the game receive", whether the pad in that slot is real or ours.
pub fn read_state(index: usize) -> Option<XPad> {
    let mut state = XINPUT_STATE::default();
    if unsafe { XInputGetState(index as u32, &mut state) } != 0 {
        return None;
    }
    let g = state.Gamepad;
    Some(XPad {
        buttons: g.wButtons.0,
        lt: g.bLeftTrigger,
        rt: g.bRightTrigger,
        lx: g.sThumbLX,
        ly: g.sThumbLY,
        rx: g.sThumbRX,
        ry: g.sThumbRY,
    })
}

pub fn connected() -> [bool; 4] {
    let mut out = [false; 4];
    for (i, slot) in out.iter_mut().enumerate() {
        let mut state = XINPUT_STATE::default();
        *slot = unsafe { XInputGetState(i as u32, &mut state) } == 0;
    }
    out
}

pub struct XboxStatus {
    pub transport: Transport,
    pub battery_label: Option<String>,
}

pub fn status(index: u32) -> XboxStatus {
    let mut info = XINPUT_BATTERY_INFORMATION::default();
    let ok = unsafe { XInputGetBatteryInformation(index, BATTERY_DEVTYPE_GAMEPAD, &mut info) } == 0;
    if !ok {
        return XboxStatus { transport: Transport::Unknown, battery_label: None };
    }
    if info.BatteryType == BATTERY_TYPE_WIRED {
        return XboxStatus { transport: Transport::Usb, battery_label: None };
    }
    if info.BatteryType == BATTERY_TYPE_ALKALINE || info.BatteryType == BATTERY_TYPE_NIMH {
        let label = if info.BatteryLevel == BATTERY_LEVEL_EMPTY {
            "Battery empty"
        } else if info.BatteryLevel == BATTERY_LEVEL_LOW {
            "Battery low"
        } else if info.BatteryLevel == BATTERY_LEVEL_MEDIUM {
            "Battery medium"
        } else if info.BatteryLevel == BATTERY_LEVEL_FULL {
            "Battery full"
        } else {
            "Battery unknown"
        };
        return XboxStatus { transport: Transport::Wireless, battery_label: Some(label.to_string()) };
    }
    XboxStatus { transport: Transport::Unknown, battery_label: None }
}

pub fn rumble(index: u32, left: u16, right: u16) {
    let v = XINPUT_VIBRATION { wLeftMotorSpeed: left, wRightMotorSpeed: right };
    unsafe {
        XInputSetState(index, &v);
    }
}
