// Browser-only stand-in for the Rust engine so the UI can be developed with
// `npm run ui` and no Tauri. Everything here is labelled DEMO in the UI; it is
// never used when window.__TAURI__ exists.

const DS4_BUTTONS = ["Cross", "Circle", "Square", "Triangle", "L1", "R1", "L2 click", "R2 click",
  "L3", "R3", "Share", "Options", "PS", "Touchpad"];
const GAME_BUTTONS = ["A", "B", "X", "Y", "LB", "RB", "L3", "R3", "Back", "Start", "Guide",
  "D-pad up", "D-pad down", "D-pad left", "D-pad right"];

export function mockBridge() {
  const listeners = [];
  const probeListeners = [];
  const snapshot = {
    demo: true,
    slots: [
      { index: 0, seat: "controller", controller: {
        id: "demo-ds4", kind: "ds4", name: "Demo DualShock 4", transport: "usb",
        battery: 80, charging: true, color: [0, 0, 255], has_light: true, light_off: false,
        moves: [1, 2, 3], note: null, is_virtual: true, hidden: false } },
      { index: 1, seat: "controller", controller: {
        id: "demo-xbox", kind: "xbox", name: "Demo Xbox controller", transport: "bluetooth",
        battery: null, charging: null, color: null, has_light: false, light_off: false, moves: [],
        note: "Windows assigns Xbox controller seats in connection order. Reconnect it to move it.",
        is_virtual: false, hidden: false } },
      { index: 2, seat: "held", held_for: "Demo DualSense", held_seconds_left: 21 },
      { index: 3, seat: "empty" },
    ],
    unseated: [],
    drivers: {
      vigem: true, hidhide: false, hidhide_active: false, installing: null,
      bundled: [
        { key: "vigem", name: "ViGEmBus", version: "1.22.0", status: "verified", required: true },
        { key: "hidhide", name: "HidHide", version: "1.5.230", status: "verified", required: false },
      ],
    },
    bluetooth: { scanning: false, devices: [], message: "" },
    settings: { hide_from_games: false, hold_seat_seconds: 30, start_minimized: false },
    log: ["DEMO MODE: no engine attached; nothing here is real."],
  };
  const emit = () => listeners.forEach((cb) => cb(structuredClone(snapshot)));

  // A fake controller that wanders, so the test view has something to show.
  let probe = null;
  const bools = (n, on) => Array.from({ length: n }, (_, i) => i === on);
  const orBools = (a, b) => a.map((v, i) => v || b[i]);

  function startProbe(id) {
    stopProbe();
    const xbox = id === "demo-xbox";
    const layout = {
      kind: xbox ? "xbox" : "ds4",
      hardware: xbox ? [] : DS4_BUTTONS,
      game: GAME_BUTTONS,
      hardware_available: !xbox,
    };
    let t = 0;
    let hwSeen = DS4_BUTTONS.map(() => false);
    let gameSeen = GAME_BUTTONS.map(() => false);
    let leftMax = 0;
    probe = { id, timer: setInterval(() => {
      t += 1;
      const step = Math.floor(t / 20);
      const hwOn = step % DS4_BUTTONS.length;
      const gameOn = step % GAME_BUTTONS.length;
      const hwButtons = bools(DS4_BUTTONS.length, hwOn);
      const gameButtons = bools(GAME_BUTTONS.length, gameOn);
      hwSeen = orBools(hwSeen, hwButtons);
      gameSeen = orBools(gameSeen, gameButtons);
      // A left stick sitting a little off centre, a right stick circling.
      const lx = 134, ly = 131;
      const rx = Math.round(128 + 110 * Math.cos(t / 12));
      const ry = Math.round(128 + 110 * Math.sin(t / 12));
      const toAxis = (v) => Math.round(((v - 128) / 127) * 32767);
      const gl = { lx: toAxis(lx), ly: -toAxis(ly), rx: toAxis(rx), ry: -toAxis(ry) };
      leftMax = Math.max(leftMax, Math.hypot(gl.lx, gl.ly));
      const frame = {
        id, slot: xbox ? 1 : 0,
        hardware: xbox ? null : { lx, ly, rx, ry, l2: (t * 4) % 256, r2: 0, dpad: 8, buttons: hwButtons },
        game: { ...gl, lt: (t * 4) % 256, rt: 0, buttons: gameButtons },
        left: { now: Math.round(Math.hypot(gl.lx, gl.ly)), max: Math.round(leftMax), deadzone: 7849,
          verdict: "inside", max_verdict: "inside" },
        right: { now: 28000, max: 28000, deadzone: 8689, verdict: "outside", max_verdict: "outside" },
        drift_source: "game",
        coverage: {
          hardware_seen: xbox ? [] : hwSeen, game_seen: gameSeen,
          hardware_all: !xbox && hwSeen.every(Boolean), game_all: gameSeen.every(Boolean),
        },
        agreement: xbox ? "single" : "match",
        note: null,
        samples: t,
      };
      probeListeners.forEach((cb) => cb(frame));
    }, 50) };
    return layout;
  }

  function stopProbe() {
    if (probe) clearInterval(probe.timer);
    probe = null;
  }

  return {
    name: "demo",
    onSnapshot(cb) { listeners.push(cb); },
    onProbe(cb) { probeListeners.push(cb); },
    async invoke(cmd, args = {}) {
      if (cmd !== "get_snapshot") snapshot.log.push(`DEMO: ${cmd} ${JSON.stringify(args)}`);
      const find = (id) => snapshot.slots.map((s) => s.controller).find((x) => x && x.id === id);
      if (cmd === "start_probe") return startProbe(args.id);
      if (cmd === "stop_probe") { stopProbe(); return null; }
      if (cmd === "reset_probe") { const id = probe?.id; if (id) startProbe(id); return null; }
      if (cmd === "set_color") {
        const c = find(args.id);
        if (c) { c.color = args.rgb; c.light_off = false; }
      }
      if (cmd === "set_light_off") {
        const c = find(args.id);
        if (c) c.light_off = args.off;
      }
      if (cmd === "rename") {
        const c = find(args.id);
        if (c) c.name = args.name;
      }
      if (cmd === "bt_scan") {
        snapshot.bluetooth.scanning = true;
        emit();
        await new Promise((r) => setTimeout(r, 800));
        snapshot.bluetooth = { scanning: false, message: "DEMO scan finished", devices: [
          { id: "bt-1", name: "DualSense Wireless Controller", paired: false },
          { id: "bt-2", name: "JBL Flip 6", paired: true },
        ] };
      }
      if (cmd === "set_setting") snapshot.settings[args.key] = args.value;
      if (cmd === "install_driver") {
        snapshot.drivers.installing = args.key;
        emit();
        await new Promise((r) => setTimeout(r, 1000));
        snapshot.drivers.installing = null;
        snapshot.drivers[args.key] = true;
        snapshot.log.push(`DEMO: ${args.key} installed`);
      }
      emit();
      return cmd === "get_snapshot" ? structuredClone(snapshot) : null;
    },
  };
}
