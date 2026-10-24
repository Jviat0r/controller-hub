//! The engine: one thread owns every piece of mutable state and applies the
//! pure rules from the core crate to what the platform threads report.
//!
//! Threads:
//! - hotplug: enumerates PlayStation HID devices every 500 ms
//! - xinput:  polls which XInput seats are occupied every 500 ms
//! - tick:    one-second heartbeat for seat holds and driver re-checks
//! - one HID reader per PlayStation pad (see `hid.rs`)
//! - short-lived threads for Bluetooth scan/pair
//!
//! Everything funnels into `Event`s handled here; after each one the UI
//! receives a fresh `Snapshot`.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use controller_hub_core::{
    bundle::manifest,
    devpath::instance_path_from_hid_path,
    ids::{Kind, Transport},
    model::{
        BluetoothView, BtDevice, BundledInstaller, ControllerView, DriverStatus, KnownController, Settings,
        SettingsView, SlotView, Snapshot, SEAT_COUNT,
    },
    output::{seat_color, Output},
    seats::{Action, Seat, Seats, Target},
    tester::{Layout, Tester},
};
use hidapi::{HidApi, HidDevice};
use tauri::{AppHandle, Emitter, Manager};

use crate::{
    bluetooth,
    drivers::{self, BundleState},
    hid::{self, LatestInput, OutMsg, PadEvent, SonyInfo},
    hidhide, settings,
    vigem::{Bus, VirtualPad},
    xinput,
};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const LOG_CAP: usize = 40;
const BT_SCAN_SECONDS: u64 = 8;
/// Live test refresh. Fast enough that a button tap is never missed by eye,
/// slow enough to cost nothing.
const PROBE_INTERVAL: Duration = Duration::from_millis(30);

pub enum Cmd {
    Reassign { id: String, slot: usize },
    SetColor { id: String, rgb: [u8; 3] },
    SetLightOff { id: String, off: bool },
    Identify { id: String },
    StartProbe { id: String, reply: Sender<Result<Layout, String>> },
    StopProbe,
    ResetProbe,
    Rename { id: String, name: String },
    ReleaseHold { slot: usize },
    BtScan,
    BtPair { id: String },
    BtUnpair { id: String },
    SetSetting { key: String, value: serde_json::Value },
    RecheckDrivers,
    OpenUrl { key: String },
    InstallDriver { key: String },
    Shutdown(Sender<()>),
}

enum Event {
    Cmd(Cmd),
    Arrived { info: SonyInfo, dev: HidDevice, id: String },
    PathGone(String),
    Pad(PadEvent),
    XInput([bool; SEAT_COUNT]),
    Tick,
    BtScanDone(Result<Vec<BtDevice>, String>),
    BtDone { id: String, result: Result<String, String>, paired: Option<bool> },
    DriverInstallDone { key: String, result: Result<u32, String> },
    HidUnavailable(String),
}

#[derive(Clone)]
pub struct EngineHandle {
    tx: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
}

impl EngineHandle {
    pub fn send(&self, cmd: Cmd) -> Result<(), String> {
        self.tx.send(Event::Cmd(cmd)).map_err(|_| "the engine has stopped".to_string())
    }

    /// Starts the live test and hands back the button names the frames use.
    pub fn start_probe(&self, id: String) -> Result<Layout, String> {
        let (reply, rx) = mpsc::channel();
        self.send(Cmd::StartProbe { id, reply })?;
        rx.recv_timeout(Duration::from_secs(2)).map_err(|_| "the engine did not answer".to_string())?
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().unwrap().clone()
    }

    /// Unplug virtual pads and unhide devices, then return. Safe to call twice.
    pub fn shutdown(&self) {
        let (tx, rx) = mpsc::channel();
        if self.send(Cmd::Shutdown(tx)).is_ok() {
            let _ = rx.recv_timeout(Duration::from_secs(3));
        }
    }
}

struct Pad {
    info: SonyInfo,
    name: String,
    out: Sender<OutMsg>,
    vpad: Arc<Mutex<Option<VirtualPad>>>,
    latest: LatestInput,
    xinput_index: Option<usize>,
    battery: Option<u8>,
    charging: Option<bool>,
    color: [u8; 3],
    custom_color: bool,
    light_off: bool,
    hidden: bool,
    instance: Option<String>,
}

/// What the probe thread needs to know, updated by the engine as seats move.
struct ProbeTarget {
    id: String,
    slot: Option<usize>,
    latest: Option<LatestInput>,
}

struct Probe {
    id: String,
    target: Arc<Mutex<ProbeTarget>>,
    active: Arc<AtomicBool>,
    reset: Arc<AtomicBool>,
}

