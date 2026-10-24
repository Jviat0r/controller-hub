# Controller Hub design notes

This is the long design document that used to be the README. For a short
overview and build steps see the [README](../README.md). For day-to-day use
see [HOW-TO-USE.md](../HOW-TO-USE.md).

## What it does

- **Xbox 360 / One / Series** pads: nothing to install. Windows already
  drives them. The hub shows them in their seat with a coarse battery reading
  and lets you buzz them with *Identify*.
- **DualShock 4 / DualSense / DualSense Edge** pads over USB or Bluetooth:
  read directly over HID and re-emitted as a virtual Xbox 360 pad through the
  ViGEmBus driver, so every XInput game sees a normal controller. Light bar
  colour, DualSense player LEDs, and rumble from the game all work.
- **Seats (Player 1–4)** follow XInput slots exactly, so what the hub shows is
  what the game sees. When a PlayStation pad drops (battery swap, out of
  range), its seat is *held* for 30 seconds by a silent placeholder so it
  comes back as the same player and nobody else shifts down. You can move a
  PlayStation pad to another seat from its card.
- **Bluetooth pairing** without the Windows settings maze: pick the
  controller family, follow three steps, press Scan, press Pair.
- **USB hubs / 4-port splitters**: nothing special needed. Each pad is
  identified by its Bluetooth MAC (read over USB too), so the same physical
  pad keeps its name and colour whether it arrives by cable or by air, and the
  hub ignores the second link if a pad is connected both ways at once.
- **A live test** on every controller card answers "is this actually
  working?" without guessing. See below.
- **The light bar can be switched off** per controller and stays off, for
  battery or for people who just don't want a glowing pad. DualSense player
  number LEDs stay on, because they are how you tell the seats apart.
- **Optional HidHide** support hides the raw PlayStation device from games so
  they never see the pad twice.
- Closing the window minimises to the tray; controllers keep working. Quit
  from the tray menu.

## The live test

Press **Test** on any controller card. It shows two columns side by side:

- **From the controller.** The report read straight off the pad over USB or
  Bluetooth, before this app touches it.
- **What games see.** The same slot read back through XInput, which is the
  identical call a game makes.

That split is the whole point. If the left column moves and the right one
does not, the fault is this app or the virtual pad driver, and the test says
so in those words. If neither moves, the controller or its link is the
problem. An Xbox pad shows one column, because Windows drives it directly and
there is nothing to compare.

Underneath:

- **Stick drift** is measured as distance from centre in XInput units and
  judged against the deadzones Microsoft documents. A wander inside the
  deadzone is called harmless; past it, the test says a game will read it as
  movement. The worst reading is kept, so a stick that twitches once is still
  caught.
- **Button check** ticks off every button as it is pressed, on both sides,
  and names the ones still untried.

`src-tauri/core/src/tester.rs` holds every judgement, with tests. The UI only
describes what it is told, so a verdict can never drift between the two.

## Drivers

Two kernel drivers by Nefarius Software Solutions do the parts a user-mode
app cannot. Their official signed installers ship inside this app's
installer; the app never builds or modifies them.

| Driver | Needed for | Licence | Bundled |
| --- | --- | --- | --- |
| ViGEmBus 1.22.0 | PlayStation pads showing up as Xbox pads | BSD-3-Clause | always installed |
| HidHide 1.5.230 | Hiding the raw PlayStation device from games | MIT | asked at install time; may need a reboot |

`src-tauri/drivers/manifest.json` is the single source of truth: file name,
version, size, SHA-256, silent flags, licence file. Three things are derived
from it and nothing else names those files:

- `build.rs` verifies every installer byte-for-byte against the manifest. A
  release build fails if one is missing or altered; a dev build only warns so
  a fresh clone can still run tests.
- `build.rs` generates `windows/hooks.nsh`, the NSIS hook that runs the
  setups (`/exenoui /qn /norestart`) right after the app files are copied.
  Uninstalling turns HidHide cloaking off and removes the app from its
  allow-list so no controller is ever left invisible; the drivers themselves
  stay, because other apps may use them.
- At run time the app verifies the same hashes again, shows the result in
  Settings, and offers "Install" (through UAC) only for a verified file. If a
  file is missing or fails verification it falls back to the official
  download link.

The app also remembers which devices it hid and unhides them on the next
start if it did not exit cleanly.

Both licence texts are installed alongside the drivers in the `drivers`
folder. To upgrade a driver: drop the new official file in `src-tauri/drivers`,
update the manifest entry, done.

## How it is put together

