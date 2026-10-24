//! PlayStation controllers over raw HID (USB or Bluetooth) via hidapi.
//! One reader thread per pad owns the HID handle: it parses input reports,
//! pushes state into the pad's virtual Xbox controller, and writes output
//! reports (light bar, rumble, player LEDs) when asked.

use std::{
    ffi::CString,
    sync::{
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use controller_hub_core::{
    ids::{mac_from_le_bytes, normalize_mac, transport_from_path, Kind, Transport},
    input::{parse_ds4, parse_dualsense, Input},
    output::{ds4_report, dualsense_report, identify_pattern, Output},
    xmap::to_xpad,
};
use hidapi::{BusType, HidApi, HidDevice};

use crate::vigem::VirtualPad;

#[derive(Clone, Debug)]
pub struct SonyInfo {
    pub path: String,
    pub kind: Kind,
    pub transport: Transport,
    pub serial: Option<String>,
}

pub fn enumerate(api: &HidApi) -> Vec<SonyInfo> {
    api.device_list()
        .filter_map(|d| {
            let kind = Kind::from_vid_pid(d.vendor_id(), d.product_id())?;
            let gamepad_collection = d.usage_page() == 0x01 && d.usage() == 0x05;
            if !gamepad_collection && d.usage_page() != 0 {
                return None;
            }
            let path = d.path().to_string_lossy().to_string();
            let transport = match d.bus_type() {
                BusType::Usb => Transport::Usb,
                BusType::Bluetooth => Transport::Bluetooth,
                _ => transport_from_path(&path),
            };
            Some(SonyInfo { path, kind, transport, serial: d.serial_number().map(str::to_string) })
        })
        .collect()
}

pub fn open(api: &HidApi, info: &SonyInfo) -> Result<HidDevice, String> {
    let c = CString::new(info.path.clone()).map_err(|e| e.to_string())?;
    api.open_path(&c).map_err(|e| e.to_string())
}

fn feature(dev: &HidDevice, id: u8, len: usize) -> Option<Vec<u8>> {
    // Windows wants a buffer at least as large as the declared report; larger is fine.
    let mut buf = vec![0u8; len.max(64)];
    buf[0] = id;
    let n = dev.get_feature_report(&mut buf).ok()?;
    if n < 7 {
        return None;
    }
    buf.truncate(n);
    Some(buf)
}

/// A stable identity for the physical pad, the same over USB and Bluetooth:
/// its Bluetooth MAC from a feature report, else the serial the HID stack
/// reports (which is the MAC over Bluetooth), else the device path.
pub fn identify(dev: &HidDevice, info: &SonyInfo) -> String {
    let from_feature = match info.kind {
        Kind::Ds4 => feature(dev, 0x81, 7)
            .or_else(|| feature(dev, 0x12, 16))
            .and_then(|b| mac_from_le_bytes(&b[1..])),
        Kind::Dualsense | Kind::DualsenseEdge => feature(dev, 0x09, 20).and_then(|b| mac_from_le_bytes(&b[1..])),
        Kind::Xbox => None,
    };
    if let Some(mac) = from_feature.filter(|m| m != "000000000000" && m != "ffffffffffff") {
        return format!("mac:{mac}");
    }
    if let Some(mac) = info.serial.as_deref().and_then(normalize_mac) {
        return format!("mac:{mac}");
    }
    format!("path:{}", info.path)
}

/// Over Bluetooth both pads start in a stripped-down report mode. Reading the
/// calibration feature report switches them to full reports.
fn enter_full_mode(dev: &HidDevice, kind: Kind) {
    let _ = feature(dev, 0x05, 41);
    if kind == Kind::Ds4 {
        let _ = feature(dev, 0x02, 37);
    }
}

pub enum OutMsg {
    Set(Output),
    Rumble(u8, u8),
    Identify,
    Stop,
}

pub enum PadEvent {
    Battery { id: String, battery: Option<u8>, charging: Option<bool> },
    Gone { id: String },
}

/// The most recent parsed report, for the live test view to read. It is a
/// plain shared slot rather than a channel so a viewer that is not looking
/// costs nothing and can never slow the reader down.
pub type LatestInput = Arc<Mutex<Option<Input>>>;

pub fn spawn_reader(
    dev: HidDevice,
    info: SonyInfo,
    id: String,
    vpad: Arc<Mutex<Option<VirtualPad>>>,
    latest: LatestInput,
    events: Box<dyn Fn(PadEvent) + Send>,
) -> Sender<OutMsg> {
    let (tx, rx) = mpsc::channel();
    let name = format!("hid-{}", id.chars().take(20).collect::<String>());
    thread::Builder::new()
        .name(name)
        .spawn(move || reader_loop(dev, info, id, vpad, latest, events, rx))
        .expect("spawn hid reader");
    tx
}

#[allow(clippy::too_many_arguments)]
fn reader_loop(
    dev: HidDevice,
    info: SonyInfo,
    id: String,
    vpad: Arc<Mutex<Option<VirtualPad>>>,
    latest: LatestInput,
    events: Box<dyn Fn(PadEvent) + Send>,
    rx: Receiver<OutMsg>,
) {
    if info.transport == Transport::Bluetooth {
        enter_full_mode(&dev, info.kind);
    }
    let _ = dev.set_blocking_mode(true);

    let mut buf = [0u8; 128];
    let mut current = Output::default();
    let mut dirty = true;
    let mut first = true;
    let mut seq: u8 = 0;
    let mut identify_until: Option<Instant> = None;
    let mut last_battery = None;
    let mut last_pad = None;
    let mut errors = 0u32;

    loop {
        loop {
            match rx.try_recv() {
                Ok(OutMsg::Set(o)) => {
                    current = o;
                    dirty = true;
                }
                Ok(OutMsg::Rumble(l, r)) => {
                    if current.rumble_left != l || current.rumble_right != r {
                        current.rumble_left = l;
                        current.rumble_right = r;
                        dirty = true;
                    }
                }
                Ok(OutMsg::Identify) => {
                    identify_until = Some(Instant::now() + Duration::from_millis(1500));
                    dirty = true;
                }
                Ok(OutMsg::Stop) | Err(TryRecvError::Disconnected) => return,
                Err(TryRecvError::Empty) => break,
            }
        }

        if let Some(t) = identify_until {
            if Instant::now() >= t {
                identify_until = None;
                dirty = true;
            }
        }

        if dirty {
            let effective = if identify_until.is_some() { identify_pattern(&current) } else { current };
            let report = match info.kind {
                Kind::Ds4 => ds4_report(&effective, info.transport),
                _ => dualsense_report(&effective, info.transport, seq, first),
            };
            seq = (seq + 1) & 0x0F;
            match dev.write(&report) {
                Ok(_) => {
                    dirty = false;
                    first = false;
                }
                Err(_) => errors += 1,
            }
        }

        match dev.read_timeout(&mut buf, 4) {
            Ok(0) => {}
            Ok(n) => {
                errors = 0;
                let parsed = match info.kind {
                    Kind::Ds4 => parse_ds4(&buf[..n], info.transport),
                    _ => parse_dualsense(&buf[..n], info.transport),
                };
                if let Some(input) = parsed {
                    if let Ok(mut slot) = latest.lock() {
                        *slot = Some(input);
                    }
                    let pad = to_xpad(&input);
                    if last_pad != Some(pad) {
                        last_pad = Some(pad);
                        if let Ok(mut guard) = vpad.lock() {
                            if let Some(v) = guard.as_mut() {
                                v.update(&pad);
                            }
                        }
                    }
                    let battery = (input.battery, input.charging);
                    if last_battery != Some(battery) {
                        last_battery = Some(battery);
                        events(PadEvent::Battery { id: id.clone(), battery: battery.0, charging: battery.1 });
                    }
                }
            }
            Err(_) => {
                errors += 1;
                thread::sleep(Duration::from_millis(20));
            }
        }

        if errors > 25 {
            events(PadEvent::Gone { id });
            return;
        }
    }
}
