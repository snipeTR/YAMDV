// SPDX-License-Identifier: AGPL-3.0-or-later
//! Wheel scroll model (R-REN-17, P-51; F-37): notches accumulate into one moving target, a new
//! notch retargets the running motion from its current velocity (critically damped follow), so
//! fast scrolling is never restarted, paused or slowed. Pure and clock-free: the caller passes
//! milliseconds, so 018a can unit-test it.

/// Notches closer than this (ms) count as fast scrolling.
pub const FAST_GAP_MS: f64 = 60.0;
/// More pending notches than this count as fast scrolling.
pub const FAST_PENDING: f64 = 3.0;
/// Ease length for normal wheel motion (ms).
pub const EASE_MS: f64 = 150.0;
/// Ease length while scrolling fast (ms, P-51 "at most 60 ms").
pub const FAST_EASE_MS: f64 = 60.0;
/// Longest gap between two animation steps (ms): one timer tick (15.6 ms) rounded up.
pub const FRAME_MS: f64 = 16.0;
/// Rest time before a smart settle onto a heading may start (ms).
pub const SETTLE_REST_MS: f64 = 150.0;
/// Largest distance (DIPs) from a heading top for a smart settle.
pub const SETTLE_PX: f64 = 24.0;
/// Length of the settle animation (ms).
pub const SETTLE_MS: f64 = 120.0;

/// Accumulated wheel motion.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wheel {
    /// Current position (DIPs).
    pub pos: f64,
    /// Target position (DIPs).
    pub target: f64,
    /// Velocity (DIPs per ms).
    pub vel: f64,
    /// Time of the last step (ms).
    pub t: f64,
    /// Time of the last notch (ms), `None` before the first one.
    pub last_notch: Option<f64>,
    /// Size of the last notch (DIPs, absolute).
    pub notch_px: f64,
    /// The last movement was fast (blocks a smart settle).
    pub fast: bool,
    /// Whether the motion is running.
    pub active: bool,
    /// Time the motion stopped (ms), for the settle rest.
    pub stopped_at: Option<f64>,
}

impl Wheel {
    /// Adds one notch of `dy` DIPs at time `now` while the view is at `pos` and may scroll in
    /// `0..=max`. The target accumulates; the velocity is kept.
    pub fn notch(&mut self, now: f64, pos: f64, dy: f64, max: f64) {
        if !self.active {
            self.pos = pos;
            self.target = pos;
            self.vel = 0.0;
            self.t = now;
        }
        self.step(now);
        self.active = true;
        let gap = self.last_notch.map_or(f64::INFINITY, |l| now - l);
        let old = self.target;
        self.target = (self.target + dy).clamp(0.0, max.max(0.0));
        self.notch_px = dy.abs().max(1.0);
        let pending = (self.target - self.pos).abs() / self.notch_px;
        self.fast = gap < FAST_GAP_MS || pending > FAST_PENDING;
        // Impulse: with the critically damped follow below, each notch on its own moves like an
        // exponential ease-out, and notches add up (superposition) instead of restarting.
        self.vel += (self.target - old) / self.tau();
        self.last_notch = Some(now);
        self.stopped_at = None;
    }

    /// Ease length now in effect (ms).
    pub fn ease_ms(&self) -> f64 {
        if self.fast { FAST_EASE_MS } else { EASE_MS }
    }

    fn tau(&self) -> f64 {
        self.ease_ms() / 5.0
    }

