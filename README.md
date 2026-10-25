# Controller Hub

A small Windows app that makes up to four PlayStation and Xbox controllers work in games, with no configuration.

It shows one screen: four player seats, a light bar colour picker and a Bluetooth pairing helper. Think DS4Windows with most of the knobs removed.

> **Status:** version 0.1.0. Everything is covered by automated tests, but no physical controller has been connected to a build yet. Treat it as ready to test, not ready to rely on.

## Features

- **PlayStation pads** (DualShock 4, DualSense, DualSense Edge) over USB or Bluetooth are read directly over HID and shown to games as a virtual Xbox 360 controller through the ViGEmBus driver, so any XInput game sees a normal pad. Light bar colour, DualSense player LEDs and game rumble are passed through.
- **Xbox pads** (360, One, Series) use the driver already in Windows. The hub shows them in their seat with a coarse battery reading and can buzz them with Identify.
- **Four seats that match XInput slots**, so what the hub shows is what a game sees. A PlayStation pad can be moved to another seat from its card.
- **Seat holding.** When a PlayStation pad drops (battery swap, out of range) its seat is held for 30 seconds by an idle placeholder, so it comes back as the same player and nobody else shifts down. The length is configurable and 0 turns it off.
- **Bluetooth pairing** inside the app: pick the controller family, follow the steps, Scan, Pair.
- **Same pad, same identity.** Each pad is recognised by its Bluetooth address, read over USB too, so it keeps its name and colour whether it connects by cable or Bluetooth. A pad connected both ways at once is used once, cable preferred. USB hubs and splitters need nothing special.
- **Live test** per controller: what the controller sent next to what games receive through XInput, plus stick drift against Microsoft's documented deadzones and a button check.
- **Light bar off** per controller, remembered between sessions.
- **Optional HidHide** support so games never see a PlayStation pad twice.
- Runs in the tray when the window is closed. Quit from the tray menu.

## Requirements

- Windows 10 or 11, 64-bit. The bundled HidHide installer is x64 only.
- [ViGEmBus](https://github.com/nefarius/ViGEmBus) (required for PlayStation pads). The installer built from this repo installs ViGEmBus 1.22.0 for you. If you run the app another way, install it from the [official releases](https://github.com/nefarius/ViGEmBus/releases).
- [HidHide](https://github.com/nefarius/HidHide) (optional). Offered during install, or later from Settings.
- Bluetooth pairing needs a working Bluetooth radio.

Both drivers are by Nefarius Software Solutions. ViGEmBus is no longer maintained by its author; it still works on current Windows and is what comparable tools ship.

## Build from source

You need:

- [Node.js](https://nodejs.org/) 22 or newer
- [Rust](https://rustup.rs/) stable (MSVC toolchain)
- The [Tauri 2 prerequisites for Windows](https://v2.tauri.app/start/prerequisites/): Microsoft C++ Build Tools and WebView2 (already on Windows 11)

```
npm ci
npm run dev
```

`npm run dev` starts the Vite dev server on port 1420 and opens the app. To work on the UI without Tauri or any hardware, run `npm run ui`; it runs against a clearly labelled demo bridge.

Release build:

```
npm run build
```

This produces an NSIS installer in `src-tauri/target/release/bundle/nsis/`. A release build fails if a driver installer in `src-tauri/drivers/` is missing or does not match the size and SHA-256 pinned in `src-tauri/drivers/manifest.json`.

Tests:

```
npm test
cd src-tauri
cargo test --workspace
```

`npm test` runs the UI tests (Vitest). `cargo test --workspace` runs the Rust core and app tests. Tests that need real hardware or the ViGEmBus driver are marked `#[ignore]`; run them with `cargo test --test platform_smoke -- --ignored --nocapture` and `cargo test --test vigem_smoke -- --ignored --nocapture`.

## How it works

```
PlayStation pad --HID (hidapi)--> parse report --> map to XInput --> ViGEmBus virtual Xbox 360 pad --> game
                 <-- light bar, player LEDs, rumble (Bluetooth output reports carry a CRC-32) <--
```

- `ui/` is plain JavaScript with no runtime dependencies. It renders a snapshot from the engine and sends intents back. It never decides anything.
- `src-tauri/core` is pure Rust with no OS calls: report parsing, CRC, XInput mapping, the seat manager, the live test verdicts and the JSON model. This is where most of the tests are.
- `src-tauri/src` is the platform layer: hidapi for HID, vigem-client for ViGEmBus, XInput, WinRT Bluetooth pairing, the HidHide CLI, the engine thread and the Tauri shell (window, tray, commands).

**Seats.** XInput gives a newly plugged pad, real or virtual, the lowest free slot. The only lever the hub has is when it plugs its own virtual pads. A PlayStation pad that arrives is plugged straight away. When it leaves, a placeholder virtual pad is plugged into the seat it freed, so the seat stays taken until the pad returns or the hold runs out. Moving a pad to a higher seat may need placeholders in the gaps, shown as "Reserved seat". A physical Xbox pad owns its seat and the hub will not plug over it.

**Bluetooth.** Pairing uses the Windows WinRT device APIs. The scan filters to devices that look like controllers. Once paired, a PlayStation pad over Bluetooth is read over HID the same way as over USB.

**Drivers.** The official signed ViGEmBus and HidHide installers are bundled in `src-tauri/drivers/` with their licence texts and pinned by size and SHA-256 in `manifest.json`. `build.rs` checks them and generates the NSIS install hook from the same manifest, and the app checks the hashes again before it runs one.

Settings are stored in `%APPDATA%\ControllerHub\settings.json`.

The full design notes, including the seat rules and rejected alternatives, are in [docs/DESIGN.md](docs/DESIGN.md). End-user instructions are in [HOW-TO-USE.md](HOW-TO-USE.md). Release history is in [CHANGELOG.md](CHANGELOG.md).

## Known limitations

- Not yet tested on real controllers. Report layouts follow the Linux `hid-sony` and `hid-playstation` drivers.
- Xbox pads cannot be moved between seats. Windows decides their slot; reconnect them in the order you want.
- Seat holding only helps PlayStation pads, and only when the freed seat is the lowest free one.
- A held seat is a real idle virtual pad, so a game sees an extra connected controller during the hold.
- Touchpad click, the mute button and adaptive triggers have no Xbox 360 equivalent and are dropped.
- The live test proves input as far as XInput. Games using Steam Input or DirectInput can behave differently.
- Xbox battery is only empty, low, medium or full, because that is all XInput reports.
- Without HidHide, games that read raw HID may see a PlayStation pad twice.
- The DualSense Edge is treated as a DualSense, with no back paddle support.
- The hub cannot put a pad into pairing mode, and Xbox 360 and original 2013 Xbox One pads have no Bluetooth.

## License

Controller Hub is released under the [MIT License](LICENSE).

The bundled driver installers are third-party software under their own licences: ViGEmBus is BSD-3-Clause ([src-tauri/drivers/LICENSE-ViGEmBus.txt](src-tauri/drivers/LICENSE-ViGEmBus.txt)) and HidHide is MIT ([src-tauri/drivers/LICENSE-HidHide.txt](src-tauri/drivers/LICENSE-HidHide.txt)).