```
ui/            plain JS, zero runtime deps. Renders a snapshot, sends intent.
               tester.js + testview.js are the live test's formatting and DOM.
src-tauri/core pure Rust, no OS calls. Report parsing, CRC, XInput mapping,
               the seat manager, the JSON model. `cargo test` covers it.
src-tauri/src  the platform layer: hidapi (HID), vigem-client (ViGEmBus),
               XInput, WinRT Bluetooth pairing, HidHide CLI, the engine
               thread, and the Tauri shell (window, tray, commands).
```

The rule is *pure core, dumb UI*: the UI never decides whether a move is
allowed (the engine sends `moves` and `note` per controller), and the core
never touches a device (it returns `Action`s that the engine carries out and
reports back on). Unknown values stay unknown all the way to the screen: a
battery we have not read says "Battery unknown", never 0%.

## The seat model

XInput assigns slots itself: a newly plugged pad, real or virtual, takes the
lowest free index and keeps it until it leaves. The only lever the hub has is
*when* it plugs its own virtual pads. So:

- A PlayStation pad that arrives is plugged straight away and lands in the
  lowest free seat.
- When it leaves, a placeholder virtual pad is plugged into the seat it just
  freed, for `hold_seat_seconds` (default 30). If the pad returns in time, the
  placeholder is pulled and the pad lands back in the same seat. If the seat
  was not the lowest free one, no hold is attempted, because Windows would put
  the placeholder somewhere else.
- Moving a pad to a higher seat may need placeholders in the gaps; those are
  shown as "Reserved seat" and stay until you release them.
- A physical Xbox pad owns its seat. The hub will not plug over it, and
  refuses moves onto it with a plain explanation.

`src-tauri/core/src/seats.rs` is the whole rule set, with tests for each of
those sentences.

## Rejected alternatives

- **C# + WPF (what DS4Windows uses).** The most proven stack for this domain,
  but it does not fit the pure-core/dumb-UI split as cleanly, and the UI would
  have grown the way DS4Windows' did. Rust for the driver layer plus a plain
  HTML UI keeps the two halves honest.
- **Electron.** Native HID and ViGEm bindings through Node addons are painful
  to build and ship. Tauri gives the same UI freedom with a Rust backend.
- **Reading Xbox pads over raw HID and re-emitting them too.** Would let the
  hub reorder Xbox seats, but Windows exposes Xbox pads through XUSB/GIP, not
  HID, so HidHide cannot hide the original and games would see two pads.
- **Remapping, macros, profiles, gyro, touchpad-as-mouse.** DS4Windows does
  all of it. This app deliberately does none of it; the goal is four
  controllers working with no configuration.
- **Detecting the drivers and only linking to their download pages.** This
  was the first plan. 0.1.0 replaced it with shipping the authors' official
  signed installers, pinned by hash (see Drivers above). The download links
  remain as the fallback.

## Honest limitations

- **Not yet tested on real controllers.** The report layouts follow the Linux
  hid-sony / hid-playstation drivers and the ViGEm virtual pad path was
  exercised, but as of 2026-09-20 no DualShock, DualSense or Xbox pad has been
  plugged into a build of this app. Expect at least one round of fixes when
  that happens.
- **Xbox pads cannot be moved between seats.** Windows decides their slot.
  The card says so and explains the workaround (reconnect them in the order
  you want).
- **Seat holding only helps PlayStation pads,** and only when the freed seat
  is the lowest free one. Mixed setups can still shuffle if an Xbox pad and a
  PlayStation pad drop at the same time.
- **A placeholder is a real (idle) virtual pad.** During a 30-second hold a
  game sees an extra connected controller that does nothing.
- **Bluetooth pairing needs the controller in pairing mode** and a working
  Bluetooth radio; the hub cannot push a pad into pairing mode. Original
  2013 Xbox One pads and Xbox 360 pads have no Bluetooth at all.
- **Touchpad click, mute button and adaptive triggers** have no Xbox 360
  equivalent and are dropped. The live test shows them arriving on the
  controller side and correctly absent on the game side.
- **The live test proves input up to XInput, not past it.** A game using
  Steam Input or DirectInput can still behave differently. Windows' own
  `joy.cpl` is the second opinion.
- **Xbox battery is coarse** (empty/low/medium/full) because that is all
  XInput reports. It is shown as those words, not as a percentage.
- **Without HidHide,** games that read raw HID (some non-XInput titles, Steam
  Input) may see the PlayStation pad twice.
- The DualSense Edge is treated exactly like a DualSense (no back paddles).
