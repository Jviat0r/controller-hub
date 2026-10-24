//! Hardware smoke test. Needs the ViGEmBus driver installed, and proves the
//! assumption the whole seat model rests on: XInput hands a new pad the
//! lowest free slot and reuses a freed one.
//!
//! Ignored by default so `cargo test` stays hardware-free. Run it with:
//! `cargo test --test vigem_smoke -- --ignored --nocapture`

use std::{thread, time::Duration};

use controller_hub_core::xmap::{xbtn, XPad};
use controller_hub_lib::{
    vigem::{Bus, VirtualPad},
    xinput,
};

#[test]
#[ignore]
fn plug_order_follows_lowest_free_xinput_slot() {
    let bus = Bus::connect().expect("ViGEmBus is not installed or not running");
    let before = xinput::connected();
    println!("XInput slots occupied before: {before:?}");

    let (a, ia) = VirtualPad::plug(&bus).expect("plug first pad");
    let (b, ib) = VirtualPad::plug(&bus).expect("plug second pad");
    println!("first pad -> XInput slot {ia}, second -> {ib}");
    assert!(ib > ia, "second pad must land in a higher slot than the first");
    assert!(!before[ia] && !before[ib], "pads must land in slots that were free");

    thread::sleep(Duration::from_millis(300));
    let now = xinput::connected();
    assert!(now[ia] && now[ib], "XInput must report both virtual pads as connected: {now:?}");

    a.unplug();
    thread::sleep(Duration::from_millis(700));
    assert!(!xinput::connected()[ia], "slot {ia} must free up after unplug");

    let (mut c, ic) = VirtualPad::plug(&bus).expect("plug third pad");
    println!("third pad after unplugging the first -> {ic}");
    assert_eq!(ic, ia, "a freed lower slot is reused by the next plug");

    c.update(&XPad { buttons: xbtn::A, ..Default::default() });
    thread::sleep(Duration::from_millis(100));

    c.unplug();
    b.unplug();
    thread::sleep(Duration::from_millis(500));
    let after = xinput::connected();
    assert!(!after[ia] && !after[ib], "everything unplugged again: {after:?}");
}
