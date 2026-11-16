use std::time::{Duration, Instant};

const MARKS: [u64; 4] = [15, 60, 300, 900];

/// A pure clock-driven state machine. Only fixed phrases and elapsed seconds leave it.
pub struct WindowWatch {
    last_beat: Option<Instant>,
    visible: bool,
    episode_started: Option<Instant>,
    logged_marks: [bool; 4],
    last_tick: Instant,
}

impl WindowWatch {
    pub fn new(now: Instant) -> Self {
        Self { last_beat: None, visible: false, episode_started: None, logged_marks: [false; 4], last_tick: now }
    }

    fn clear(&mut self) {
        self.episode_started = None;
        self.logged_marks = [false; 4];
    }

    pub fn beat(&mut self, now: Instant, visible: bool) -> Option<String> {
        self.visible = visible;
        let line = if visible {
            self.last_beat = Some(now);
            self.episode_started.filter(|_| self.logged_marks.iter().any(|v| *v))
                .map(|start| format!("window answering again after {} s", now.duration_since(start).as_secs()))
        } else { None };
        self.clear();
        line
    }

    pub fn tick(&mut self, now: Instant) -> Vec<String> {
        let gap = now.duration_since(self.last_tick);
        self.last_tick = now;
        if gap > Duration::from_secs(20) {
            if self.last_beat.is_some() { self.last_beat = Some(now); }
            self.clear();
            return Vec::new();
        }
        let Some(last) = self.last_beat.filter(|_| self.visible) else { return Vec::new(); };
        let elapsed = now.duration_since(last).as_secs();
        let mut lines = Vec::new();
        for (i, mark) in MARKS.iter().enumerate() {
            if elapsed >= *mark && !self.logged_marks[i] {
                self.episode_started.get_or_insert(last);
                self.logged_marks[i] = true;
                lines.push(format!("window not answering for {mark} s"));
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn at(t: Instant, s: u64) -> Instant { t + Duration::from_secs(s) }

    #[test]
    fn no_beat_and_regular_beats_log_nothing() {
        let t = Instant::now();
        let mut w = WindowWatch::new(t);
        for s in (5..=100).step_by(5) { assert!(w.tick(at(t, s)).is_empty()); }
        let mut w = WindowWatch::new(t);
        for s in (0..=100).step_by(5) {
            assert!(w.beat(at(t, s), true).is_none());
            assert!(w.tick(at(t, s)).is_empty());
        }
    }

    #[test]
    fn stopped_visible_beats_log_each_mark_once_then_recover() {
        let t = Instant::now();
        let mut w = WindowWatch::new(t);
        w.beat(t, true);
        let mut lines = Vec::new();
        for s in (5..=65).step_by(5) { lines.extend(w.tick(at(t, s))); }
        assert_eq!(lines, ["window not answering for 15 s", "window not answering for 60 s"]);
        assert_eq!(w.beat(at(t, 67), true).as_deref(), Some("window answering again after 67 s"));
        assert!(w.beat(at(t, 68), true).is_none());
        assert!(w.tick(at(t, 70)).is_empty());
    }

    #[test]
    fn hidden_pauses_and_clears_without_recovery() {
        let t = Instant::now();
        let mut w = WindowWatch::new(t);
        w.beat(t, true);
        for s in [5, 10, 15] { w.tick(at(t, s)); }
        assert!(w.beat(at(t, 16), false).is_none());
        for s in (20..=1000).step_by(5) { assert!(w.tick(at(t, s)).is_empty()); }
        assert!(w.beat(at(t, 1001), true).is_none());
        assert!(w.tick(at(t, 1005)).is_empty());
    }

    #[test]
    fn suspend_gap_resets_the_watch_and_episode() {
        let t = Instant::now();
        let mut w = WindowWatch::new(t);
        w.beat(t, true);
        for s in [5, 10, 15] { w.tick(at(t, s)); }
        assert!(w.tick(at(t, 45)).is_empty());
        assert!(w.tick(at(t, 50)).is_empty());
        assert!(w.beat(at(t, 51), true).is_none());
    }

    #[test]
    fn nine_hundred_seconds_is_the_last_mark() {
        let t = Instant::now();
        let mut w = WindowWatch::new(t);
        w.beat(t, true);
        let mut lines = Vec::new();
        for s in (5..=1800).step_by(5) { lines.extend(w.tick(at(t, s))); }
        assert_eq!(lines, [15, 60, 300, 900].map(|s| format!("window not answering for {s} s")));
        assert_eq!(w.beat(at(t, 1801), true).as_deref(), Some("window answering again after 1801 s"));
    }
}
