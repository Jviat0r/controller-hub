# Changelog

All notable changes to Controller Hub are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[semantic versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- MIT licence, a public README and GitHub Actions CI (UI build, Vitest,
  `cargo test --workspace` on Windows). The old README design document moved
  to `docs/DESIGN.md`.

## [0.1.0] — 2026-09-20

First build. Everything below is implemented and covered by automated tests,
but **no physical controller has been connected to it yet**. Treat this
version as ready to test, not ready to rely on.

### Added

**Controllers**

- DualShock 4 (2013 and 2016 models), DualSense, and DualSense Edge over USB
  and Bluetooth, read directly over HID and presented to games as a virtual
  Xbox 360 pad through ViGEmBus.
- Xbox 360, Xbox One, and Xbox Series controllers shown in their seat using
  the driver already built into Windows. No extra driver needed for these.
- Each controller is recognised by its Bluetooth address, which is read over
  USB as well, so the same pad keeps its name and colour whichever way it
  connects. A pad connected by cable and by Bluetooth at once is used once,
  with the cable preferred.
- Battery level and charging state for PlayStation pads. Xbox pads report
  only empty, low, medium, or full, and are shown in those words rather than
  as an invented percentage.

**Player seats**

- Four seats that mirror Windows' own XInput slots, so what the app shows is
  what a game receives.
- A seat is held for thirty seconds when a PlayStation pad disconnects, so it
  rejoins as the same player and nobody else shifts down. The hold length is
  configurable, and zero turns it off.
- Any PlayStation pad can be moved to another seat, with the seats below it
  left untouched so other players never see a disconnect.
- Gaps created by a move are shown as reserved seats and can be released.
- A fifth controller waits for a seat and joins automatically when one frees
  up.

**Lights**

- Light bar colour picker per controller, remembered between sessions.
- The light bar can be switched off per controller. DualSense player-number
  lights stay on, since they are how seats are told apart.
- DualSense player-number lights follow the seat.
- Identify flashes the light and buzzes the motors so you can tell which pad
  is which. Xbox pads buzz.
- Rumble sent by a game is forwarded back to the physical pad.

**Bluetooth**

- A pairing helper with the exact button sequence for each controller family,
  a scan that filters to things that look like controllers, and pair and
  unpair without leaving the app.

**Live test**

- A test view per controller showing what the controller sent beside what
  games receive, read back through the same call a game makes, so a fault can
  be attributed to the controller or to this app rather than guessed at.
- Stick drift measured as distance from centre and judged against the
  deadzones Microsoft documents, keeping the worst reading so a brief twitch
  is still caught.
- A button check that ticks off each button as it is pressed, on both sides,
  and names the ones still untried.

**Drivers**

- The official signed installers for ViGEmBus 1.22.0 and HidHide 1.5.230 ship
  inside the installer, with their licence texts. Both are pinned by size and
  SHA-256 in a single manifest.
- The build fails if either bundled installer is missing or altered, and the
  app checks the same hashes again before it will run one.
- ViGEmBus is installed automatically. HidHide is offered as a choice, since
  it is optional.
- A driver that is missing later can be installed from Settings, or reached
  through a link to its official download page if the bundled copy is
  unusable.
- Uninstalling turns HidHide cloaking off and removes the app from its
  allow-list, so no controller is ever left invisible. The drivers themselves
  are left in place, because other apps may be using them.

**Application**

- Optional HidHide support so games never see a PlayStation pad twice.
  Controllers hidden by a session that ended badly are restored on the next
  start.
- Closing the window keeps the app running in the tray so controllers stay
  connected. Quit from the tray menu.
- Optional start minimised to the tray.
- Settings stored in `%APPDATA%\ControllerHub\settings.json`, written
  atomically so a crash cannot corrupt them.

### Known limitations

- Never tested against real hardware. Report layouts follow the Linux
  `hid-sony` and `hid-playstation` drivers.
- Xbox controllers cannot be moved between seats. Windows decides their slot,
  and the only way to change it is to reconnect them in the order you want.
- Seat holding applies to PlayStation pads only, and only when the freed seat
  is the lowest free one.
- A held seat is a real, idle virtual pad, so a game sees an extra connected
  controller for the length of the hold.
- Touchpad click, the mute button, and adaptive triggers have no Xbox 360
  equivalent and are dropped.
- The live test proves input as far as XInput. A game using Steam Input or
  DirectInput can still behave differently.
- The DualSense Edge is treated as a DualSense, with no back paddle support.
- Without HidHide, games that read raw HID may see a PlayStation pad twice.
- ViGEmBus is no longer maintained by its author. It works on current Windows
  and is what comparable tools ship, but it is a dependency worth watching.