impl Probe {
    fn stop(&self) {
        self.active.store(false, Ordering::Relaxed);
    }
}

struct XboxSeat {
    transport: Transport,
    battery_label: Option<String>,
}

struct Engine {
    app: AppHandle,
    tx: Sender<Event>,
    snapshot: Arc<Mutex<Snapshot>>,
    started: Instant,
    ticks: u64,
    settings: Settings,
    seats: Seats,
    bus: Option<Bus>,
    pads: HashMap<String, Pad>,
    placeholders: HashMap<usize, VirtualPad>,
    xbox: HashMap<usize, XboxSeat>,
    recently_unplugged: HashMap<usize, Instant>,
    drivers: DriverStatus,
    bt: BluetoothView,
    log: VecDeque<String>,
    hidhide_ready: bool,
    resource_dir: Option<PathBuf>,
    probe: Option<Probe>,
}

pub fn start(app: AppHandle) -> EngineHandle {
    let settings = settings::load();
    let resource_dir = app.path().resource_dir().ok();
    let initial = Snapshot {
        slots: (0..SEAT_COUNT).map(SlotView::empty).collect(),
        settings: SettingsView::from(&settings),
        ..Snapshot::default()
    };
    let snapshot = Arc::new(Mutex::new(initial));
    let (tx, rx) = mpsc::channel::<Event>();
    let handle = EngineHandle { tx: tx.clone(), snapshot: snapshot.clone() };

    {
        let tx = tx.clone();
        thread::Builder::new().name("hotplug".into()).spawn(move || hotplug_loop(tx)).expect("spawn hotplug");
    }
    {
        let tx = tx.clone();
        thread::Builder::new()
            .name("xinput".into())
            .spawn(move || loop {
                if tx.send(Event::XInput(xinput::connected())).is_err() {
                    return;
                }
                thread::sleep(Duration::from_millis(500));
            })
            .expect("spawn xinput");
    }
    {
        let tx = tx.clone();
        thread::Builder::new()
            .name("tick".into())
            .spawn(move || loop {
                thread::sleep(Duration::from_secs(1));
                if tx.send(Event::Tick).is_err() {
                    return;
                }
            })
            .expect("spawn tick");
    }

    let hold_ms = settings.hold_seat_seconds * 1000;
    thread::Builder::new()
        .name("engine".into())
        .spawn(move || {
            let mut engine = Engine {
                app,
                tx,
                snapshot,
                started: Instant::now(),
                ticks: 0,
                settings,
                seats: Seats::new(hold_ms),
                bus: None,
                pads: HashMap::new(),
                placeholders: HashMap::new(),
                xbox: HashMap::new(),
                recently_unplugged: HashMap::new(),
                drivers: DriverStatus::default(),
                bt: BluetoothView::default(),
                log: VecDeque::new(),
                hidhide_ready: false,
                resource_dir,
                probe: None,
            };
            engine.check_drivers();
            engine.recover_hidden_devices();
            engine.publish();
            for event in rx {
                let stop = engine.handle(event);
                engine.publish();
                if stop {
                    break;
                }
            }
        })
        .expect("spawn engine");
    handle
}

fn hotplug_loop(tx: Sender<Event>) {
    let mut api = match HidApi::new() {
        Ok(a) => a,
        Err(e) => {
            let _ = tx.send(Event::HidUnavailable(e.to_string()));
            return;
        }
    };
    let mut handed: HashSet<String> = HashSet::new();
    loop {
        if api.refresh_devices().is_ok() {
            let list = hid::enumerate(&api);
            let present: HashSet<String> = list.iter().map(|i| i.path.clone()).collect();
            let gone: Vec<String> = handed.iter().filter(|p| !present.contains(*p)).cloned().collect();
            for path in gone {
                handed.remove(&path);
                if tx.send(Event::PathGone(path)).is_err() {
                    return;
                }
            }
            for info in list {
                if handed.contains(&info.path) {
                    continue;
                }
                // A failed open (device still initialising, or in use) is retried next round.
                if let Ok(dev) = hid::open(&api, &info) {
                    let id = hid::identify(&dev, &info);
                    handed.insert(info.path.clone());
                    if tx.send(Event::Arrived { info, dev, id }).is_err() {
                        return;
                    }
                }
            }
        }
        thread::sleep(Duration::from_millis(500));
    }
}

fn xinput_id(id: &str) -> Option<u32> {
    id.strip_prefix("xinput:")?.parse().ok()
}

fn mac_of(id: &str) -> Option<&str> {
    id.strip_prefix("mac:")
}

fn default_name(info: &SonyInfo, id: &str) -> String {
    match mac_of(id) {
        Some(mac) if mac.len() >= 4 => format!("{} {}", info.kind.label(), &mac[mac.len() - 4..]),
        _ => info.kind.label().to_string(),
    }
}

