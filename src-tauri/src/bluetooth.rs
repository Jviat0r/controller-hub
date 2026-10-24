//! Bluetooth discovery and pairing through the Windows Runtime device APIs.
//! Controllers pair with the "confirm only" ceremony (no PIN), so we accept
//! the request automatically. Windows still does the actual pairing.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use controller_hub_core::model::BtDevice;
use windows::{
    core::{Ref, HSTRING},
    Devices::Enumeration::{
        DeviceInformation, DeviceInformationCustomPairing, DeviceInformationKind, DeviceInformationUpdate,
        DevicePairingKinds, DevicePairingRequestedEventArgs, DevicePairingResultStatus, DeviceWatcher,
    },
    Foundation::TypedEventHandler,
    Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
};
use windows_collections::IIterable;

/// Classic Bluetooth (BR/EDR) association endpoints; controllers are classic, not BLE.
const BT_CLASSIC_AQS: &str = r#"System.Devices.Aep.ProtocolId:="{e0cbf06c-cd8b-4647-bb8a-263b43f0f974}""#;

fn cache() -> &'static Mutex<HashMap<String, DeviceInformation>> {
    static CACHE: OnceLock<Mutex<HashMap<String, DeviceInformation>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn init_com() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn err(e: windows::core::Error) -> String {
    format!("{e}")
}

/// Run an inquiry scan for `duration` and return everything seen.
pub fn scan(duration: Duration) -> Result<Vec<BtDevice>, String> {
    init_com();
    let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
        &HSTRING::from(BT_CLASSIC_AQS),
        None::<&IIterable<HSTRING>>,
        DeviceInformationKind::AssociationEndpoint,
    )
    .map_err(err)?;

    let found: Arc<Mutex<Vec<BtDevice>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = found.clone();
    watcher
        .Added(&TypedEventHandler::<DeviceWatcher, DeviceInformation>::new(move |_, info: Ref<DeviceInformation>| {
            if let Some(info) = info.as_ref() {
                let id = info.Id()?.to_string();
                let name = info.Name()?.to_string();
                let paired = info.Pairing().and_then(|p| p.IsPaired()).ok();
                cache().lock().unwrap().insert(id.clone(), info.clone());
                let mut list = sink.lock().unwrap();
                match list.iter_mut().find(|d| d.id == id) {
                    Some(existing) => *existing = BtDevice { id, name, paired },
                    None => list.push(BtDevice { id, name, paired }),
                }
            }
            Ok(())
        }))
        .map_err(err)?;
    // Windows only keeps delivering Added events when Updated/Removed are handled too.
    watcher
        .Updated(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(|_, _| Ok(())))
        .map_err(err)?;
    watcher
        .Removed(&TypedEventHandler::<DeviceWatcher, DeviceInformationUpdate>::new(|_, _| Ok(())))
        .map_err(err)?;

    watcher.Start().map_err(err)?;
    thread::sleep(duration);
    let _ = watcher.Stop();
    thread::sleep(Duration::from_millis(300));

    let mut list = found.lock().unwrap().clone();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

fn lookup(id: &str) -> Result<DeviceInformation, String> {
    if let Some(info) = cache().lock().unwrap().get(id).cloned() {
        return Ok(info);
    }
    DeviceInformation::CreateFromIdAsyncWithKindAndAdditionalProperties(
        &HSTRING::from(id),
        None::<&IIterable<HSTRING>>,
        DeviceInformationKind::AssociationEndpoint,
    )
    .map_err(err)?
    .join()
    .map_err(err)
}

pub fn pair(id: &str) -> Result<String, String> {
    init_com();
    let info = lookup(id)?;
    let pairing = info.Pairing().map_err(err)?;
    if pairing.IsPaired().map_err(err)? {
        return Ok("Already paired.".into());
    }
    if !pairing.CanPair().map_err(err)? {
        return Err("Windows says this device cannot be paired right now. Put it in pairing mode and scan again.".into());
    }
    let custom = pairing.Custom().map_err(err)?;
    custom
        .PairingRequested(&TypedEventHandler::<DeviceInformationCustomPairing, DevicePairingRequestedEventArgs>::new(
            |_, args: Ref<DevicePairingRequestedEventArgs>| {
                if let Some(a) = args.as_ref() {
                    a.Accept()?;
                }
                Ok(())
            },
        ))
        .map_err(err)?;
    let kinds = DevicePairingKinds::ConfirmOnly | DevicePairingKinds::ConfirmPinMatch | DevicePairingKinds::DisplayPin;
    let result = custom.PairAsync(kinds).map_err(err)?.join().map_err(err)?;
    let status = result.Status().map_err(err)?;
    if status == DevicePairingResultStatus::Paired || status == DevicePairingResultStatus::AlreadyPaired {
        Ok("Paired. The controller should connect within a few seconds.".into())
    } else {
        Err(format!("Pairing failed: {}.", status_text(status)))
    }
}

pub fn unpair(id: &str) -> Result<String, String> {
    init_com();
    let info = lookup(id)?;
    let result = info.Pairing().map_err(err)?.UnpairAsync().map_err(err)?.join().map_err(err)?;
    let status = result.Status().map_err(err)?.0;
    // 0 = Unpaired, 1 = AlreadyUnpaired
    if status == 0 || status == 1 {
        Ok("Unpaired.".into())
    } else {
        Err(format!("Unpair failed (status {status})."))
    }
}

fn status_text(s: DevicePairingResultStatus) -> &'static str {
    match s.0 {
        1 => "the controller is not ready to pair",
        2 => "not paired",
        4 => "the controller rejected the connection",
        5 => "too many connections",
        6 => "hardware failure",
        7 => "timed out waiting for the controller",
        8 => "authentication not allowed",
        9 => "authentication failed",
        10 => "no supported profiles",
        12 => "access denied",
        14 => "cancelled",
        15 => "another pairing is already in progress",
        17 => "rejected",
        18 => "the controller is already associated with another device",
        _ => "failed",
    }
}