    /// Moves the motion to `now`; returns the new position. A critically damped follow with
    /// time constant `ease / 5` is within 1 % of the target after one ease length.
    pub fn step(&mut self, now: f64) -> f64 {
        if !self.active {
            return self.pos;
        }
        let mut dt = (now - self.t).max(0.0);
        self.t = now;
        let tau = self.tau();
        // Sub-steps keep the integration stable when a frame is late.
        while dt > 0.0 {
            let h = dt.min(4.0);
            dt -= h;
            let x = self.pos - self.target;
            let acc = -x / (tau * tau) - 2.0 * self.vel / tau;
            self.vel += acc * h;
            self.pos += self.vel * h;
        }
        // P-51 "at most 60 ms" while scrolling fast: the follow lags a free-spinning stream by
        // ~2·tau·velocity, and the critically damped tail would take ~100 ms to cover it, so
        // the motion ends one fast ease length after the last input (018a scroll-fling): at the
        // first step after `FAST_EASE_MS - 1.5 × FRAME_MS`, so a step on a late timer tick
        // (CI measured 61.7 ms with `- FRAME_MS`) still lands in time. The remaining distance
        // then is a few DIPs, a few ms of the running motion.
        let fast_done = self.fast
            && self
                .last_notch
                .is_some_and(|l| now - l >= FAST_EASE_MS - 1.5 * FRAME_MS);
        if fast_done || ((self.pos - self.target).abs() < 0.5 && self.vel.abs() < 0.05) {
            self.pos = self.target;
            self.vel = 0.0;
            self.active = false;
            self.stopped_at = Some(now);
        }
        self.pos
    }

    /// Shifts position and target by `dy` (the viewer moved the scroll to keep content in
    /// place after a height correction).
    pub fn shift(&mut self, dy: f64) {
        self.pos += dy;
        self.target += dy;
    }

    /// Stops the motion (direct scrolling: touchpad, thumb drag, jumps).
    pub fn stop(&mut self) {
        self.active = false;
        self.vel = 0.0;
        self.fast = false;
        self.stopped_at = None;
    }
}

/// Smart settle (P-51): the heading top to settle onto, when the view rested ≥ 150 ms after a
/// slow movement and a heading top lies within 24 DIPs of `pos`.
pub fn settle_target(w: &Wheel, now: f64, pos: f64, heading_tops: &[f64]) -> Option<f64> {
    let rested = w.stopped_at.is_some_and(|s| now - s >= SETTLE_REST_MS);
    if !rested || w.fast || w.active {
        return None;
    }
    heading_tops
        .iter()
        .copied()
        .filter(|t| (t - pos).abs() <= SETTLE_PX && (t - pos).abs() >= 0.5)
        .min_by(|a, b| (a - pos).abs().total_cmp(&(b - pos).abs()))
}

/// Summary of a dev-tools scroll trace (018a `scroll-fling`): one line per event,
/// `"<ms> <event> ..."` with the events `notch d= y= target=`, `step y= target=`,
/// `prepare <ms>ms y=`, `frame <ms>ms`, `settle <a> -> <b>` and `anchor-fix <dy>`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TraceStats {
    /// Painted frames.
    pub frames: usize,
    /// Frame time p99 (ms, nearest rank).
    pub p99_ms: f64,
    /// Frame time p95 (ms, nearest rank; F-52).
    pub p95_ms: f64,
    /// Every frame moved the view in the scroll direction (no reversal > 0.5 DIP).
    pub monotonic: bool,
    /// Largest run of consecutive frames without movement while the motion was still running.
    pub max_stall: usize,
    /// Distance (DIPs) between the final position and start + the sum of the notch deltas.
    pub end_err: f64,
    /// Time (ms) from the last wheel input to the last position change.
    pub settle_ms: f64,
    /// Smart settles started.
    pub settles: usize,
    /// Wheel messages (summed over batches).
    pub inputs: u32,
}

fn field(s: &str, key: &str) -> Option<f64> {
    let i = s.find(key)? + key.len();
    let rest = s.get(i..)?;
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .unwrap_or(rest.len());
    rest.get(..end)?.parse().ok()
}