fn open_url(key: &str) {
    let url = match key {
        "vigem" => "https://github.com/nefarius/ViGEmBus/releases/latest",
        "hidhide" => "https://github.com/nefarius/HidHide/releases/latest",
        _ => return,
    };
    let _ = Command::new("cmd").args(["/C", "start", "", url]).creation_flags(CREATE_NO_WINDOW).spawn();
}

impl Engine {
    fn now_ms(&self) -> u64 {
        self.started.elapsed().as_millis() as u64
    }

    fn log(&mut self, msg: impl Into<String>) {
        let msg = msg.into();
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(msg);
    }

    fn name_of(&self, id: &str) -> String {
        if let Some(p) = self.pads.get(id) {
            return p.name.clone();
        }
        if xinput_id(id).is_some() {
            return "Xbox controller".into();
        }
        id.to_string()
    }

    fn known(&self, id: &str) -> KnownController {
        mac_of(id).and_then(|m| self.settings.known.get(m).cloned()).unwrap_or_default()
    }

    fn remember(&mut self, id: &str, edit: impl FnOnce(&mut KnownController)) {
        let Some(mac) = mac_of(id).map(str::to_string) else { return };
        edit(self.settings.known.entry(mac).or_default());
        if let Err(e) = settings::save(&self.settings) {
            self.log(format!("Could not save settings: {e}"));
        }
    }

    // ---------- events ----------

