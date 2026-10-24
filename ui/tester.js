// Pure formatting for the live test view. No DOM, no rules: every verdict
// ("inside", "outside", "mismatch") is decided by the engine and only
// described here.

export const STICK_MAX = 32767;

/// A signed XInput axis (-32768..32767) as a 0..100 position.
export function gamePercent(v) {
  const n = Math.max(-32768, Math.min(32767, Number(v) || 0));
  return ((n + 32768) / 65535) * 100;
}

/// An unsigned HID axis (0..255) as a 0..100 position.
export function hwPercent(v) {
  const n = Math.max(0, Math.min(255, Number(v) || 0));
  return (n / 255) * 100;
}

// Screens measure downwards. XInput reports Y upwards, HID reports it
// downwards, so only the game view is flipped.
export function gameDot(x, y) {
  return { left: gamePercent(x), top: 100 - gamePercent(y) };
}

export function hwDot(x, y) {
  return { left: hwPercent(x), top: hwPercent(y) };
}

export function percentOfRange(v) {
  return ((Math.abs(Number(v) || 0) / STICK_MAX) * 100).toFixed(1);
}

export function driftText(d) {
  if (!d || d.verdict === "unknown") return "Not reading this stick yet.";
  const now = `${percentOfRange(d.now)}% off centre now`;
  const worst = `worst ${percentOfRange(d.max)}%`;
  if (d.max_verdict === "outside") {
    return `${now}, ${worst}. That is past the deadzone, so a game will read it as movement. This looks like stick drift.`;
  }
  if (d.max_verdict === "inside") {
    return `${now}, ${worst}. Inside the deadzone games ignore, so it is harmless.`;
  }
  return `${now}, ${worst}. Dead centre.`;
}

/// Short label for the drift row, never colour alone.
export function driftTag(d) {
  if (!d || d.max_verdict === "unknown") return "no reading";
  if (d.max_verdict === "outside") return "drifting";
  if (d.max_verdict === "inside") return "ok";
  return "ok";
}

export function agreementText(frame) {
  if (!frame) return "";
  switch (frame.agreement) {
    case "match":
      return "The controller and the game view agree, so input is reaching games correctly.";
    case "mismatch":
      return frame.note || "What games receive does not match what the controller sent.";
    case "single":
      return "Windows drives this controller directly, so there is only one view to check.";
    default:
      return "Waiting for the controller to send something. Move a stick or press a button.";
  }
}

/// Which side a problem is on, in the user's words.
export function faultText(frame) {
  if (!frame) return "";
  if (frame.agreement === "mismatch") return "Software side";
  if (frame.hardware === null || frame.hardware === undefined) {
    if (frame.agreement === "single") return "";
    return "Nothing from the hardware yet";
  }
  return "Hardware side reading fine";
}

export function coverageSummary(seen) {
  const list = Array.isArray(seen) ? seen : [];
  if (!list.length) return "";
  const n = list.filter(Boolean).length;
  return `${n} of ${list.length} pressed`;
}

export function notPressed(names, seen) {
  const list = Array.isArray(seen) ? seen : [];
  return (names || []).filter((_, i) => !list[i]);
}

/// The controller this test is about, or null once it disconnects.
export function findController(snapshot, id) {
  if (!snapshot || !id) return null;
  for (const slot of snapshot.slots || []) {
    if (slot.controller && slot.controller.id === id) return slot.controller;
  }
  for (const c of snapshot.unseated || []) {
    if (c.id === id) return c;
  }
  return null;
}

const DPAD = ["up", "up and right", "right", "down and right", "down", "down and left", "left", "up and left"];

export function dpadName(n) {
  return DPAD[n] ?? "released";
}

export function slotText(slot) {
  if (slot === null || slot === undefined) return "No player seat yet, so games cannot see it.";
  return `Windows shows this as player ${slot + 1}.`;
}
