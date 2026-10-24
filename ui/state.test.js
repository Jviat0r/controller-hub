import { describe, it, expect } from "vitest";
import {
  batteryLabel, batteryText, bundleLabel, controllerHasLight, driverLabel, emptySnapshot, filterBtDevices,
  hasLight, hasPlayerLeds, hexToRgb, kindLabel, lastLogLine, normalizeSnapshot, pairingSteps, rgbToHex,
  slotTitle, transportLabel,
} from "./state.js";

describe("unknown reads as unknown", () => {
  it("battery null/undefined/NaN never renders a number", () => {
    for (const v of [null, undefined, NaN]) {
      expect(batteryLabel(v, null)).toBe("Battery unknown");
      expect(batteryLabel(v, null)).not.toMatch(/\d/);
    }
  });
  it("driver status null is 'Checking…', not 'Not installed'", () => {
    expect(driverLabel(null)).toBe("Checking…");
    expect(driverLabel(undefined)).toBe("Checking…");
    expect(driverLabel(false)).toBe("Not installed");
    expect(driverLabel(true)).toBe("Installed");
  });
  it("empty snapshot has unknown drivers and settings, four empty seats", () => {
    const s = emptySnapshot();
    expect(s.slots).toHaveLength(4);
    expect(s.slots.every((x) => x.seat === "empty")).toBe(true);
    expect(s.drivers.vigem).toBeNull();
    expect(s.drivers.bundled).toEqual([]);
    expect(s.settings.hide_from_games).toBeNull();
  });
  it("bundle status never claims verified unless the engine said so", () => {
    expect(bundleLabel("verified")).toMatch(/verified/);
    expect(bundleLabel("missing")).toMatch(/not found/);
    expect(bundleLabel("corrupt")).toMatch(/failed/);
    expect(bundleLabel(undefined)).toMatch(/not checked/);
    expect(bundleLabel("anything")).not.toMatch(/verified/);
  });
  it("unknown kinds/transports get generic labels", () => {
    expect(kindLabel("mystery")).toBe("Controller");
    expect(transportLabel("serial")).toBe("Unknown link");
    expect(transportLabel("wireless")).toBe("Wireless");
  });
  it("a coarse engine label beats an absent percentage", () => {
    expect(batteryText({ battery: null, charging: null, battery_label: "Battery low" })).toBe("Battery low");
    expect(batteryText({ battery: 50, charging: false, battery_label: null })).toBe("Battery 50%");
    expect(batteryText({ battery: null, charging: null })).toBe("Battery unknown");
    expect(batteryText(null)).toBe("Battery unknown");
  });
});

describe("formatting", () => {
  it("battery clamps and shows charging", () => {
    expect(batteryLabel(250, false)).toBe("Battery 100%");
    expect(batteryLabel(-5, null)).toBe("Battery 0%");
    expect(batteryLabel(42.4, true)).toBe("Battery 42%, charging");
  });
  it("slot titles are 1-based", () => {
    expect(slotTitle(0)).toBe("Player 1");
    expect(slotTitle(3)).toBe("Player 4");
  });
  it("colour round-trips", () => {
    expect(rgbToHex([0, 0, 255])).toBe("#0000ff");
    expect(hexToRgb("#0000FF")).toEqual([0, 0, 255]);
    expect(hexToRgb("ff8800")).toEqual([255, 136, 0]);
    expect(hexToRgb("nope")).toBeNull();
    expect(rgbToHex(null)).toBeNull();
  });
  it("only Sony pads expose a light", () => {
    expect(hasLight("ds4")).toBe(true);
    expect(hasLight("dualsense")).toBe(true);
    expect(hasLight("dualsense_edge")).toBe(true);
    expect(hasLight("xbox")).toBe(false);
  });
  it("only the DualSense family has player-number LEDs", () => {
    expect(hasPlayerLeds("dualsense")).toBe(true);
    expect(hasPlayerLeds("dualsense_edge")).toBe(true);
    expect(hasPlayerLeds("ds4")).toBe(false);
    expect(hasPlayerLeds("xbox")).toBe(false);
  });
  it("the engine's word on a light beats the family guess", () => {
    expect(controllerHasLight({ kind: "ds4", has_light: false })).toBe(false);
    expect(controllerHasLight({ kind: "ds4" })).toBe(true);
    expect(controllerHasLight({ kind: "xbox" })).toBe(false);
    expect(controllerHasLight(null)).toBe(false);
  });
});

describe("bluetooth list", () => {
  const devices = [
    { id: "a", name: "Wireless Controller" },
    { id: "b", name: "JBL Flip 6" },
    { id: "c", name: "DualSense Wireless Controller" },
    { id: "d", name: "Xbox Wireless Controller" },
    { id: "e", name: null },
  ];
  it("filters to controllers by default", () => {
    expect(filterBtDevices(devices, false).map((d) => d.id)).toEqual(["a", "c", "d"]);
  });
  it("shows everything when asked", () => {
    expect(filterBtDevices(devices, true)).toHaveLength(5);
  });
  it("tolerates a missing list", () => {
    expect(filterBtDevices(undefined, false)).toEqual([]);
  });
  it("has pairing steps for every supported family", () => {
    for (const k of ["ds4", "dualsense", "xbox"]) expect(pairingSteps(k).length).toBeGreaterThan(2);
    expect(pairingSteps("other")).toEqual([]);
  });
});

describe("normalizeSnapshot", () => {
  it("fills missing keys without inventing values", () => {
    const s = normalizeSnapshot({ slots: [{ index: 0, seat: "controller", controller: { id: "x" } }] });
    expect(s.slots).toHaveLength(4);
    expect(s.slots[0].controller.id).toBe("x");
    expect(s.slots[1].seat).toBe("empty");
    expect(s.drivers.vigem).toBeNull();
  });
  it("returns the empty snapshot for garbage", () => {
    expect(normalizeSnapshot(null).slots).toHaveLength(4);
    expect(normalizeSnapshot("x").log).toEqual([]);
  });
  it("last log line", () => {
    expect(lastLogLine({ log: [] })).toBe("");
    expect(lastLogLine({ log: ["a", "b"] })).toBe("b");
  });
});
