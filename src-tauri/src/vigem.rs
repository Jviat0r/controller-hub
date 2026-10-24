//! Virtual Xbox 360 pads through the ViGEmBus kernel driver. One virtual pad
//! per seated PlayStation controller, plus silent placeholders that keep a
//! seat warm.

use std::{sync::Arc, thread, time::Duration};

use controller_hub_core::xmap::XPad;
use vigem_client::{Client, TargetId, XButtons, XGamepad, XNotification, Xbox360Wired};

pub struct Bus {
    client: Arc<Client>,
}

impl Bus {
    pub fn connect() -> Result<Bus, String> {
        Client::connect()
            .map(|c| Bus { client: Arc::new(c) })
            .map_err(|e| format!("{e:?}"))
    }
}

pub struct VirtualPad {
    target: Xbox360Wired<Arc<Client>>,
}

impl VirtualPad {
    /// Plug a new pad in and report the XInput slot Windows gave it.
    pub fn plug(bus: &Bus) -> Result<(VirtualPad, usize), String> {
        let mut target = Xbox360Wired::new(bus.client.clone(), TargetId::XBOX360_WIRED);
        target.plugin().map_err(|e| format!("plugin: {e:?}"))?;
        // ViGEmBus 1.17 answers the "wait until ready" request with a hardware
        // error even though the pad comes up fine, so treat it as advisory and
        // rely on polling the XInput slot, which only succeeds once the device
        // has actually started.
        let wait_note = target.wait_ready().err().map(|e| format!("wait_ready: {e:?}"));
        let mut last = None;
        for _ in 0..60 {
            match target.get_user_index() {
                Ok(i) => return Ok((VirtualPad { target }, i as usize)),
                Err(e) => {
                    last = Some(e);
                    thread::sleep(Duration::from_millis(50));
                }
            }
        }
        let _ = target.unplug();
        Err(format!(
            "could not learn the XInput slot: {last:?}{}",
            wait_note.map(|n| format!(" ({n})")).unwrap_or_default()
        ))
    }

    pub fn update(&mut self, p: &XPad) {
        let _ = self.target.update(&XGamepad {
            buttons: XButtons { raw: p.buttons },
            left_trigger: p.lt,
            right_trigger: p.rt,
            thumb_lx: p.lx,
            thumb_ly: p.ly,
            thumb_rx: p.rx,
            thumb_ry: p.ry,
        });
    }

    pub fn unplug(mut self) {
        let _ = self.target.unplug();
    }

    /// Forward rumble the game sends to the virtual pad. The thread ends when
    /// the pad is unplugged.
    pub fn on_rumble(&mut self, mut f: impl FnMut(u8, u8) + Send + 'static) -> Result<(), String> {
        let notification = self.target.request_notification().map_err(|e| format!("{e:?}"))?;
        notification.spawn_thread(move |_, n: XNotification| f(n.large_motor, n.small_motor));
        Ok(())
    }
}
