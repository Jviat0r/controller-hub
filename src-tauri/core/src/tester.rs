//! The controller tester: pure rules behind the live test view.
//!
//! It answers three questions the user actually has:
//!
//! 1. Is the controller sending anything at all? (the hardware column)
//! 2. Is a game going to receive it? (the game column, read back from XInput)
//! 3. If those two disagree, whose fault is it? (the agreement verdict)
//!
//! Plus stick drift measured against the deadzones Microsoft documents, so a
//! number turns into a decision instead of a worry.

use serde::Serialize;

use crate::{
    ids::Kind,
    input::{btn, Input, DPAD_NONE},
    xmap::{to_xpad, xbtn, XPad},
};

/// Deadzones from the XInput documentation. Games that use the recommended
/// values ignore anything inside these, so drift below them is harmless.
pub const LEFT_DEADZONE: u16 = 7849;
pub const RIGHT_DEADZONE: u16 = 8689;
pub const TRIGGER_THRESHOLD: u8 = 30;

/// How many consecutive disagreeing samples before we call it a real
/// mismatch. At roughly 30 samples a second this is about half a second,
/// long enough to ride out the one-frame skew between our write and our read.
const MISMATCH_SAMPLES: u32 = 15;

/// Buttons as the controller itself reports them, in display order.
pub fn hardware_buttons(kind: Kind) -> Vec<(u32, &'static str)> {
    if kind == Kind::Xbox {
        return Vec::new();
    }
    let sony = matches!(kind, Kind::Dualsense | Kind::DualsenseEdge);
    let mut list = vec![
        (btn::CROSS, "Cross"),
        (btn::CIRCLE, "Circle"),
        (btn::SQUARE, "Square"),
        (btn::TRIANGLE, "Triangle"),
        (btn::L1, "L1"),
        (btn::R1, "R1"),
        (btn::L2, "L2 click"),
        (btn::R2, "R2 click"),
        (btn::L3, "L3"),
        (btn::R3, "R3"),
        (btn::SHARE, if sony { "Create" } else { "Share" }),
        (btn::OPTIONS, "Options"),
        (btn::PS, "PS"),
        (btn::TOUCHPAD, "Touchpad"),
    ];
    if sony {
        list.push((btn::MUTE, "Mute"));
    }
    list
}

/// Buttons as a game receives them.
pub fn game_buttons() -> Vec<(u16, &'static str)> {
    vec![
        (xbtn::A, "A"),
        (xbtn::B, "B"),
        (xbtn::X, "X"),
        (xbtn::Y, "Y"),
        (xbtn::LB, "LB"),
        (xbtn::RB, "RB"),
        (xbtn::LTHUMB, "L3"),
        (xbtn::RTHUMB, "R3"),
        (xbtn::BACK, "Back"),
        (xbtn::START, "Start"),
        (xbtn::GUIDE, "Guide"),
        (xbtn::UP, "D-pad up"),
        (xbtn::DOWN, "D-pad down"),
        (xbtn::LEFT, "D-pad left"),
        (xbtn::RIGHT, "D-pad right"),
    ]
}

