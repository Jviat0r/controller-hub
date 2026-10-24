// Pure view-model helpers for the UI. No DOM, no Tauri, no rules.
// The engine (Rust) decides everything; this file only labels and formats
// what it was told. Unknown values must read as unknown, never as 0 or "".

export const SLOT_COUNT = 4;

const ESCAPES = { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" };

/// Every value that reaches innerHTML goes through here first.
export function esc(s) {
  return String(s ?? "").replace(/[&<>"']/g, (c) => ESCAPES[c]);
}

const KIND_LABEL = {
  ds4: "DualShock 4",
  dualsense: "DualSense",
  dualsense_edge: "DualSense Edge",
  xbox: "Xbox controller",
};

const TRANSPORT_LABEL = {
  usb: "USB",
  bluetooth: "Bluetooth",
  wireless: "Wireless",
  unknown: "Unknown link",
};

export function kindLabel(kind) {
  return KIND_LABEL[kind] ?? "Controller";
}

export function transportLabel(transport) {
  return TRANSPORT_LABEL[transport] ?? TRANSPORT_LABEL.unknown;
}

export function hasLight(kind) {
  return kind === "ds4" || kind === "dualsense" || kind === "dualsense_edge";
}

// Only the DualSense family has the row of player-number LEDs, which stay on
// even when the light bar is switched off.
export function hasPlayerLeds(kind) {
  return kind === "dualsense" || kind === "dualsense_edge";
}

// Prefer what the engine said; fall back to the family for older snapshots.
export function controllerHasLight(c) {
  if (!c) return false;
  if (typeof c.has_light === "boolean") return c.has_light;
  return hasLight(c.kind);
}

export function batteryLabel(battery, charging) {
  if (battery === null || battery === undefined || Number.isNaN(battery)) {
    return "Battery unknown";
  }
  const pct = Math.max(0, Math.min(100, Math.round(battery)));
  if (charging === true) return `Battery ${pct}%, charging`;
  return `Battery ${pct}%`;
}

// The engine may send a coarse text label (Xbox pads only report
// low/medium/full). That wins over a percentage we do not actually have.
export function batteryText(controller) {
  if (!controller) return "Battery unknown";
  if (typeof controller.battery_label === "string" && controller.battery_label) return controller.battery_label;
  return batteryLabel(controller.battery, controller.charging);
}

export function slotTitle(index) {
  return `Player ${index + 1}`;
}

export function rgbToHex(rgb) {
  if (!Array.isArray(rgb) || rgb.length !== 3) return null;
  return "#" + rgb.map((v) => Math.max(0, Math.min(255, v | 0)).toString(16).padStart(2, "0")).join("");
}

export function hexToRgb(hex) {
  if (typeof hex !== "string") return null;
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export function driverLabel(status) {
  if (status === true) return "Installed";
  if (status === false) return "Not installed";
  return "Checking…";
}

// Status of an installer shipped inside the app's own install folder.
export function bundleLabel(status) {
  switch (status) {
    case "verified": return "bundled installer verified";
    case "missing": return "bundled installer not found";
    case "corrupt": return "bundled installer failed verification";
    default: return "bundled installer not checked";
  }
}

// Bluetooth scan results: only show things that look like controllers unless
// the user asks for everything.
const CONTROLLER_NAME = /dualshock|dualsense|wireless controller|xbox/i;

export function isControllerName(name) {
  return CONTROLLER_NAME.test(name ?? "");
}

export function filterBtDevices(devices, showAll) {
  const list = Array.isArray(devices) ? devices : [];
  return showAll ? list : list.filter((d) => isControllerName(d.name));
}

export function pairingSteps(kind) {
  switch (kind) {
    case "ds4":
      return [
        "Turn the controller off (hold PS for 10 seconds if it is on).",
        "Hold SHARE and PS together for about 3 seconds.",
        "The light bar flashes quickly. It is now discoverable.",
        "Press Scan below, then Pair next to “Wireless Controller”.",
      ];
    case "dualsense":
      return [
        "Turn the controller off (hold PS for 10 seconds if it is on).",
        "Hold CREATE and PS together for about 3 seconds.",
        "The light bar flashes quickly. It is now discoverable.",
        "Press Scan below, then Pair next to “DualSense Wireless Controller”.",
      ];
    case "xbox":
      return [
        "Press the Xbox button to turn the controller on.",
        "Hold the small Pair button on the top edge until the Xbox button flashes quickly.",
        "Press Scan below, then Pair next to “Xbox Wireless Controller”.",
        "Original Xbox One (2013) and Xbox 360 pads have no Bluetooth; use USB or the Xbox wireless adapter.",
      ];
    default:
      return [];
  }
}

// A snapshot with nothing known yet. Drivers are null (unknown), not false.
export function emptySnapshot() {
  return {
    slots: Array.from({ length: SLOT_COUNT }, (_, index) => ({ index, seat: "empty" })),
    unseated: [],
    drivers: { vigem: null, hidhide: null, hidhide_active: null, bundled: [], installing: null },
    bluetooth: { scanning: false, devices: [], message: "" },
    settings: { hide_from_games: null, hold_seat_seconds: null, start_minimized: null },
    log: [],
  };
}

// Merge a partial snapshot from the engine over the empty one so the renderer
// never has to null-check top-level keys.
export function normalizeSnapshot(raw) {
  const base = emptySnapshot();
  if (!raw || typeof raw !== "object") return base;
  const slots = Array.isArray(raw.slots) ? raw.slots : [];
  return {
    ...base,
    ...raw,
    slots: base.slots.map((s, i) => ({ ...s, ...(slots[i] ?? {}) })),
    unseated: Array.isArray(raw.unseated) ? raw.unseated : [],
    drivers: { ...base.drivers, ...(raw.drivers ?? {}) },
    bluetooth: { ...base.bluetooth, ...(raw.bluetooth ?? {}) },
    settings: { ...base.settings, ...(raw.settings ?? {}) },
    log: Array.isArray(raw.log) ? raw.log : [],
  };
}

export function lastLogLine(snapshot) {
  const log = snapshot?.log ?? [];
  return log.length ? log[log.length - 1] : "";
}