/// Computes [`TraceStats`] from trace lines.
#[must_use]
pub fn trace_stats(lines: &[String]) -> TraceStats {
    let mut st = TraceStats {
        monotonic: true,
        ..TraceStats::default()
    };
    let mut frame_ms = Vec::new();
    // Positions seen at each frame (corrected for anchor fixes) and their times.
    let mut frame_pos: Vec<(f64, f64)> = Vec::new();
    let mut cur: Option<f64> = None;
    let mut fix = 0.0;
    let mut start: Option<f64> = None;
    let mut sum = 0.0;
    let mut last_input = 0.0;
    let mut last_change = 0.0;
    for l in lines {
        let Some((t, ev)) = l.split_once(' ') else {
            continue;
        };
        let Ok(t) = t.parse::<f64>() else { continue };
        let pos_before = cur;
        if let Some(rest) = ev.strip_prefix("notch ") {
            let d = field(rest, "d=").unwrap_or(0.0);
            let y = field(rest, " y=").unwrap_or(0.0);
            if start.is_none() {
                start = Some(y - fix);
                cur = Some(y - fix);
            }
            sum += d;
            st.inputs += rest
                .split_once("batch=")
                .and_then(|(_, b)| b.trim().parse::<u32>().ok())
                .unwrap_or(1)
                .max(1);
            last_input = t;
        } else if let Some(rest) = ev
            .strip_prefix("step ")
            .or_else(|| ev.strip_prefix("prepare "))
        {
            if let Some(y) = field(rest, "y=") {
                cur = Some(y - fix);
            }
        } else if let Some(rest) = ev.strip_prefix("frame ") {
            frame_ms.push(field(rest, "").unwrap_or(0.0));
            if let Some(y) = cur {
                frame_pos.push((t, y));
            }
        } else if ev.starts_with("settle ") {
            st.settles += 1;
        } else if let Some(rest) = ev.strip_prefix("anchor-fix ") {
            fix += field(rest, "").unwrap_or(0.0);
        }
        if let (Some(a), Some(b)) = (pos_before, cur)
            && (a - b).abs() > 1e-6
        {
            last_change = t;
        }
    }
    st.frames = frame_ms.len();
    if !frame_ms.is_empty() {
        frame_ms.sort_by(f64::total_cmp);
        // Nearest rank: ceil(0.99 · n).
        let k = (frame_ms.len() * 99).div_ceil(100);
        st.p99_ms = frame_ms
            .get(k.saturating_sub(1).min(frame_ms.len() - 1))
            .copied()
            .unwrap_or(0.0);
        let k95 = (frame_ms.len() * 95).div_ceil(100);
        st.p95_ms = frame_ms
            .get(k95.saturating_sub(1).min(frame_ms.len() - 1))
            .copied()
            .unwrap_or(0.0);
    }
    let start = start.unwrap_or(0.0);
    let dir = sum.signum();
    let end = cur.unwrap_or(start);
    st.end_err = (end - (start + sum)).abs();
    st.settle_ms = (last_change - last_input).max(0.0);
    // Frames while the motion runs: from the first input until the last position change.
    let mut run = 0usize;
    for w in frame_pos.windows(2) {
        let (Some(&(_, a)), Some(&(tb, b))) = (w.first(), w.get(1)) else {
            continue;
        };
        if (b - a) * dir < -0.5 {
            st.monotonic = false;
        }
        if tb <= last_change && (b - a).abs() < 1e-6 {
            run += 1;
            st.max_stall = st.max_stall.max(run);
        } else {
            run = 0;
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(w: &mut Wheel, from: f64, to: f64) -> f64 {
        let mut t = from;
        while t < to {
            t += 4.0;
            w.step(t);
        }
        w.pos
    }

    #[test]
    fn single_notch_eases_to_target_without_overshoot() {
        let mut w = Wheel::default();
        w.notch(0.0, 100.0, 120.0, 10_000.0);
        assert!(!w.fast);
        assert!((w.target - 220.0).abs() < 1e-9);
        let mut last = 100.0;
        let mut t = 0.0;
        while w.active && t < 1000.0 {
            t += 4.0;
            let y = w.step(t);
            assert!(y + 1e-9 >= last, "monotonic");
            assert!(y <= 220.0 + 1e-9, "no overshoot");
            last = y;
        }
        assert!(!w.active && (w.pos - 220.0).abs() < 1e-9);
        assert!(
            t <= EASE_MS * 2.0,
            "settles within two ease lengths ({t} ms)"
        );
    }

    #[test]
    fn new_notch_retargets_and_keeps_velocity() {
        let mut w = Wheel::default();
        w.notch(0.0, 0.0, 120.0, 10_000.0);
        run(&mut w, 0.0, 40.0);
        let v = w.vel;
        assert!(v > 0.0);
        w.notch(40.0, w.pos, 120.0, 10_000.0);
        assert!((w.target - 240.0).abs() < 1e-9, "the target accumulates");
        assert!(
            w.vel > v,
            "the running motion speeds up, it is not restarted"
        );
    }

    #[test]
    fn fast_threshold_by_gap_and_pending() {
        let mut w = Wheel::default();
        w.notch(0.0, 0.0, 120.0, 1e6);
        assert!(!w.fast);
        w.notch(FAST_GAP_MS - 1.0, w.pos, 120.0, 1e6);
        assert!(w.fast, "a notch closer than FAST_GAP_MS is fast");
        assert!((w.ease_ms() - FAST_EASE_MS).abs() < 1e-9);
        // Slow notches but many pending: fast as well.
        // (a long jump is still far from done when a small notch arrives).
        let mut w = Wheel::default();
        w.notch(0.0, 0.0, 1200.0, 1e6);
        w.notch(FAST_GAP_MS + 10.0, w.pos, 5.0, 1e6);
        assert!((w.target - w.pos) / 5.0 > FAST_PENDING);
        assert!(w.fast);
    }

    #[test]
    fn burst_of_30_notches_ends_on_the_sum() {
        let mut w = Wheel::default();
        let mut t = 0.0;
        let mut last = 0.0;
        for _ in 0..30 {
            w.notch(t, w.pos, 120.0, 1e6);
            t += 10.0;
            let y = w.step(t);
            assert!(y >= last);
            last = y;
        }
        let end = run(&mut w, t, t + 1000.0);
        assert!((end - 3600.0).abs() < 1e-9);
        assert!(!w.active);
    }

    #[test]
    fn free_spin_stream_reaches_target_within_60_ms() {
        let mut w = Wheel::default();
        let mut t = 0.0;
        for _ in 0..1500 {
            w.notch(t, w.pos, 15.0, 1e9);
            t += 3.0;
            w.step(t);
        }
        let last_input = t - 3.0;
        // The window steps on a 15.6 ms timer tick.
        while w.active && t < last_input + 500.0 {
            t += 15.6;
            w.step(t);
        }
        assert!(!w.active);
        assert!(t - last_input <= 60.0, "{} ms", t - last_input);
        assert!((w.pos - 22_500.0).abs() < 1e-9);
    }

    #[test]
    fn free_spin_ends_in_time_on_late_ticks() {
        // 018a scroll-fling: a timer tick 4 ms late, at any phase, still ends within 60 ms.
        for phase in 0..20 {
            let mut w = Wheel::default();
            let mut t = 0.0;
            for _ in 0..300 {
                w.notch(t, w.pos, 15.0, 1e9);
                t += 3.0;
                w.step(t);
            }
            let last_input = t - 3.0;
            t = last_input + f64::from(phase);
            w.step(t);
            while w.active && t < last_input + 500.0 {
                t += 19.6;
                w.step(t);
            }
            assert!(!w.active);
            assert!(
                t - last_input <= 60.0,
                "phase {phase}: {} ms",
                t - last_input
            );
        }
    }

    #[test]
    fn clamps_to_range() {
        let mut w = Wheel::default();
        w.notch(0.0, 50.0, -120.0, 1000.0);
        assert!(w.target.abs() < 1e-9);
        w.stop();
        w.notch(1000.0, 990.0, 120.0, 1000.0);
        assert!((w.target - 1000.0).abs() < 1e-9);
    }

    #[test]
    fn settle_conditions() {
        let mut w = Wheel::default();
        w.notch(0.0, 0.0, 120.0, 1e6);
        let _ = run(&mut w, 0.0, 1000.0);
        let stop = w.stopped_at.unwrap_or(0.0);
        let heads = [130.0, 500.0];
        // Rested long enough after a slow notch, heading 10 px away: settle.
        assert_eq!(
            settle_target(&w, stop + SETTLE_REST_MS, 120.0, &heads),
            Some(130.0)
        );
        // Too early.
        assert_eq!(
            settle_target(&w, stop + SETTLE_REST_MS - 1.0, 120.0, &heads),
            None
        );
        // Too far from a heading.
        assert_eq!(
            settle_target(&w, stop + 1000.0, 120.0, &[120.0 + SETTLE_PX + 1.0]),
            None
        );
        // Already on it.
        assert_eq!(settle_target(&w, stop + 1000.0, 130.0, &heads), None);
        // A fast movement never settles.
        let mut f = w;
        f.fast = true;
        assert_eq!(settle_target(&f, stop + 1000.0, 120.0, &heads), None);
        // A running motion never settles.
        let mut a = w;
        a.active = true;
        assert_eq!(settle_target(&a, stop + 1000.0, 120.0, &heads), None);
    }

    #[test]
    fn stop_is_direct_scrolling() {
        let mut w = Wheel::default();
        w.notch(0.0, 0.0, 120.0, 1e6);
        w.stop();
        assert!(!w.active && w.vel == 0.0 && !w.fast);
        assert!(
            (w.step(100.0) - w.pos).abs() < 1e-9,
            "no animation after stop"
        );
    }

    #[test]
    fn trace_p95_ignores_one_slow_frame_of_forty() {
        let mut lines = vec!["0.0 notch d=120.0 y=0.0 target=120.0 fast=false batch=1".to_string()];
        for i in 0..40 {
            let ms = if i == 7 { 20.0 } else { 10.0 };
            lines.push(format!("{}.0 frame {ms:.2}ms", i + 1));
        }
        let s = trace_stats(&lines);
        assert_eq!(s.frames, 40);
        assert!((s.p99_ms - 20.0).abs() < 1e-9, "p99 = the slowest of 40");
        assert!((s.p95_ms - 10.0).abs() < 1e-9);
    }

    #[test]
    fn trace_stats_summary() {
        let l = |s: &str| s.to_string();
        let lines = vec![
            l("0.0 notch d=120.0 y=0.0 target=120.0 fast=false batch=1"),
            l("1.0 step y=10.0 target=120.0"),
            l("2.0 frame 8.00ms"),
            l("10.0 notch d=120.0 y=10.0 target=240.0 fast=true batch=2"),
            l("11.0 step y=60.0 target=240.0"),
            l("12.0 frame 9.00ms"),
            l("20.0 step y=200.0 target=240.0"),
            l("21.0 frame 12.00ms"),
            l("30.0 step y=240.0 target=240.0"),
            l("31.0 frame 7.00ms"),
        ];
        let s = trace_stats(&lines);
        assert_eq!(s.frames, 4);
        assert!((s.p99_ms - 12.0).abs() < 1e-9);
        assert!(s.monotonic);
        assert_eq!(s.max_stall, 0);
        assert!(s.end_err.abs() < 1e-9);
        assert!((s.settle_ms - 20.0).abs() < 1e-9);
        assert_eq!(s.settles, 0);
        assert_eq!(s.inputs, 3);
        // A stall of two frames and a reversal.
        let bad = vec![
            l("0.0 notch d=120.0 y=0.0 target=120.0 fast=false batch=1"),
            l("1.0 step y=50.0 target=120.0"),
            l("2.0 frame 8.00ms"),
            l("3.0 frame 8.00ms"),
            l("4.0 frame 8.00ms"),
            l("5.0 step y=40.0 target=120.0"),
            l("6.0 frame 8.00ms"),
            l("7.0 step y=120.0 target=120.0"),
            l("8.0 frame 8.00ms"),
            l("9.0 settle 120.0 -> 130.0"),
        ];
        let s = trace_stats(&bad);
        assert!(!s.monotonic);
        assert_eq!(s.max_stall, 2);
        assert_eq!(s.settles, 1);
    }
}