/// Button names in the order the boolean arrays in every frame use.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Layout {
    pub kind: Kind,
    pub hardware: Vec<&'static str>,
    pub game: Vec<&'static str>,
    /// Xbox pads have no separate hardware view: Windows drives them, so the
    /// only thing to show is what a game sees.
    pub hardware_available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct HardwareView {
    pub lx: u8,
    pub ly: u8,
    pub rx: u8,
    pub ry: u8,
    pub l2: u8,
    pub r2: u8,
    /// 0..7 clockwise from up, 8 = released.
    pub dpad: u8,
    pub buttons: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct GameView {
    pub lx: i16,
    pub ly: i16,
    pub rx: i16,
    pub ry: i16,
    pub lt: u8,
    pub rt: u8,
    pub buttons: Vec<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct StickDrift {
    /// Distance from centre right now, in XInput units (0 to 32767).
    pub now: u16,
    /// Worst distance seen since the test was reset.
    pub max: u16,
    pub deadzone: u16,
    /// "centred" | "inside" | "outside" | "unknown"
    pub verdict: &'static str,
    /// The same judgement applied to the worst reading, which is what
    /// actually tells you whether a stick drifts.
    pub max_verdict: &'static str,
}

impl StickDrift {
    fn unknown(deadzone: u16) -> Self {
        StickDrift { now: 0, max: 0, deadzone, verdict: "unknown", max_verdict: "unknown" }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Coverage {
    pub hardware_seen: Vec<bool>,
    pub game_seen: Vec<bool>,
    pub hardware_all: bool,
    pub game_all: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProbeFrame {
    pub id: String,
    pub slot: Option<usize>,
    pub hardware: Option<HardwareView>,
    pub game: Option<GameView>,
    pub left: StickDrift,
    pub right: StickDrift,
    /// Where the drift numbers came from: "game" (read back from XInput) or
    /// "controller" (computed, because the pad has no seat yet).
    pub drift_source: &'static str,
    pub coverage: Coverage,
    /// "match" | "mismatch" | "single" | "waiting"
    pub agreement: &'static str,
    pub note: Option<String>,
    pub samples: u64,
}

pub struct Tester {
    kind: Kind,
    hardware: Vec<(u32, &'static str)>,
    game: Vec<(u16, &'static str)>,
    hardware_seen: u32,
    game_seen: u16,
    left_max: u16,
    right_max: u16,
    mismatch_streak: u32,
    last_note: Option<String>,
    samples: u64,
}

impl Tester {
    pub fn new(kind: Kind) -> Tester {
        Tester {
            kind,
            hardware: hardware_buttons(kind),
            game: game_buttons(),
            hardware_seen: 0,
            game_seen: 0,
            left_max: 0,
            right_max: 0,
            mismatch_streak: 0,
            last_note: None,
            samples: 0,
        }
    }

    pub fn layout(&self) -> Layout {
        Layout {
            kind: self.kind,
            hardware: self.hardware.iter().map(|(_, n)| *n).collect(),
            game: self.game.iter().map(|(_, n)| *n).collect(),
            hardware_available: !self.hardware.is_empty(),
        }
    }

    /// Clears everything the test has accumulated. The live values keep
    /// flowing; only the "worst seen" and "buttons seen" memory resets.
    pub fn reset(&mut self) {
        self.hardware_seen = 0;
        self.game_seen = 0;
        self.left_max = 0;
        self.right_max = 0;
        self.mismatch_streak = 0;
        self.last_note = None;
        self.samples = 0;
    }

    pub fn feed(&mut self, id: &str, slot: Option<usize>, raw: Option<Input>, game: Option<XPad>) -> ProbeFrame {
        self.samples = self.samples.saturating_add(1);

        if let Some(r) = &raw {
            self.hardware_seen |= r.buttons;
        }
        if let Some(g) = &game {
            self.game_seen |= g.buttons;
        }

        // Drift is measured on what a game receives when we have it, because
        // that is the number that decides whether a game reacts.
        let (drift_from, drift_source) = match (&game, &raw) {
            (Some(g), _) => (Some(*g), "game"),
            (None, Some(r)) => (Some(to_xpad(r)), "controller"),
            (None, None) => (None, "unknown"),
        };
        let (left, right) = match drift_from {
            Some(g) => {
                let l = magnitude(g.lx, g.ly);
                let r = magnitude(g.rx, g.ry);
                self.left_max = self.left_max.max(l);
                self.right_max = self.right_max.max(r);
                (
                    StickDrift {
                        now: l,
                        max: self.left_max,
                        deadzone: LEFT_DEADZONE,
                        verdict: verdict(l, LEFT_DEADZONE),
                        max_verdict: verdict(self.left_max, LEFT_DEADZONE),
                    },
                    StickDrift {
                        now: r,
                        max: self.right_max,
                        deadzone: RIGHT_DEADZONE,
                        verdict: verdict(r, RIGHT_DEADZONE),
                        max_verdict: verdict(self.right_max, RIGHT_DEADZONE),
                    },
                )
            }
            None => (StickDrift::unknown(LEFT_DEADZONE), StickDrift::unknown(RIGHT_DEADZONE)),
        };

        let (agreement, note) = self.judge(&raw, &game);

        ProbeFrame {
            id: id.to_string(),
            slot,
            hardware: raw.as_ref().map(|r| self.hardware_view(r)),
            game: game.as_ref().map(|g| self.game_view(g)),
            left,
            right,
            drift_source,
            coverage: self.coverage(),
            agreement,
            note,
            samples: self.samples,
        }
    }

    /// Does what the controller sent match what a game receives?
    fn judge(&mut self, raw: &Option<Input>, game: &Option<XPad>) -> (&'static str, Option<String>) {
        let (Some(r), Some(g)) = (raw, game) else {
            // An Xbox pad has one source, so there is nothing to compare.
            if self.kind == Kind::Xbox {
                return ("single", None);
            }
            return ("waiting", None);
        };
        let expected = to_xpad(r);
        let mut differs = Vec::new();
        if expected.buttons != g.buttons {
            differs.push("buttons");
        }
        if expected.lx != g.lx || expected.ly != g.ly {
            differs.push("left stick");
        }
        if expected.rx != g.rx || expected.ry != g.ry {
            differs.push("right stick");
        }
        if expected.lt != g.lt || expected.rt != g.rt {
            differs.push("triggers");
        }
        if differs.is_empty() {
            self.mismatch_streak = 0;
            self.last_note = None;
            return ("match", None);
        }
        self.mismatch_streak = self.mismatch_streak.saturating_add(1);
        if self.mismatch_streak < MISMATCH_SAMPLES {
            // Brief disagreement is just the gap between our write and our
            // read. Keep reporting the last settled verdict.
            return ("match", self.last_note.clone());
        }
        let note = format!(
            "The {} reaching games did not match the controller for {} samples in a row. That points at this app or the virtual pad driver, not the controller.",
            differs.join(", "),
            self.mismatch_streak
        );
        self.last_note = Some(note.clone());
        ("mismatch", Some(note))
    }

    fn hardware_view(&self, r: &Input) -> HardwareView {
        HardwareView {
            lx: r.lx,
            ly: r.ly,
            rx: r.rx,
            ry: r.ry,
            l2: r.l2,
            r2: r.r2,
            dpad: if r.dpad > 7 { DPAD_NONE } else { r.dpad },
            buttons: self.hardware.iter().map(|(m, _)| r.buttons & m != 0).collect(),
        }
    }

    fn game_view(&self, g: &XPad) -> GameView {
        GameView {
            lx: g.lx,
            ly: g.ly,
            rx: g.rx,
            ry: g.ry,
            lt: g.lt,
            rt: g.rt,
            buttons: self.game.iter().map(|(m, _)| g.buttons & m != 0).collect(),
        }
    }

    fn coverage(&self) -> Coverage {
        let hardware_seen: Vec<bool> = self.hardware.iter().map(|(m, _)| self.hardware_seen & m != 0).collect();
        let game_seen: Vec<bool> = self.game.iter().map(|(m, _)| self.game_seen & m != 0).collect();
        Coverage {
            hardware_all: !hardware_seen.is_empty() && hardware_seen.iter().all(|x| *x),
            game_all: game_seen.iter().all(|x| *x),
            hardware_seen,
            game_seen,
        }
    }
}

/// Distance of a stick from its centre, in XInput units.
fn magnitude(x: i16, y: i16) -> u16 {
    let d = ((x as f64).powi(2) + (y as f64).powi(2)).sqrt();
    d.min(i16::MAX as f64) as u16
}

fn verdict(distance: u16, deadzone: u16) -> &'static str {
    if distance == 0 {
        "centred"
    } else if distance < deadzone {
        "inside"
    } else {
        "outside"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn centred() -> Input {
        Input { lx: 128, ly: 128, rx: 128, ry: 128, dpad: DPAD_NONE, ..Default::default() }
    }

    #[test]
    fn layout_matches_the_controller_family() {
        let ds = Tester::new(Kind::Dualsense).layout();
        assert!(ds.hardware.contains(&"Create"));
        assert!(ds.hardware.contains(&"Mute"));
        assert!(ds.hardware_available);

        let ds4 = Tester::new(Kind::Ds4).layout();
        assert!(ds4.hardware.contains(&"Share"));
        assert!(!ds4.hardware.contains(&"Mute"));

        let xbox = Tester::new(Kind::Xbox).layout();
        assert!(xbox.hardware.is_empty());
        assert!(!xbox.hardware_available);
        assert!(xbox.game.contains(&"Guide"), "an Xbox pad still has a game-side view");
    }

    #[test]
    fn boolean_arrays_line_up_with_the_layout() {
        let mut t = Tester::new(Kind::Ds4);
        let layout = t.layout();
        let mut raw = centred();
        raw.buttons = btn::TRIANGLE | btn::L1;
        let frame = t.feed("x", Some(0), Some(raw), None);
        let hw = frame.hardware.unwrap();
        assert_eq!(hw.buttons.len(), layout.hardware.len());
        let pressed: Vec<&str> = layout
            .hardware
            .iter()
            .zip(&hw.buttons)
            .filter(|(_, on)| **on)
            .map(|(n, _)| *n)
            .collect();
        assert_eq!(pressed, vec!["Triangle", "L1"]);
    }

    #[test]
    fn a_centred_stick_reports_centred_not_drift() {
        let mut t = Tester::new(Kind::Ds4);
        let frame = t.feed("x", None, Some(centred()), None);
        assert_eq!(frame.left.now, 0);
        assert_eq!(frame.left.verdict, "centred");
        assert_eq!(frame.drift_source, "controller");
    }

    #[test]
    fn small_drift_is_reported_as_harmless_large_drift_is_not() {
        let mut t = Tester::new(Kind::Ds4);
        // A couple of units off centre: well inside the deadzone.
        let small = XPad { lx: 1200, ly: 0, ..Default::default() };
        let frame = t.feed("x", Some(0), None, Some(small));
        assert_eq!(frame.left.now, 1200);
        assert_eq!(frame.left.verdict, "inside");
        assert!(frame.left.now < frame.left.deadzone);

        // Past the documented deadzone: a game will act on this.
        let big = XPad { lx: 9000, ly: 0, ..Default::default() };
        let frame = t.feed("x", Some(0), None, Some(big));
        assert_eq!(frame.left.verdict, "outside");
        assert_eq!(frame.drift_source, "game");
    }

    #[test]
    fn worst_drift_is_remembered_until_reset() {
        let mut t = Tester::new(Kind::Ds4);
        t.feed("x", Some(0), None, Some(XPad { lx: 5000, ..Default::default() }));
        t.feed("x", Some(0), None, Some(XPad { lx: 0, ..Default::default() }));
        let frame = t.feed("x", Some(0), None, Some(XPad { lx: 0, ..Default::default() }));
        assert_eq!(frame.left.now, 0);
        assert_eq!(frame.left.max, 5000, "the worst reading is what proves drift");
        assert_eq!(frame.left.verdict, "centred", "right now it is still");
        assert_eq!(frame.left.max_verdict, "inside", "but it has wandered");
        t.reset();
        let frame = t.feed("x", Some(0), None, Some(XPad { lx: 0, ..Default::default() }));
        assert_eq!(frame.left.max, 0);
    }

    #[test]
    fn diagonal_drift_uses_real_distance() {
        let mut t = Tester::new(Kind::Ds4);
        let frame = t.feed("x", Some(0), None, Some(XPad { lx: 3000, ly: 4000, ..Default::default() }));
        assert_eq!(frame.left.now, 5000);
    }

    #[test]
    fn button_coverage_accumulates_and_completes() {
        let mut t = Tester::new(Kind::Ds4);
        let layout = t.layout();
        let all: u32 = hardware_buttons(Kind::Ds4).iter().map(|(m, _)| m).sum();
        let mut raw = centred();

        raw.buttons = btn::CROSS;
        let frame = t.feed("x", None, Some(raw), None);
        assert!(!frame.coverage.hardware_all);
        assert_eq!(frame.coverage.hardware_seen.iter().filter(|x| **x).count(), 1);

        // Releasing it does not forget that it was seen.
        raw.buttons = 0;
        let frame = t.feed("x", None, Some(raw), None);
        assert_eq!(frame.coverage.hardware_seen[0], true);
        assert_eq!(frame.hardware.unwrap().buttons[0], false);

        raw.buttons = all;
        let frame = t.feed("x", None, Some(raw), None);
        assert!(frame.coverage.hardware_all);
        assert_eq!(frame.coverage.hardware_seen.len(), layout.hardware.len());

        t.reset();
        raw.buttons = 0;
        let frame = t.feed("x", None, Some(raw), None);
        assert!(!frame.coverage.hardware_all);
    }

    #[test]
    fn agreement_holds_when_the_game_sees_what_the_pad_sent() {
        let mut t = Tester::new(Kind::Ds4);
        let mut raw = centred();
        raw.buttons = btn::CROSS;
        let expected = to_xpad(&raw);
        for _ in 0..50 {
            let frame = t.feed("x", Some(0), Some(raw), Some(expected));
            assert_eq!(frame.agreement, "match");
            assert_eq!(frame.note, None);
        }
    }

    #[test]
    fn a_brief_disagreement_is_not_reported_as_a_fault() {
        let mut t = Tester::new(Kind::Ds4);
        let raw = centred();
        let stale = XPad { buttons: xbtn::A, ..to_xpad(&raw) };
        // One frame of skew between our write and our read is normal.
        for _ in 0..(MISMATCH_SAMPLES - 1) {
            assert_eq!(t.feed("x", Some(0), Some(raw), Some(stale)).agreement, "match");
        }
        // Sustained disagreement is a real fault, and it points away from the
        // controller.
        let frame = t.feed("x", Some(0), Some(raw), Some(stale));
        assert_eq!(frame.agreement, "mismatch");
        let note = frame.note.unwrap();
        assert!(note.contains("buttons"));
        assert!(note.contains("not the controller"));
    }

    #[test]
    fn recovering_from_a_mismatch_clears_it() {
        let mut t = Tester::new(Kind::Ds4);
        let raw = centred();
        let wrong = XPad { lx: 9000, ..to_xpad(&raw) };
        for _ in 0..MISMATCH_SAMPLES {
            t.feed("x", Some(0), Some(raw), Some(wrong));
        }
        assert_eq!(t.feed("x", Some(0), Some(raw), Some(wrong)).agreement, "mismatch");
        let frame = t.feed("x", Some(0), Some(raw), Some(to_xpad(&raw)));
        assert_eq!(frame.agreement, "match");
        assert_eq!(frame.note, None);
    }

    #[test]
    fn nothing_from_the_controller_is_waiting_not_a_fault() {
        let mut t = Tester::new(Kind::Ds4);
        let frame = t.feed("x", Some(0), None, Some(XPad::default()));
        assert_eq!(frame.agreement, "waiting");
        assert!(frame.hardware.is_none());
        assert_eq!(frame.note, None);
    }

    #[test]
    fn an_xbox_pad_has_one_source_and_says_so() {
        let mut t = Tester::new(Kind::Xbox);
        let frame = t.feed("xinput:0", Some(0), None, Some(XPad { buttons: xbtn::A, ..Default::default() }));
        assert_eq!(frame.agreement, "single");
        assert!(frame.hardware.is_none());
        assert_eq!(frame.game.unwrap().buttons[0], true);
        assert!(frame.coverage.hardware_seen.is_empty());
        assert!(!frame.coverage.hardware_all, "an empty list must not read as complete");
    }

    #[test]
    fn an_unreadable_pad_reports_unknown_rather_than_zero() {
        let mut t = Tester::new(Kind::Ds4);
        let frame = t.feed("x", None, None, None);
        assert_eq!(frame.left.verdict, "unknown");
        assert_eq!(frame.right.verdict, "unknown");
        assert_eq!(frame.drift_source, "unknown");
        assert!(frame.hardware.is_none() && frame.game.is_none());
    }

    #[test]
    fn triggers_and_dpad_survive_the_round_trip() {
        let mut t = Tester::new(Kind::Dualsense);
        let raw = Input { l2: 200, r2: 7, dpad: 4, ..centred() };
        let frame = t.feed("x", Some(1), Some(raw), Some(to_xpad(&raw)));
        let hw = frame.hardware.unwrap();
        assert_eq!((hw.l2, hw.r2, hw.dpad), (200, 7, 4));
        let g = frame.game.unwrap();
        assert_eq!((g.lt, g.rt), (200, 7));
        let layout = t.layout();
        let down = layout.game.iter().position(|n| *n == "D-pad down").unwrap();
        assert!(g.buttons[down]);
        assert_eq!(frame.slot, Some(1));
    }
}
