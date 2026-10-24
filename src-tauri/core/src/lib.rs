//! Controller Hub core: pure rules with no OS calls.
//!
//! Everything in here is testable on any machine: HID report parsing for
//! Sony pads, output report building (light bar, rumble, player LEDs),
//! XInput mapping, the seat (player slot) manager, and the JSON model the
//! UI renders. The Tauri app crate wires these to hidapi, ViGEmBus, XInput
//! and Bluetooth.

pub mod bundle;
pub mod crc;
pub mod devpath;
pub mod ids;
pub mod input;
pub mod model;
pub mod output;
pub mod seats;
pub mod sha256;
pub mod tester;
pub mod xmap;
