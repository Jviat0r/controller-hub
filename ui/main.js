// Dumb UI: renders the engine snapshot and forwards user intent. It never
// decides what a controller may do; the engine sends `moves`, `note`, and
// every verdict the test view shows.

import {
  batteryText, bundleLabel, controllerHasLight, driverLabel, esc, filterBtDevices, hasPlayerLeds,
  hexToRgb, kindLabel, lastLogLine, normalizeSnapshot, pairingSteps, rgbToHex, slotTitle, transportLabel,
} from "./state.js";
import { findController } from "./tester.js";
import { buildTester, clearTester, testerGone, updateTester } from "./testview.js";
import { mockBridge } from "./mock.js";

function tauriBridge() {
  const { invoke } = window.__TAURI__.core;
  const { listen } = window.__TAURI__.event;
  return {
    name: "engine",
    onSnapshot(cb) { listen("snapshot", (e) => cb(e.payload)); },
    onProbe(cb) { listen("probe", (e) => cb(e.payload)); },
    invoke(cmd, args) { return invoke(cmd, args ?? {}); },
  };
}

const bridge = window.__TAURI__ ? tauriBridge() : mockBridge();
let snap = normalizeSnapshot(null);
const ui = { btKind: "dualsense", showAllBt: false, slotsDirty: false, error: "", testId: null };

const el = (id) => document.getElementById(id);

async function call(cmd, args) {
  try {
    ui.error = "";
    return await bridge.invoke(cmd, args);
  } catch (e) {
    ui.error = `Error: ${e?.message ?? e}`;
    renderLog();
    return null;
  }
}

// ---------- rendering ----------

function render() {
  renderHeader();
  renderSlots();
  renderUnseated();
  renderBluetooth();
  renderSettings();
  renderTestPresence();
  renderLog();
}

function renderHeader() {
  const pill = el("driver-pill");
  pill.textContent = `Virtual pad driver: ${driverLabel(snap.drivers.vigem)}`;
  pill.dataset.status = String(snap.drivers.vigem);
  el("demo-banner").hidden = bridge.name !== "demo";
}

function lightControls(c) {
  if (!controllerHasLight(c)) return `<p class="muted">No colour light on this controller.</p>`;
  const hex = rgbToHex(c.color);
  const off = c.light_off === true;
  const note = hasPlayerLeds(c.kind) ? " The player number lights stay on." : "";
  return `<div class="light">
      <label>Light colour <input type="color" data-action="color" data-id="${esc(c.id)}" value="${hex ?? "#000000"}" ${off ? "disabled" : ""}></label>
      <code>${off ? "off" : hex ?? "unknown"}</code>
    </div>
    <label class="setting"><input type="checkbox" data-action="light-off" data-id="${esc(c.id)}" ${off ? "checked" : ""}> Turn the light bar off.${note}</label>`;
}

function controllerCard(slot, title) {
  const c = slot.controller;
  const moves = (c.moves ?? []).map((m) => `<option value="${m}">${slotTitle(m)}</option>`).join("");
  const move = moves
    ? `<label>Move to <select data-action="move" data-id="${esc(c.id)}"><option value="">Choose…</option>${moves}</select></label>`
    : "";
  return `<article class="card card--controller" data-kind="${esc(c.kind)}">
    <header><h2>${title}</h2><span class="tag">${transportLabel(c.transport)}</span></header>
    <input class="name" data-action="rename" data-id="${esc(c.id)}" value="${esc(c.name)}" aria-label="Controller name" maxlength="40">
    <p class="meta">${kindLabel(c.kind)} · ${batteryText(c)}</p>
    ${lightControls(c)}
    <div class="actions">
      <button type="button" data-action="identify" data-id="${esc(c.id)}">Identify</button>
      <button type="button" data-action="test" data-id="${esc(c.id)}">Test</button>
      ${move}
    </div>
    ${c.note ? `<p class="muted">${esc(c.note)}</p>` : ""}
    ${c.hidden ? `<p class="muted">Hidden from games; only the virtual pad is visible to them.</p>` : ""}
  </article>`;
}

