// SPDX-License-Identifier: AGPL-3.0-or-later
//! F-38 (prompt 019): a timer-free throttle. Idle has no timers (PERFORMANCE rule: timers only
//! while something animates), so a rate-limited job runs at the event that asks for it when the
//! last run is old enough; otherwise it is marked dirty and runs at the next event, on document
//! close or at exit (`flush`).

/// Minimum gap between two jump-list host starts (P-53).
pub const JUMPLIST_GAP_MS: u64 = 30_000;

/// Rate limit without a timer. Times are monotonic milliseconds.
#[derive(Debug, Clone, Copy)]
pub struct Throttle {
    gap: u64,
    last: Option<u64>,
    dirty: bool,
}

impl Throttle {
    /// A throttle that runs at most once per `gap_ms` on [`Throttle::change`].
    #[must_use]
    pub const fn new(gap_ms: u64) -> Self {
        Self {
            gap: gap_ms,
            last: None,
            dirty: false,
        }
    }

    /// Something changed at `now`: `true` = run the job now; `false` = it is pending (dirty).
    pub fn change(&mut self, now: u64) -> bool {
        if self.last.is_none_or(|l| now.saturating_sub(l) > self.gap) {
            self.last = Some(now);
            self.dirty = false;
            true
        } else {
            self.dirty = true;
            false
        }
    }

    /// Document close / exit: `true` = a pending change must run now.
    pub fn flush(&mut self, now: u64) -> bool {
        if self.dirty {
            self.dirty = false;
            self.last = Some(now);
            true
        } else {
            false
        }
    }

    /// The pending change was undone before it ran.
    pub fn clear(&mut self) {
        self.dirty = false;
    }

    /// A change is waiting for the next event.
    #[must_use]
    pub const fn dirty(&self) -> bool {
        self.dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f38_first_change_runs_now_then_marks_dirty() {
        let mut t = Throttle::new(JUMPLIST_GAP_MS);
        assert!(t.change(1_000), "first change runs at once");
        assert!(!t.change(5_000), "within 30 s: pending");
        assert!(t.dirty());
        assert!(
            !t.change(31_000),
            "exactly 30 s after the run: still pending"
        );
        assert!(t.change(31_001), "next change after > 30 s runs");
        assert!(!t.dirty());
    }

    #[test]
    fn f38_flush_runs_only_when_dirty() {
        let mut t = Throttle::new(JUMPLIST_GAP_MS);
        assert!(!t.flush(0), "nothing pending");
        assert!(t.change(0));
        assert!(!t.flush(10), "already applied");
        assert!(!t.change(10));
        assert!(t.flush(20), "close / exit applies the pending change");
        assert!(!t.flush(30));
        assert!(!t.change(40), "the flush counts as a run");
        t.clear();
        assert!(!t.flush(50), "an undone change is not flushed");
    }
}
