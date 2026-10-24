//! Platform smoke tests that touch the real machine (HID stack, Bluetooth
//! radio). They need no controller: they prove the OS calls work and print
//! what they saw. Ignored by default; run with
//! `cargo test --test platform_smoke -- --ignored --nocapture`

use std::{path::Path, time::Duration};

use controller_hub_core::bundle::manifest;
use controller_hub_lib::{
    bluetooth,
    drivers::{check, BundleState},
    hid,
};
use hidapi::HidApi;

/// Not ignored: needs no hardware. The exact code path the app uses at run
/// time must accept the files we ship and reject a tampered one.
#[test]
fn bundled_installers_verify_with_the_runtime_check() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let m = manifest();
    assert!(!m.installers.is_empty());
    for spec in &m.installers {
        assert_eq!(check(dir, spec), BundleState::Verified, "{} must be present and untouched", spec.file);
        assert!(dir.join("drivers").join(&spec.license).exists(), "{} licence must ship", spec.name);
    }
    let mut tampered = m.installers[0].clone();
    tampered.sha256 = "0".repeat(64);
    assert!(matches!(check(dir, &tampered), BundleState::Corrupt(_)));
    let mut absent = m.installers[0].clone();
    absent.file = "does-not-exist.exe".into();
    assert_eq!(check(dir, &absent), BundleState::Missing);
}

#[test]
#[ignore]
fn hid_enumeration_runs() {
    let mut api = HidApi::new().expect("open the HID stack");
    api.refresh_devices().expect("refresh");
    let total = api.device_list().count();
    let sony = hid::enumerate(&api);
    println!("HID devices on this PC: {total}; PlayStation pads: {}", sony.len());
    for pad in &sony {
        println!("  {:?} over {:?} at {}", pad.kind, pad.transport, pad.path);
    }
    assert!(total > 0, "a Windows PC always has at least one HID device");
}

#[test]
#[ignore]
fn bluetooth_scan_runs() {
    let list = bluetooth::scan(Duration::from_secs(4)).expect("Bluetooth scan");
    println!("Bluetooth devices seen: {}", list.len());
    for d in &list {
        println!("  {:?} paired={:?} id={}", d.name, d.paired, d.id);
    }
    // Every entry must carry a real pairing state; unknown is allowed but must be explicit.
    for d in &list {
        assert!(!d.id.is_empty());
    }
}