function cardHtml(slot) {
  const title = slotTitle(slot.index);
  switch (slot.seat) {
    case "controller":
      return controllerCard(slot, title);
    case "held":
      return `<article class="card card--held">
        <h2>${title}</h2>
        <p class="big">Seat held</p>
        <p>Kept for <b>${esc(slot.held_for || "a controller")}</b> while it reconnects.</p>
        <p>${slot.held_seconds_left == null ? "" : `Releases in ${slot.held_seconds_left} s.`}</p>
        <div class="actions"><button type="button" data-action="release" data-slot="${slot.index}">Release now</button></div>
      </article>`;
    case "placeholder":
      return `<article class="card card--placeholder">
        <h2>${title}</h2>
        <p class="big">Reserved seat</p>
        <p>No controller here. Kept so the players after it keep their numbers.</p>
        <div class="actions"><button type="button" data-action="release" data-slot="${slot.index}">Release</button></div>
      </article>`;
    default:
      return `<article class="card card--empty">
        <h2>${title}</h2>
        <p class="big">No controller</p>
        <p>Plug one in over USB, or pair over Bluetooth.</p>
      </article>`;
  }
}

function renderSlots() {
  const host = el("slots");
  if (host.contains(document.activeElement) && document.activeElement !== host) {
    ui.slotsDirty = true; // don't yank an input out from under the user
    return;
  }
  ui.slotsDirty = false;
  host.innerHTML = snap.slots.map(cardHtml).join("");
}

function renderUnseated() {
  const host = el("unseated");
  const list = snap.unseated;
  host.hidden = list.length === 0;
  if (!list.length) return;
  host.innerHTML = `<h2>Connected but not playable yet</h2><ul>${list
    .map((c) => `<li><b>${esc(c.name)}</b> (${kindLabel(c.kind)}, ${transportLabel(c.transport)})${c.note ? ` — ${esc(c.note)}` : ""}
      <button type="button" data-action="test" data-id="${esc(c.id)}">Test</button></li>`)
    .join("")}</ul>`;
}

function renderBluetooth() {
  el("bt-steps").innerHTML = pairingSteps(ui.btKind).map((s) => `<li>${esc(s)}</li>`).join("");
  const scan = el("bt-scan");
  scan.disabled = snap.bluetooth.scanning;
  scan.textContent = snap.bluetooth.scanning ? "Scanning…" : "Scan for controllers";
  el("bt-message").textContent = snap.bluetooth.message ?? "";
  const devices = filterBtDevices(snap.bluetooth.devices, ui.showAllBt);
  el("bt-list").innerHTML = devices.length
    ? devices.map((d) => `<li><span>${esc(d.name || "Unnamed device")}${d.paired === true ? " · paired" : d.paired === false ? "" : " · pairing state unknown"}</span>
        ${d.paired === true
          ? `<button type="button" data-action="bt-unpair" data-id="${esc(d.id)}">Unpair</button>`
          : `<button type="button" class="primary" data-action="bt-pair" data-id="${esc(d.id)}">Pair</button>`}</li>`).join("")
    : (snap.bluetooth.scanning ? "" : `<li class="muted">Nothing found yet. Put the controller in pairing mode, then Scan.</li>`);
}

function renderSettings() {
  const d = snap.drivers;
  const bundled = Object.fromEntries((d.bundled ?? []).map((b) => [b.key, b]));
  const row = (label, status, key, extra) => {
    const b = bundled[key];
    const bundleLine = b
      ? `<br><span class="muted">${esc(b.name)} ${esc(b.version)}: ${bundleLabel(b.status)}${b.detail ? ` (${esc(b.detail)})` : ""}</span>`
      : "";
    let button = "";
    if (status === false) {
      if (b && b.status === "verified") {
        button = d.installing
          ? `<button type="button" disabled>${d.installing === key ? "Installing…" : "Install"}</button>`
          : `<button type="button" class="primary" data-action="install-driver" data-key="${key}">Install ${esc(b.name)} ${esc(b.version)}</button>`;
      } else {
        button = `<button type="button" data-action="open-url" data-key="${key}">Get ${label}</button>`;
      }
    }
    return `<li><span>${label}: <b>${driverLabel(status)}</b>${extra ? ` <span class="muted">${extra}</span>` : ""}${bundleLine}</span>${button}</li>`;
  };
  el("driver-list").innerHTML =
    row("ViGEmBus", d.vigem, "vigem", null) +
    row("HidHide", d.hidhide, "hidhide", d.hidhide === true ? (d.hidhide_active === true ? "(active)" : d.hidhide_active === false ? "(installed, not hiding)" : "") : null);
  for (const input of document.querySelectorAll('[data-action="setting"]')) {
    const v = snap.settings[input.dataset.key];
    if (input.type === "checkbox") {
      input.indeterminate = v === null || v === undefined;
      input.checked = v === true;
    } else if (document.activeElement !== input) {
      input.value = v === null || v === undefined ? "" : v;
    }
    if (input.dataset.key === "hide_from_games") input.disabled = d.hidhide !== true;
  }
}

