import { describe, it, expect } from "vitest";
import {
  agreementText, coverageSummary, dpadName, driftTag, driftText, findController, gameDot, gamePercent,
  hwDot, hwPercent, notPressed, percentOfRange, slotText,
} from "./tester.js";

describe("axis positions", () => {
  it("maps a game axis across the full square", () => {
    expect(gamePercent(-32768)).toBe(0);
    expect(gamePercent(32767)).toBe(100);
    expect(gamePercent(0)).toBeCloseTo(50, 1);
  });
  it("maps a hardware axis across the full square", () => {
    expect(hwPercent(0)).toBe(0);
    expect(hwPercent(255)).toBe(100);
    expect(hwPercent(128)).toBeCloseTo(50.2, 1);
  });
  it("flips only the game view, because screens measure downwards", () => {
    // Stick pushed up: XInput says +Y, HID says low Y. Both must draw high.
    expect(gameDot(0, 32767).top).toBe(0);
    expect(hwDot(128, 0).top).toBe(0);
    expect(gameDot(0, -32768).top).toBe(100);
    expect(hwDot(128, 255).top).toBe(100);
  });
  it("clamps junk instead of drawing outside the box", () => {
    expect(gamePercent(999999)).toBe(100);
    expect(hwPercent(-5)).toBe(0);
    expect(gamePercent(undefined)).toBeCloseTo(50, 1);
  });
});

describe("drift wording follows the engine's verdict", () => {
  const d = (now, max, verdict, max_verdict) => ({ now, max, verdict, max_verdict, deadzone: 7849 });

  it("says drift only when the engine said the worst reading was outside", () => {
    expect(driftText(d(9000, 9000, "outside", "outside"))).toMatch(/stick drift/);
    expect(driftTag(d(9000, 9000, "outside", "outside"))).toBe("drifting");
  });
  it("calls a small wander harmless", () => {
    const text = driftText(d(0, 1200, "centred", "inside"));
    expect(text).toMatch(/harmless/);
    expect(text).not.toMatch(/stick drift/);
    expect(driftTag(d(0, 1200, "centred", "inside"))).toBe("ok");
  });
  it("a stick that never moved reads as dead centre", () => {
    expect(driftText(d(0, 0, "centred", "centred"))).toMatch(/Dead centre/);
  });
  it("unknown never shows a number", () => {
    expect(driftText(d(0, 0, "unknown", "unknown"))).toBe("Not reading this stick yet.");
    expect(driftText(null)).toBe("Not reading this stick yet.");
    expect(driftText(undefined)).not.toMatch(/\d/);
    expect(driftTag(null)).toBe("no reading");
  });
  it("still reports drift when the stick happens to be centred at this instant", () => {
    expect(driftText(d(0, 9000, "centred", "outside"))).toMatch(/stick drift/);
  });
  it("percentages are of the full stick range", () => {
    expect(percentOfRange(32767)).toBe("100.0");
    expect(percentOfRange(0)).toBe("0.0");
    expect(percentOfRange(-16383)).toBe("50.0");
  });
});

describe("hardware versus software verdict", () => {
  it("a mismatch is reported as the software side, using the engine's note", () => {
    const frame = { agreement: "mismatch", note: "The buttons reaching games did not match.", hardware: {} };
    expect(agreementText(frame)).toBe("The buttons reaching games did not match.");
  });
  it("agreement is stated plainly", () => {
    expect(agreementText({ agreement: "match", hardware: {} })).toMatch(/agree/);
  });
  it("an Xbox pad says there is only one view", () => {
    const frame = { agreement: "single", hardware: null };
    expect(agreementText(frame)).toMatch(/only one view/);
  });
  it("no input yet asks the user to press something", () => {
    expect(agreementText({ agreement: "waiting", hardware: null })).toMatch(/Move a stick/);
    expect(agreementText(null)).toBe("");
  });
});

describe("button coverage", () => {
  it("counts what has been pressed", () => {
    expect(coverageSummary([true, false, true])).toBe("2 of 3 pressed");
    expect(coverageSummary([])).toBe("");
    expect(coverageSummary(undefined)).toBe("");
  });
  it("names what is still missing so the user knows what to try", () => {
    expect(notPressed(["A", "B", "X"], [true, false, false])).toEqual(["B", "X"]);
    expect(notPressed(["A"], [true])).toEqual([]);
  });
});

describe("finding the controller under test", () => {
  const snap = {
    slots: [{ controller: { id: "a" } }, { seat: "empty" }],
    unseated: [{ id: "b" }],
  };
  it("looks in seats and in the unseated list", () => {
    expect(findController(snap, "a").id).toBe("a");
    expect(findController(snap, "b").id).toBe("b");
  });
  it("returns null once it disconnects", () => {
    expect(findController(snap, "gone")).toBeNull();
    expect(findController(null, "a")).toBeNull();
  });
  it("an unseated pad is described as not visible to games", () => {
    expect(slotText(null)).toMatch(/cannot see it/);
    expect(slotText(0)).toBe("Windows shows this as player 1.");
  });
  it("names every d-pad direction and calls 8 released", () => {
    expect(dpadName(0)).toBe("up");
    expect(dpadName(2)).toBe("right");
    expect(dpadName(6)).toBe("left");
    expect(dpadName(8)).toBe("released");
    expect(dpadName(undefined)).toBe("released");
  });
});
