//! Seat (player slot) manager.
//!
//! XInput hands out player slots itself: a newly plugged pad, real or
//! virtual, takes the lowest free index and keeps it until it leaves. The
//! only lever we have is *when* we plug our own virtual pads. So this module
//! turns intent ("keep Player 2's seat while it reconnects", "make this pad
//! Player 3") into an ordered list of plug/unplug actions, and the platform
//! layer reports back where each plug actually landed.
//!
//! Rules:
//! - A physical Xbox pad owns its seat; we never move it and never plug over it.
//! - When a Sony pad leaves, a silent placeholder pad keeps its seat for
//!   `hold_ms` so a reconnect lands on the same number. The hold is only
//!   possible when that seat is the lowest free one (XInput would otherwise
//!   put the placeholder somewhere else).
//! - Moving a pad to a higher seat may need placeholders in the gaps; those
//!   are kept indefinitely and shown as "reserved" until released.
//! - All of this is pure; time is a `u64` millisecond value passed in.

use std::collections::VecDeque;

use crate::model::SEAT_COUNT;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hold {
    /// Empty when the placeholder is a gap filler rather than a seat hold.
    pub for_id: String,
    pub for_name: String,
    /// None = keep until released.
    pub until_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seat {
    Empty,
    Physical,
    Virtual(String),
    Placeholder(Hold),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Target {
    Pad(String),
    Placeholder(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Unplug(Target),
    /// Plug and expect it to land on this seat. The platform must call
    /// `landed` with the real index afterwards.
    Plug { target: Target, expected: usize, hold: Option<Hold> },
}

#[derive(Debug, Clone)]
pub struct Seats {
    seats: [Seat; SEAT_COUNT],
    hold_ms: u64,
    waiting: VecDeque<(String, String)>,
    pending: Vec<(Target, usize, Option<Hold>)>,
}

impl Seats {
    pub fn new(hold_ms: u64) -> Self {
        Seats {
            seats: [Seat::Empty, Seat::Empty, Seat::Empty, Seat::Empty],
            hold_ms,
            waiting: VecDeque::new(),
            pending: Vec::new(),
        }
    }

    pub fn set_hold_ms(&mut self, ms: u64) {
        self.hold_ms = ms;
    }

    pub fn seats(&self) -> &[Seat; SEAT_COUNT] {
        &self.seats
    }

    pub fn waiting(&self) -> impl Iterator<Item = &str> {
        self.waiting.iter().map(|(id, _)| id.as_str())
    }

    pub fn seat_of(&self, id: &str) -> Option<usize> {
        self.seats.iter().position(|s| matches!(s, Seat::Virtual(x) if x == id))
    }

    pub fn is_pending(&self, id: &str) -> bool {
        self.pending.iter().any(|(t, _, _)| matches!(t, Target::Pad(x) if x == id))
    }

    fn lowest_free(&self) -> Option<usize> {
        self.seats.iter().position(|s| matches!(s, Seat::Empty))
    }

    /// Physical Xbox pads as seen by XInput, with our own virtual pads already
    /// removed by the caller.
    pub fn physical_scan(&mut self, connected: [bool; SEAT_COUNT]) -> Vec<Action> {
        for i in 0..SEAT_COUNT {
            match (&self.seats[i], connected[i]) {
                (Seat::Empty, true) => self.seats[i] = Seat::Physical,
                (Seat::Physical, false) => self.seats[i] = Seat::Empty,
                _ => {}
            }
        }
        self.seat_waiting()
    }

    /// A Sony pad appeared. Returns the actions to seat it (possibly none if
    /// every seat is taken; it then waits).
    pub fn sony_arrived(&mut self, id: &str, name: &str) -> Vec<Action> {
        if self.seat_of(id).is_some() || self.is_pending(id) {
            return Vec::new();
        }
        let mut actions = Vec::new();
        // A placeholder was holding this exact pad's seat: release it first.
        if let Some(i) = self
            .seats
            .iter()
            .position(|s| matches!(s, Seat::Placeholder(h) if h.for_id == id))
        {
            self.seats[i] = Seat::Empty;
            actions.push(Action::Unplug(Target::Placeholder(i)));
        }
        match self.lowest_free() {
            Some(i) => {
                self.pending.push((Target::Pad(id.to_string()), i, None));
                actions.push(Action::Plug { target: Target::Pad(id.to_string()), expected: i, hold: None });
            }
            None => self.waiting.push_back((id.to_string(), name.to_string())),
        }
        actions
    }

    /// The platform plugged something and XInput put it at `index`.
    /// Returns a note when it did not land where expected.
    pub fn landed(&mut self, target: &Target, index: usize) -> Option<String> {
        let pos = self.pending.iter().position(|(t, _, _)| t == target);
        let (expected, hold) = match pos {
            Some(p) => {
                let (_, e, h) = self.pending.remove(p);
                (Some(e), h)
            }
            None => (None, None),
        };
        if index >= SEAT_COUNT {
            return Some(format!("{target:?} landed outside the four seats (index {index})"));
        }
        self.seats[index] = match target {
            Target::Pad(id) => Seat::Virtual(id.clone()),
            Target::Placeholder(_) => Seat::Placeholder(hold.unwrap_or(Hold {
                for_id: String::new(),
                for_name: String::new(),
                until_ms: None,
            })),
        };
        match expected {
            Some(e) if e != index => Some(format!("expected seat {} but Windows gave seat {}", e + 1, index + 1)),
            _ => None,
        }
    }

    /// Something we tried to plug failed; forget the expectation.
    pub fn plug_failed(&mut self, target: &Target) {
        self.pending.retain(|(t, _, _)| t != target);
    }

    /// A Sony pad went away. Its seat is held with a placeholder when possible.
    pub fn sony_left(&mut self, id: &str, name: &str, now_ms: u64) -> Vec<Action> {
        self.waiting.retain(|(w, _)| w != id);
        self.pending.retain(|(t, _, _)| !matches!(t, Target::Pad(x) if x == id));
        let Some(i) = self.seat_of(id) else { return Vec::new() };
        self.seats[i] = Seat::Empty;
        let mut actions = vec![Action::Unplug(Target::Pad(id.to_string()))];
        if self.hold_ms > 0 && self.lowest_free() == Some(i) && self.waiting.is_empty() {
            let hold = Hold { for_id: id.to_string(), for_name: name.to_string(), until_ms: Some(now_ms + self.hold_ms) };
            self.pending.push((Target::Placeholder(i), i, Some(hold.clone())));
            actions.push(Action::Plug { target: Target::Placeholder(i), expected: i, hold: Some(hold) });
        }
        actions.extend(self.seat_waiting());
        actions
    }

    /// Expire timed holds.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Action> {
        let mut actions = Vec::new();
        for i in 0..SEAT_COUNT {
            if let Seat::Placeholder(Hold { until_ms: Some(t), .. }) = &self.seats[i] {
                if *t <= now_ms {
                    self.seats[i] = Seat::Empty;
                    actions.push(Action::Unplug(Target::Placeholder(i)));
                }
            }
        }
        actions.extend(self.seat_waiting());
        actions
    }

    /// User asked to drop a hold or reserved seat.
    pub fn release(&mut self, seat: usize) -> Vec<Action> {
        let mut actions = Vec::new();
        if let Some(Seat::Placeholder(_)) = self.seats.get(seat) {
            self.seats[seat] = Seat::Empty;
            actions.push(Action::Unplug(Target::Placeholder(seat)));
        }
        actions.extend(self.seat_waiting());
        actions
    }

    /// Seats a waiting pad if one is free.
    fn seat_waiting(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        while let Some(i) = self.lowest_free() {
            let Some((id, _)) = self.waiting.pop_front() else { break };
            self.pending.push((Target::Pad(id.clone()), i, None));
            actions.push(Action::Plug { target: Target::Pad(id), expected: i, hold: None });
            // Only one plug per free seat; the next free seat is found after `landed`.
            break;
        }
        actions
    }

    /// Seats a virtual pad may move to right now.
    pub fn allowed_moves(&self, id: &str) -> Vec<usize> {
        let Some(cur) = self.seat_of(id) else { return Vec::new() };
        (0..SEAT_COUNT)
            .filter(|&i| i != cur && !matches!(self.seats[i], Seat::Physical))
            .collect()
    }

    /// Move a virtual pad to `target`, swapping with whatever virtual thing
    /// is there. Seats below `min(current, target)` are untouched, so other
    /// players in lower seats never see a disconnect.
    pub fn reassign(&mut self, id: &str, target: usize) -> Result<Vec<Action>, String> {
        if target >= SEAT_COUNT {
            return Err("there are only four seats".into());
        }
        let cur = self.seat_of(id).ok_or("that controller is not seated right now")?;
        if cur == target {
            return Ok(Vec::new());
        }
        if matches!(self.seats[target], Seat::Physical) {
            return Err(format!(
                "Player {} is an Xbox controller and Windows decides its seat. Reconnect the Xbox controller after this one instead.",
                target + 1
            ));
        }
        let mut desired = self.seats.clone();
        desired.swap(cur, target);
        // Fill gaps below the target so XInput counts up to it.
        for s in 0..target {
            if matches!(desired[s], Seat::Empty) {
                desired[s] = Seat::Placeholder(Hold { for_id: String::new(), for_name: String::new(), until_ms: None });
            }
        }
        let from = cur.min(target);
        let mut actions = Vec::new();
        // Unplug everything of ours from `from` upwards.
        for i in from..SEAT_COUNT {
            match &self.seats[i] {
                Seat::Virtual(x) => actions.push(Action::Unplug(Target::Pad(x.clone()))),
                Seat::Placeholder(_) => actions.push(Action::Unplug(Target::Placeholder(i))),
                _ => {}
            }
            if !matches!(self.seats[i], Seat::Physical) {
                self.seats[i] = Seat::Empty;
            }
        }
        // Re-plug in ascending order so each lands on its seat.
        for i in from..SEAT_COUNT {
            match &desired[i] {
                Seat::Virtual(x) => {
                    self.pending.push((Target::Pad(x.clone()), i, None));
                    actions.push(Action::Plug { target: Target::Pad(x.clone()), expected: i, hold: None });
                }
                Seat::Placeholder(h) => {
                    self.pending.push((Target::Placeholder(i), i, Some(h.clone())));
                    actions.push(Action::Plug { target: Target::Placeholder(i), expected: i, hold: Some(h.clone()) });
                }
                _ => {}
            }
        }
        Ok(actions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(seats: &mut Seats, actions: &[Action]) {
        for a in actions {
            if let Action::Plug { target, expected, .. } = a {
                assert_eq!(seats.landed(target, *expected), None, "unexpected landing for {target:?}");
            }
        }
    }

    fn pad(id: &str) -> Target {
        Target::Pad(id.to_string())
    }

    #[test]
    fn first_pad_takes_seat_one() {
        let mut s = Seats::new(30_000);
        let a = s.sony_arrived("a", "A");
        assert_eq!(a, vec![Action::Plug { target: pad("a"), expected: 0, hold: None }]);
        apply(&mut s, &a);
        assert_eq!(s.seat_of("a"), Some(0));
        assert!(s.sony_arrived("a", "A").is_empty(), "no double-plug");
    }

    #[test]
    fn physical_pads_block_seats() {
        let mut s = Seats::new(0);
        s.physical_scan([true, false, false, false]);
        let a = s.sony_arrived("a", "A");
        assert_eq!(a, vec![Action::Plug { target: pad("a"), expected: 1, hold: None }]);
        apply(&mut s, &a);
        assert_eq!(s.allowed_moves("a"), vec![2, 3]);
        assert!(s.reassign("a", 0).is_err());
    }

    #[test]
    fn leaving_holds_the_seat_and_reconnect_gets_it_back() {
        let mut s = Seats::new(30_000);
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a);
        let b = s.sony_arrived("b", "B");
        apply(&mut s, &b);
        // A leaves at t=0: unplug + placeholder at seat 0.
        let left = s.sony_left("a", "A", 0);
        assert_eq!(left[0], Action::Unplug(pad("a")));
        assert!(matches!(&left[1], Action::Plug { target: Target::Placeholder(0), expected: 0, hold: Some(h) } if h.for_id == "a" && h.until_ms == Some(30_000)));
        apply(&mut s, &left);
        assert!(matches!(s.seats()[0], Seat::Placeholder(_)));
        // B stays put.
        assert_eq!(s.seat_of("b"), Some(1));
        // A returns at t=10s: placeholder released, A lands on 0.
        let back = s.sony_arrived("a", "A");
        assert_eq!(back, vec![
            Action::Unplug(Target::Placeholder(0)),
            Action::Plug { target: pad("a"), expected: 0, hold: None },
        ]);
        apply(&mut s, &back);
        assert_eq!(s.seat_of("a"), Some(0));
        assert_eq!(s.seat_of("b"), Some(1));
    }

    #[test]
    fn second_player_rejoining_does_not_steal_seat_one() {
        let mut s = Seats::new(30_000);
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a);
        let b = s.sony_arrived("b", "B");
        apply(&mut s, &b);
        // Both drop; A first (hold on 0), then B (hold on 1).
        let la = s.sony_left("a", "A", 0);
        apply(&mut s, &la);
        let lb = s.sony_left("b", "B", 100);
        apply(&mut s, &lb);
        // B returns first: must land on seat 1, not 0.
        let back = s.sony_arrived("b", "B");
        assert_eq!(back, vec![
            Action::Unplug(Target::Placeholder(1)),
            Action::Plug { target: pad("b"), expected: 1, hold: None },
        ]);
        apply(&mut s, &back);
        assert_eq!(s.seat_of("b"), Some(1));
        assert!(matches!(s.seats()[0], Seat::Placeholder(_)));
    }

    #[test]
    fn holds_expire() {
        let mut s = Seats::new(1_000);
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a);
        let left = s.sony_left("a", "A", 0);
        apply(&mut s, &left);
        assert!(s.tick(999).is_empty());
        assert_eq!(s.tick(1_000), vec![Action::Unplug(Target::Placeholder(0))]);
        assert_eq!(s.seats()[0], Seat::Empty);
    }

    #[test]
    fn hold_disabled_when_zero() {
        let mut s = Seats::new(0);
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a);
        assert_eq!(s.sony_left("a", "A", 0), vec![Action::Unplug(pad("a"))]);
    }

    #[test]
    fn hold_is_skipped_when_a_lower_seat_is_free() {
        let mut s = Seats::new(30_000);
        s.physical_scan([true, false, false, false]);
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a); // seat 1
        s.physical_scan([false, false, false, false]); // xbox at 0 left
        let left = s.sony_left("a", "A", 0);
        assert_eq!(left, vec![Action::Unplug(pad("a"))], "placeholder would land on 0, so no hold");
    }

    #[test]
    fn full_house_waits_then_seats() {
        let mut s = Seats::new(0);
        for id in ["a", "b", "c", "d"] {
            let a = s.sony_arrived(id, id);
            apply(&mut s, &a);
        }
        assert!(s.sony_arrived("e", "E").is_empty());
        assert_eq!(s.waiting().collect::<Vec<_>>(), vec!["e"]);
        let left = s.sony_left("b", "B", 0);
        assert_eq!(left, vec![
            Action::Unplug(pad("b")),
            Action::Plug { target: pad("e"), expected: 1, hold: None },
        ]);
        apply(&mut s, &left);
        assert_eq!(s.seat_of("e"), Some(1));
        assert!(s.waiting().next().is_none());
    }

    #[test]
    fn reassign_swaps_and_leaves_lower_seats_alone() {
        let mut s = Seats::new(0);
        for id in ["a", "b", "c"] {
            let a = s.sony_arrived(id, id);
            apply(&mut s, &a);
        }
        // Move c (seat 2) to seat 1: only seats 1.. are touched.
        let acts = s.reassign("c", 1).unwrap();
        assert_eq!(acts, vec![
            Action::Unplug(pad("b")),
            Action::Unplug(pad("c")),
            Action::Plug { target: pad("c"), expected: 1, hold: None },
            Action::Plug { target: pad("b"), expected: 2, hold: None },
        ]);
        apply(&mut s, &acts);
        assert_eq!(s.seat_of("a"), Some(0));
        assert_eq!(s.seat_of("c"), Some(1));
        assert_eq!(s.seat_of("b"), Some(2));
    }

    #[test]
    fn reassign_upwards_fills_gaps_with_reserved_seats() {
        let mut s = Seats::new(0);
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a);
        let acts = s.reassign("a", 2).unwrap();
        assert_eq!(acts, vec![
            Action::Unplug(pad("a")),
            Action::Plug { target: Target::Placeholder(0), expected: 0, hold: Some(Hold { for_id: String::new(), for_name: String::new(), until_ms: None }) },
            Action::Plug { target: Target::Placeholder(1), expected: 1, hold: Some(Hold { for_id: String::new(), for_name: String::new(), until_ms: None }) },
            Action::Plug { target: pad("a"), expected: 2, hold: None },
        ]);
        apply(&mut s, &acts);
        assert_eq!(s.seat_of("a"), Some(2));
        assert!(s.tick(u64::MAX).is_empty(), "reserved seats never expire on their own");
        assert_eq!(s.release(0), vec![Action::Unplug(Target::Placeholder(0))]);
        assert_eq!(s.allowed_moves("a"), vec![0, 1, 3]);
    }

    #[test]
    fn reassign_errors() {
        let mut s = Seats::new(0);
        assert!(s.reassign("ghost", 1).is_err());
        let a = s.sony_arrived("a", "A");
        apply(&mut s, &a);
        assert!(s.reassign("a", 4).is_err());
        assert_eq!(s.reassign("a", 0).unwrap(), Vec::new());
    }

    #[test]
    fn landing_elsewhere_is_reported_not_hidden() {
        let mut s = Seats::new(0);
        let a = s.sony_arrived("a", "A");
        assert!(matches!(a[0], Action::Plug { expected: 0, .. }));
        let note = s.landed(&pad("a"), 2);
        assert!(note.unwrap().contains("seat 3"));
        assert_eq!(s.seat_of("a"), Some(2));
        assert!(!s.is_pending("a"));
    }

    #[test]
    fn failed_plug_clears_pending() {
        let mut s = Seats::new(0);
        s.sony_arrived("a", "A");
        assert!(s.is_pending("a"));
        s.plug_failed(&pad("a"));
        assert!(!s.is_pending("a"));
        assert!(!s.sony_arrived("a", "A").is_empty(), "can retry");
    }
}