/// The tested controller can vanish mid-test; say so rather than freezing.
function renderTestPresence() {
  if (!ui.testId) return;
  testerGone(findController(snap, ui.testId) === null);
}

function renderLog() {
  el("log").textContent = ui.error || lastLogLine(snap);
}

// ---------- the live test ----------

async function openTest(id) {
  const c = findController(snap, id);
  if (!c) return;
  const layout = await call("start_probe", { id });
  if (!layout) return;
  ui.testId = id;
  buildTester(layout, c.name);
  el("test-dialog").showModal();
}

async function closeTest() {
  if (!ui.testId) return;
  ui.testId = null;
  clearTester();
  await call("stop_probe");
}

// ---------- events ----------

document.addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-action]");
  if (!b) return;
  const id = b.dataset.id;
  switch (b.dataset.action) {
    case "open-bt": el("bt-dialog").showModal(); break;
    case "open-settings": el("settings-dialog").showModal(); break;
    case "close": b.closest("dialog").close(); break;
    case "test": await openTest(id); break;
    case "close-test": el("test-dialog").close(); break;
    case "reset-test": await call("reset_probe"); break;
    case "identify": await call("identify", { id }); break;
    case "release": await call("release_hold", { slot: Number(b.dataset.slot) }); break;
    case "bt-scan": await call("bt_scan"); break;
    case "bt-pair": await call("bt_pair", { id }); break;
    case "bt-unpair": await call("bt_unpair", { id }); break;
    case "open-url": await call("open_url", { key: b.dataset.key }); break;
    case "install-driver": await call("install_driver", { key: b.dataset.key }); break;
    case "recheck": await call("recheck_drivers"); break;
  }
});

// Covers the Close button, the Escape key and any other way out.
el("test-dialog").addEventListener("close", closeTest);

document.addEventListener("change", async (e) => {
  const t = e.target;
  const id = t.dataset.id;
  switch (t.dataset.action) {
    case "move": {
      const slot = Number(t.value);
      if (t.value !== "" && Number.isInteger(slot)) await call("reassign", { id, slot });
      break;
    }
    case "rename": await call("rename", { id, name: t.value.trim() }); break;
    case "color": await call("set_color", { id, rgb: hexToRgb(t.value) }); break;
    case "light-off": await call("set_light_off", { id, off: t.checked }); break;
    case "bt-kind": ui.btKind = t.value; renderBluetooth(); break;
    case "bt-all": ui.showAllBt = t.checked; renderBluetooth(); break;
    case "setting": {
      const value = t.type === "checkbox" ? t.checked : Number(t.value);
      if (t.type !== "checkbox" && !Number.isFinite(value)) break;
      await call("set_setting", { key: t.dataset.key, value });
      break;
    }
  }
});

// Live colour preview while dragging in the picker, throttled.
let colorTimer = null;
let colorPending = null;
document.addEventListener("input", (e) => {
  const t = e.target;
  if (t.dataset.action !== "color") return;
  colorPending = { id: t.dataset.id, rgb: hexToRgb(t.value) };
  if (colorTimer) return;
  colorTimer = setTimeout(() => {
    colorTimer = null;
    if (colorPending) call("set_color", colorPending);
    colorPending = null;
  }, 80);
});

el("slots").addEventListener("focusout", () => {
  if (ui.slotsDirty) setTimeout(renderSlots, 0);
});

// ---------- boot ----------

bridge.onSnapshot((s) => {
  snap = normalizeSnapshot(s);
  render();
});
bridge.onProbe((frame) => {
  if (frame && frame.id === ui.testId) updateTester(frame);
});
render();
call("get_snapshot").then((s) => {
  if (s) {
    snap = normalizeSnapshot(s);
    render();
  }
});
