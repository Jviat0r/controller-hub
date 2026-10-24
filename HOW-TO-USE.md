# Controller Hub: how to use it

Controller Hub lets you use PlayStation and Xbox controllers on a Windows PC,
up to four at once, without configuring anything. It shows four player cards,
one per seat, and each card tells you what is connected and lets you change
its colour, its player number, or test it.

## Installing

Run **Controller Hub_0.1.0_x64-setup.exe** and click through it. Windows will
ask for permission, because the app installs two drivers that games need.

Along the way it asks whether to install **HidHide**. Say yes unless you have
a reason not to. It stops games from seeing a PlayStation controller twice,
which otherwise makes menus jump and sticks fight each other.

If the installer says a restart is needed, restart when convenient. Until you
do, a driver may not work.

Nothing about this replaces your Xbox controller support. Xbox controllers
already work on Windows, and the app simply shows them.

## Connecting a controller

**Over USB.** Plug it in. A card fills in within a second or two.

**Through a USB hub or splitter.** Nothing special to do. Plug the hub into
one port and up to four controllers into the hub. Each controller is
recognised individually.

**Over Bluetooth.** Click **Pair Bluetooth** at the top right, pick your
controller type, and follow the three steps it shows. In short:

- **DualSense (PS5):** with the controller off, hold **Create** and **PS**
  together until the light flashes quickly.
- **DualShock 4 (PS4):** with the controller off, hold **Share** and **PS**
  together until the light flashes quickly.
- **Xbox:** turn it on, then hold the small pair button on the top edge until
  the Xbox button flashes quickly.

Then click **Scan**, wait a few seconds, and click **Pair** next to your
controller. It stays paired, so next time you only need to turn it on.

Xbox 360 controllers and the original 2013 Xbox One controller have no
Bluetooth at all. Use a cable or the official Xbox wireless adapter.

## Player numbers

The card titles are the player numbers games will use. Player 1 is the first
card.

If someone ends up as the wrong player, open the **Move to** menu on their
card and pick the seat you want. Everyone else keeps their number.

Xbox controllers are the exception. Windows decides their player number and
the app cannot change it. To move one, disconnect it and reconnect it after
the others.

If a controller dies or wanders out of range, its seat is held for thirty
seconds so it comes back as the same player. Nobody shifts down while it is
away. You can release a held seat early with the button on its card, or
change the thirty seconds in Settings.

## Lights

On PlayStation controllers, pick any colour with the **Light colour** swatch.
The choice sticks to that controller, whether it connects by cable or
Bluetooth.

Tick **Turn the light bar off** if you would rather it stayed dark, for
battery or for comfort. On a DualSense the small player-number lights stay
on, since those are how you tell the seats apart.

**Identify** flashes the light and buzzes the controller, so you can tell
which one is which when four people are holding them.

## Testing a controller

Press **Test** on any card. This is the thing to use when something feels
wrong.

It shows two columns. The left is what the controller sent. The right is what
a game receives. Move a stick or press a button and watch both.

- **Both move:** everything works. Games will see it.
- **Only the left moves:** the controller is fine, but something between it
  and games is not. The test says so on screen.
- **Neither moves:** the controller or its cable or Bluetooth link is the
  problem.

An Xbox controller shows one column, because Windows handles it directly and
there is nothing to compare.

Below the columns:

- **Stick drift.** Let go of both sticks and wait a moment. The test says
  whether a stick sits far enough off centre to move a game on its own. Small
  amounts are normal and are called harmless.
- **Button check.** Press every button once. Each one ticks off, and anything
  you have not tried yet is listed by name.

Press **Start over** to clear what the test has recorded and go again.

## Running and quitting

Closing the window does **not** quit. The app keeps running in the system
tray so your controllers stay connected while you play. Click the tray icon
to bring the window back.

To quit properly, right click the tray icon and choose **Quit**. Virtual
controllers are removed cleanly and any hidden controller is made visible
again.

Settings has an option to start minimised to the tray.

## If something is not working

**A card says "No controller" but it is plugged in.** Try a different USB
port, directly on the PC rather than through a hub, to rule the hub out.

**The driver status in Settings says "Not installed".** Open Settings and
press Install next to it. If the button is not there, use the download link
next to it instead.

**A game sees my controller twice, or the menu jumps.** Install HidHide from
Settings, then tick **Hide PlayStation pads from games**.

**A game does not see the controller at all.** Press Windows key + R, type
`joy.cpl`, and press Enter. Your controller should be listed as an Xbox 360
controller, and its properties page has a live test. If it works there but
not in the game, the problem is in the game or in Steam's own controller
settings, not here.

**A PlayStation controller works by cable but not by Bluetooth.** Open the
test view over Bluetooth and check the left column. If it stays blank, the
controller is connected but not sending readable reports. Say so in a bug
report and mention which controller model it is.

**Something else.** The line at the bottom of the window shows the last thing
that happened, which is usually the best clue.