    fn handle(&mut self, event: Event) -> bool {
        match event {
            Event::Cmd(cmd) => return self.command(cmd),
            Event::Arrived { info, dev, id } => self.arrived(info, dev, id),
            Event::PathGone(path) => {
                let id = self.pads.iter().find(|(_, p)| p.info.path == path).map(|(id, _)| id.clone());
                if let Some(id) = id {
                    self.remove_pad(&id, "disconnected");
                }
            }
            Event::Pad(PadEvent::Gone { id }) => {
                if self.pads.contains_key(&id) {
                    self.remove_pad(&id, "stopped responding");
                }
            }
            Event::Pad(PadEvent::Battery { id, battery, charging }) => {
                if let Some(p) = self.pads.get_mut(&id) {
                    p.battery = battery;
                    p.charging = charging;
                }
            }
            Event::XInput(connected) => self.xinput_scan(connected),
            Event::Tick => self.tick(),
            Event::BtScanDone(result) => {
                self.bt.scanning = false;
                match result {
                    Ok(list) => {
                        self.bt.message = if list.is_empty() {
                            "No Bluetooth devices found. Is the controller in pairing mode and the PC's Bluetooth on?".into()
                        } else {
                            format!("Found {} device(s).", list.len())
                        };
                        self.bt.devices = list;
                    }
                    Err(e) => self.bt.message = format!("Scan failed: {e}"),
                }
            }
            Event::BtDone { id, result, paired } => match result {
                Ok(msg) => {
                    self.bt.message = msg;
                    if let Some(d) = self.bt.devices.iter_mut().find(|d| d.id == id) {
                        d.paired = paired;
                    }
                }
                Err(e) => self.bt.message = e,
            },
            Event::DriverInstallDone { key, result } => {
                self.drivers.installing = None;
                let name = manifest()
                    .installers
                    .iter()
                    .find(|s| s.key == key)
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| key.clone());
                match result {
                    Ok(code) => self.log(format!("{name}: {}", drivers::describe_exit(code))),
                    Err(e) => self.log(format!("{name} installer: {e}")),
                }
                self.check_drivers();
            }
            Event::HidUnavailable(e) => self.log(format!("Cannot access HID devices: {e}")),
        }
        false
    }

    fn command(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Reassign { id, slot } => match self.seats.reassign(&id, slot) {
                Ok(actions) => {
                    self.execute(actions);
                    let name = self.name_of(&id);
                    self.log(format!("Moved {name} to Player {}", slot + 1));
                }
                Err(e) => self.log(e),
            },
            Cmd::SetColor { id, rgb } => {
                if let Some(p) = self.pads.get_mut(&id) {
                    p.color = rgb;
                    p.custom_color = true;
                    // Picking a colour is a clear request to see the light.
                    p.light_off = false;
                }
                self.remember(&id, |k| {
                    k.color = Some(rgb);
                    k.light_off = false;
                });
                self.apply_output(&id);
            }
            Cmd::SetLightOff { id, off } => {
                if let Some(p) = self.pads.get_mut(&id) {
                    p.light_off = off;
                }
                self.remember(&id, |k| k.light_off = off);
                self.apply_output(&id);
            }
            Cmd::StartProbe { id, reply } => {
                let result = self.start_probe(&id);
                let _ = reply.send(result);
            }
            Cmd::StopProbe => self.stop_probe(),
            Cmd::ResetProbe => {
                if let Some(p) = &self.probe {
                    p.reset.store(true, Ordering::Relaxed);
                }
            }
            Cmd::Identify { id } => {
                if let Some(p) = self.pads.get(&id) {
                    let _ = p.out.send(OutMsg::Identify);
                } else if let Some(i) = xinput_id(&id) {
                    thread::spawn(move || {
                        xinput::rumble(i, 45_000, 45_000);
                        thread::sleep(Duration::from_millis(450));
                        xinput::rumble(i, 0, 0);
                    });
                }
            }
            Cmd::Rename { id, name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    return false;
                }
                if let Some(p) = self.pads.get_mut(&id) {
                    p.name = name.clone();
                }
                self.remember(&id, |k| k.name = Some(name));
            }
            Cmd::ReleaseHold { slot } => {
                let actions = self.seats.release(slot);
                self.execute(actions);
            }
            Cmd::BtScan => {
                if !self.bt.scanning {
                    self.bt.scanning = true;
                    self.bt.message = format!("Scanning for about {BT_SCAN_SECONDS} seconds…");
                    let tx = self.tx.clone();
                    thread::spawn(move || {
                        let result = bluetooth::scan(Duration::from_secs(BT_SCAN_SECONDS));
                        let _ = tx.send(Event::BtScanDone(result));
                    });
                }
            }
            Cmd::BtPair { id } => {
                self.bt.message = "Pairing…".into();
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let result = bluetooth::pair(&id);
                    let paired = if result.is_ok() { Some(true) } else { None };
                    let _ = tx.send(Event::BtDone { id, result, paired });
                });
            }
            Cmd::BtUnpair { id } => {
                self.bt.message = "Unpairing…".into();
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let result = bluetooth::unpair(&id);
                    let paired = if result.is_ok() { Some(false) } else { None };
                    let _ = tx.send(Event::BtDone { id, result, paired });
                });
            }
            Cmd::SetSetting { key, value } => self.set_setting(&key, value),
            Cmd::RecheckDrivers => self.check_drivers(),
            Cmd::OpenUrl { key } => open_url(&key),
            Cmd::InstallDriver { key } => self.install_driver(&key),
            Cmd::Shutdown(reply) => {
                self.shutdown();
                let _ = reply.send(());
                return true;
            }
        }
        false
    }

    // ---------- PlayStation pads ----------

    fn arrived(&mut self, info: SonyInfo, dev: HidDevice, id: String) {
        if let Some(existing) = self.pads.get(&id) {
            // The same physical pad on a second link (USB cable plugged in
            // while on Bluetooth, or the reverse). USB wins.
            let keep_existing = existing.info.transport == Transport::Usb || info.transport != Transport::Usb;
            if keep_existing {
                let msg = format!(
                    "{} is already connected over {}; ignoring its {} link",
                    existing.name,
                    existing.info.transport.label(),
                    info.transport.label()
                );
                self.log(msg);
                return;
            }
            self.remove_pad(&id, "is switching to USB");
        }

        let known = self.known(&id);
        let name = known.name.clone().unwrap_or_else(|| default_name(&info, &id));
        let vpad = Arc::new(Mutex::new(None));
        let latest: LatestInput = Arc::new(Mutex::new(None));
        let tx = self.tx.clone();
        let out = hid::spawn_reader(
            dev,
            info.clone(),
            id.clone(),
            vpad.clone(),
            latest.clone(),
            Box::new(move |e| {
                let _ = tx.send(Event::Pad(e));
            }),
        );
        let transport = info.transport;
        let instance = instance_path_from_hid_path(&info.path);
        self.pads.insert(
            id.clone(),
            Pad {
                info,
                name: name.clone(),
                out,
                vpad,
                latest,
                xinput_index: None,
                battery: None,
                charging: None,
                color: known.color.unwrap_or([0, 0, 0]),
                custom_color: known.color.is_some(),
                light_off: known.light_off,
                hidden: false,
                instance,
            },
        );
        self.log(format!("{name} connected over {}", transport.label()));
        self.maybe_hide(&id);
        if self.bus.is_some() {
            let actions = self.seats.sony_arrived(&id, &name);
            self.execute(actions);
        }
        self.apply_output(&id);
    }

    fn remove_pad(&mut self, id: &str, why: &str) {
        let name = self.name_of(id);
        let actions = self.seats.sony_left(id, &name, self.now_ms());
        self.execute(actions);
        if let Some(p) = self.pads.remove(id) {
            let _ = p.out.send(OutMsg::Stop);
            let taken = p.vpad.lock().map(|mut g| g.take()).unwrap_or(None);
            if let Some(v) = taken {
                if let Some(i) = p.xinput_index {
                    self.recently_unplugged.insert(i, Instant::now());
                }
                v.unplug();
            }
            if p.hidden {
                if let Some(inst) = p.instance.clone() {
                    self.unhide(&inst);
                }
            }
        }
        self.log(format!("{name} {why}"));
    }

    /// Push colour and player LEDs for the pad's current seat. Turning the
    /// light off only darkens the bar; the DualSense player LEDs stay, since
    /// they are how you tell which player a pad is.
    fn apply_output(&mut self, id: &str) {
        let seat = self.seats.seat_of(id);
        if let Some(p) = self.pads.get_mut(id) {
            if !p.custom_color {
                if let Some(s) = seat {
                    p.color = seat_color(s);
                }
            }
            let color = if p.light_off { [0, 0, 0] } else { p.color };
            let out = Output { color, player: seat.map(|s| s as u8), ..Output::default() };
            let _ = p.out.send(OutMsg::Set(out));
        }
    }

    fn maybe_hide(&mut self, id: &str) {
        if !self.settings.hide_from_games || self.drivers.hidhide != Some(true) {
            return;
        }
        if !self.hidhide_ready {
            let setup = std::env::current_exe()
                .map_err(|e| e.to_string())
                .and_then(|exe| hidhide::register_app(&exe))
                .and_then(|_| hidhide::cloak_on());
            match setup {
                Ok(()) => self.hidhide_ready = true,
                Err(e) => {
                    self.log(format!("HidHide setup failed: {e}"));
                    return;
                }
            }
        }
        let Some(p) = self.pads.get_mut(id) else { return };
        let Some(inst) = p.instance.clone() else { return };
        let result = hidhide::hide(&inst);
        let name = p.name.clone();
        match result {
            Ok(()) => {
                p.hidden = true;
                self.mark_hidden(&inst, true);
            }
            Err(e) => self.log(format!("Could not hide {name} from games: {e}")),
        }
    }

    fn unhide(&mut self, inst: &str) {
        let _ = hidhide::unhide(inst);
        self.mark_hidden(inst, false);
    }

    /// Remember which devices we hid so a crash never leaves a pad invisible.
    fn mark_hidden(&mut self, inst: &str, hidden: bool) {
        let list = &mut self.settings.hidden_devices;
        let has = list.iter().any(|x| x == inst);
        if hidden && !has {
            list.push(inst.to_string());
        } else if !hidden && has {
            list.retain(|x| x != inst);
        } else {
            return;
        }
        if let Err(e) = settings::save(&self.settings) {
            self.log(format!("Could not save settings: {e}"));
        }
    }

    /// Runs once at start: undo any hides left over from a previous session
    /// that did not exit cleanly.
    fn recover_hidden_devices(&mut self) {
        if self.settings.hidden_devices.is_empty() {
            return;
        }
        let stale = std::mem::take(&mut self.settings.hidden_devices);
        let mut restored = 0;
        if hidhide::installed() {
            for inst in &stale {
                if hidhide::unhide(inst).is_ok() {
                    restored += 1;
                }
            }
        }
        if let Err(e) = settings::save(&self.settings) {
            self.log(format!("Could not save settings: {e}"));
        }
        self.log(format!("Unhid {restored} controller(s) left hidden by a previous session"));
    }

    // ---------- seats ----------

    fn execute(&mut self, actions: Vec<Action>) {
        for action in actions {
            match action {
                Action::Unplug(Target::Pad(id)) => {
                    if let Some(p) = self.pads.get_mut(&id) {
                        let taken = p.vpad.lock().map(|mut g| g.take()).unwrap_or(None);
                        if let Some(v) = taken {
                            if let Some(i) = p.xinput_index {
                                self.recently_unplugged.insert(i, Instant::now());
                            }
                            v.unplug();
                        }
                        p.xinput_index = None;
                    }
                }
                Action::Unplug(Target::Placeholder(seat)) => {
                    if let Some(v) = self.placeholders.remove(&seat) {
                        self.recently_unplugged.insert(seat, Instant::now());
                        v.unplug();
                    }
                }
                Action::Plug { target, .. } => {
                    let Some(bus) = self.bus.as_ref() else {
                        self.seats.plug_failed(&target);
                        continue;
                    };
                    match VirtualPad::plug(bus) {
                        Ok((mut v, index)) => {
                            match &target {
                                Target::Pad(id) => {
                                    let Some(p) = self.pads.get_mut(id) else {
                                        v.unplug();
                                        self.seats.plug_failed(&target);
                                        continue;
                                    };
                                    let out = p.out.clone();
                                    let _ = v.on_rumble(move |l, r| {
                                        let _ = out.send(OutMsg::Rumble(l, r));
                                    });
                                    if let Ok(mut g) = p.vpad.lock() {
                                        *g = Some(v);
                                    }
                                    p.xinput_index = Some(index);
                                }
                                Target::Placeholder(_) => {
                                    self.placeholders.insert(index, v);
                                }
                            }
                            if let Some(note) = self.seats.landed(&target, index) {
                                self.log(note);
                            }
                            if let Target::Pad(id) = &target {
                                let id = id.clone();
                                self.apply_output(&id);
                            }
                        }
                        Err(e) => {
                            self.seats.plug_failed(&target);
                            self.log(format!("Could not create a virtual pad: {e}"));
                        }
                    }
                }
            }
        }
    }

    fn xinput_scan(&mut self, connected: [bool; SEAT_COUNT]) {
        self.recently_unplugged.retain(|_, t| t.elapsed() < Duration::from_secs(2));
        let ours: HashSet<usize> = self
            .pads
            .values()
            .filter_map(|p| p.xinput_index)
            .chain(self.placeholders.keys().copied())
            .collect();
        let mut physical = [false; SEAT_COUNT];
        for (i, slot) in physical.iter_mut().enumerate() {
            *slot = connected[i] && !ours.contains(&i) && !self.recently_unplugged.contains_key(&i);
        }
        for (i, present) in physical.iter().enumerate() {
            if *present {
                if !self.xbox.contains_key(&i) {
                    self.log(format!("Xbox controller connected as Player {}", i + 1));
                }
                let s = xinput::status(i as u32);
                self.xbox.insert(i, XboxSeat { transport: s.transport, battery_label: s.battery_label });
            } else if self.xbox.remove(&i).is_some() {
                self.log(format!("Xbox controller at Player {} disconnected", i + 1));
            }
        }
        let actions = self.seats.physical_scan(physical);
        self.execute(actions);
    }

    fn tick(&mut self) {
        self.ticks += 1;
        let actions = self.seats.tick(self.now_ms());
        self.execute(actions);
        if self.bus.is_none() && self.ticks % 10 == 0 {
            self.check_drivers();
        }
    }

    // ---------- drivers & settings ----------

    fn check_drivers(&mut self) {
        if self.bus.is_none() {
            match Bus::connect() {
                Ok(bus) => {
                    self.bus = Some(bus);
                    self.log("ViGEmBus connected");
                    let waiting: Vec<(String, String)> = self
                        .pads
                        .iter()
                        .filter(|(id, _)| self.seats.seat_of(id).is_none())
                        .map(|(id, p)| (id.clone(), p.name.clone()))
                        .collect();
                    for (id, name) in waiting {
                        let actions = self.seats.sony_arrived(&id, &name);
                        self.execute(actions);
                        self.apply_output(&id);
                    }
                }
                Err(e) => {
                    if self.drivers.vigem != Some(false) {
                        self.log(format!("ViGEmBus is not available: {e}"));
                    }
                }
            }
        }
        self.drivers.vigem = Some(self.bus.is_some());
        let installed = hidhide::installed();
        self.drivers.hidhide = Some(installed);
        self.drivers.hidhide_active = if installed { hidhide::is_active() } else { None };
        self.check_bundle();
    }

    fn set_setting(&mut self, key: &str, value: serde_json::Value) {
        match key {
            "hide_from_games" => {
                let on = value.as_bool().unwrap_or(false);
                self.settings.hide_from_games = on;
                if on {
                    let ids: Vec<String> = self.pads.keys().cloned().collect();
                    for id in ids {
                        self.maybe_hide(&id);
                    }
                } else {
                    let to_unhide: Vec<String> = self
                        .pads
                        .values_mut()
                        .filter(|p| p.hidden)
                        .filter_map(|p| {
                            p.hidden = false;
                            p.instance.clone()
                        })
                        .collect();
                    for inst in to_unhide {
                        self.unhide(&inst);
                    }
                }
            }
            "hold_seat_seconds" => {
                let secs = value
                    .as_u64()
                    .or_else(|| value.as_f64().map(|f| f.max(0.0) as u64))
                    .unwrap_or(30)
                    .min(300);
                self.settings.hold_seat_seconds = secs;
                self.seats.set_hold_ms(secs * 1000);
            }
            "start_minimized" => self.settings.start_minimized = value.as_bool().unwrap_or(false),
            _ => {
                self.log(format!("Unknown setting: {key}"));
                return;
            }
        }
        if let Err(e) = settings::save(&self.settings) {
            self.log(format!("Could not save settings: {e}"));
        }
    }

    fn shutdown(&mut self) {
        self.stop_probe();
        let ids: Vec<String> = self.pads.keys().cloned().collect();
        for id in ids {
            if let Some(p) = self.pads.remove(&id) {
                let _ = p.out.send(OutMsg::Stop);
                let taken = p.vpad.lock().map(|mut g| g.take()).unwrap_or(None);
                if let Some(v) = taken {
                    v.unplug();
                }
                if p.hidden {
                    if let Some(inst) = p.instance.clone() {
                        self.unhide(&inst);
                    }
                }
            }
        }
        for (_, v) in self.placeholders.drain() {
            v.unplug();
        }
    }

    // ---------- bundled installers ----------

    /// Where the bundled installers live: next to the app in an install, or
    /// the source `drivers` folder during development.
    fn bundle_dir(&self) -> Option<PathBuf> {
        if let Some(dir) = &self.resource_dir {
            if dir.join("drivers").is_dir() {
                return Some(dir.clone());
            }
        }
        #[cfg(debug_assertions)]
        {
            let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            if src.join("drivers").is_dir() {
                return Some(src);
            }
        }
        None
    }

    fn check_bundle(&mut self) {
        let dir = self.bundle_dir();
        self.drivers.bundled = manifest()
            .installers
            .iter()
            .map(|spec| {
                let (status, detail) = match &dir {
                    Some(dir) => match drivers::check(dir, spec) {
                        BundleState::Verified => ("verified", None),
                        BundleState::Missing => ("missing", None),
                        BundleState::Corrupt(e) => ("corrupt", Some(e)),
                    },
                    None => ("missing", Some("no drivers folder next to the app".to_string())),
                };
                BundledInstaller {
                    key: spec.key.clone(),
                    name: spec.name.clone(),
                    version: spec.version.clone(),
                    status: status.to_string(),
                    detail,
                    required: spec.required,
                }
            })
            .collect();
    }

    fn install_driver(&mut self, key: &str) {
        if self.drivers.installing.is_some() {
            self.log("An installer is already running");
            return;
        }
        let Some(spec) = manifest().installers.into_iter().find(|s| s.key == key) else {
            self.log(format!("Unknown driver: {key}"));
            return;
        };
        let Some(dir) = self.bundle_dir() else {
            self.log("The bundled installers are not next to the app; use the download link instead");
            return;
        };
        // Verify again right before running: never execute a file that does
        // not match the manifest byte for byte.
        if drivers::check(&dir, &spec) != BundleState::Verified {
            self.check_bundle();
            self.log(format!("The bundled {} installer failed verification; use the download link instead", spec.name));
            return;
        }
        let path = drivers::installer_path(&dir, &spec);
        self.drivers.installing = Some(key.to_string());
        self.log(format!("Running the {} {} installer (Windows will ask for permission)…", spec.name, spec.version));
        let tx = self.tx.clone();
        let key = key.to_string();
        thread::spawn(move || {
            let result = drivers::run_elevated(&path, &spec.silent_args);
            let _ = tx.send(Event::DriverInstallDone { key, result });
        });
    }

    // ---------- snapshot ----------

    fn pad_view(&self, id: &str, p: &Pad, note: Option<String>) -> ControllerView {
        ControllerView {
            id: id.to_string(),
            kind: p.info.kind,
            name: p.name.clone(),
            transport: p.info.transport,
            battery: p.battery,
            charging: p.charging,
            battery_label: None,
            color: Some(p.color),
            has_light: p.info.kind.has_light(),
            light_off: p.light_off,
            moves: self.seats.allowed_moves(id),
            note,
            is_virtual: true,
            hidden: p.hidden,
        }
    }

    fn build_snapshot(&self) -> Snapshot {
        let now = self.now_ms();
        let slots = (0..SEAT_COUNT)
            .map(|i| match &self.seats.seats()[i] {
                Seat::Empty => SlotView::empty(i),
                Seat::Physical => {
                    let x = self.xbox.get(&i);
                    SlotView {
                        index: i,
                        seat: "controller",
                        held_for: None,
                        held_seconds_left: None,
                        controller: Some(ControllerView {
                            id: format!("xinput:{i}"),
                            kind: Kind::Xbox,
                            name: "Xbox controller".into(),
                            transport: x.map(|x| x.transport).unwrap_or(Transport::Unknown),
                            battery: None,
                            charging: None,
                            battery_label: x.and_then(|x| x.battery_label.clone()),
                            color: None,
                            has_light: false,
                            light_off: false,
                            moves: Vec::new(),
                            note: Some(
                                "Windows gives Xbox controllers their seat in connection order. To move it, reconnect it after the others."
                                    .into(),
                            ),
                            is_virtual: false,
                            hidden: false,
                        }),
                    }
                }
                Seat::Virtual(id) => match self.pads.get(id) {
                    Some(p) => SlotView {
                        index: i,
                        seat: "controller",
                        held_for: None,
                        held_seconds_left: None,
                        controller: Some(self.pad_view(id, p, None)),
                    },
                    None => SlotView::empty(i),
                },
                Seat::Placeholder(h) => {
                    if h.for_id.is_empty() {
                        SlotView { index: i, seat: "placeholder", held_for: None, held_seconds_left: None, controller: None }
                    } else {
                        SlotView {
                            index: i,
                            seat: "held",
                            held_for: Some(h.for_name.clone()),
                            held_seconds_left: h.until_ms.map(|u| u.saturating_sub(now).div_ceil(1000)),
                            controller: None,
                        }
                    }
                }
            })
            .collect();

        let unseated = self
            .pads
            .iter()
            .filter(|(id, _)| self.seats.seat_of(id).is_none())
            .map(|(id, p)| {
                let note = if self.bus.is_none() {
                    "Install ViGEmBus (see Settings) so games can use this controller."
                } else if self.seats.waiting().any(|w| w == id) {
                    "All four seats are taken. It joins as soon as one frees up."
                } else {
                    "Connecting…"
                };
                self.pad_view(id, p, Some(note.into()))
            })
            .collect();

        Snapshot {
            slots,
            unseated,
            drivers: self.drivers.clone(),
            bluetooth: self.bt.clone(),
            settings: SettingsView::from(&self.settings),
            log: self.log.iter().cloned().collect(),
        }
    }

    fn publish(&mut self) {
        self.refresh_probe_target();
        let snap = self.build_snapshot();
        if let Ok(mut g) = self.snapshot.lock() {
            *g = snap.clone();
        }
        let _ = self.app.emit("snapshot", &snap);
    }

    // ---------- live test ----------

    fn stop_probe(&mut self) {
        if let Some(p) = self.probe.take() {
            p.stop();
        }
    }

    /// Where the tested controller currently sits, and where its reports are.
    /// `None` when the id is not a controller we can see.
    fn probe_lookup(&self, id: &str) -> Option<(Kind, Option<usize>, Option<LatestInput>)> {
        if let Some(p) = self.pads.get(id) {
            return Some((p.info.kind, self.seats.seat_of(id).or(p.xinput_index), Some(p.latest.clone())));
        }
        if let Some(i) = xinput_id(id) {
            let seat = i as usize;
            if self.xbox.contains_key(&seat) {
                return Some((Kind::Xbox, Some(seat), None));
            }
        }
        None
    }

    fn start_probe(&mut self, id: &str) -> Result<Layout, String> {
        self.stop_probe();
        let (kind, slot, latest) = self
            .probe_lookup(id)
            .ok_or_else(|| "that controller is not connected right now".to_string())?;

        let tester = Tester::new(kind);
        let layout = tester.layout();
        let target = Arc::new(Mutex::new(ProbeTarget { id: id.to_string(), slot, latest }));
        let active = Arc::new(AtomicBool::new(true));
        let reset = Arc::new(AtomicBool::new(false));

        let app = self.app.clone();
        let thread_target = target.clone();
        let thread_active = active.clone();
        let thread_reset = reset.clone();
        thread::Builder::new()
            .name("probe".into())
            .spawn(move || {
                let mut tester = tester;
                while thread_active.load(Ordering::Relaxed) {
                    if thread_reset.swap(false, Ordering::Relaxed) {
                        tester.reset();
                    }
                    let (id, slot, latest) = {
                        let t = thread_target.lock().expect("probe target");
                        (t.id.clone(), t.slot, t.latest.clone())
                    };
                    let raw = latest.and_then(|l| l.lock().ok().and_then(|g| *g));
                    let game = slot.and_then(xinput::read_state);
                    let frame = tester.feed(&id, slot, raw, game);
                    let _ = app.emit("probe", &frame);
                    thread::sleep(PROBE_INTERVAL);
                }
            })
            .map_err(|e| format!("could not start the test: {e}"))?;

        self.probe = Some(Probe { id: id.to_string(), target, active, reset });
        Ok(layout)
    }

    /// Seats move and controllers leave while a test is open. Keep the probe
    /// pointed at the right slot, and stop it if the controller goes away.
    fn refresh_probe_target(&mut self) {
        let Some(probe) = &self.probe else { return };
        let id = probe.id.clone();
        match self.probe_lookup(&id) {
            Some((_, slot, latest)) => {
                if let Ok(mut t) = probe.target.lock() {
                    t.slot = slot;
                    t.latest = latest;
                }
            }
            None => self.stop_probe(),
        }
    }
}
