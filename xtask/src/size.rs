// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask size`: executable size against the B-SIZE budget (Appendix D).

/// B-SIZE gate (PERF-05, prompt 020 §3.9, ADR-0026): a hard gate. Prompt 020 §5.2 item 8 (owner
/// decision): 3,600,000 bytes for v2.0.0 (the toolbar pass), replacing 3,500,000.
pub const B_SIZE_SOFT_BYTES: u64 = 3_600_000;
/// B-SIZE stretch goal (3.0 MB, informational).
pub const B_SIZE_FINAL_BYTES: u64 = 3_000_000;

/// Outcome of a size check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeReport {
    /// File size in bytes.
    pub bytes: u64,
    /// Budget the file was checked against.
    pub budget: u64,
}

impl SizeReport {
    /// `true` when the file is within the budget.
    pub fn ok(&self) -> bool {
        self.bytes <= self.budget
    }

    /// Human-readable one-line summary.
    pub fn summary(&self) -> String {
        format!(
            "size: {} bytes ({:.2} MiB) — budget {} bytes ({:.2} MiB, B-SIZE gate; stretch {}) — {}",
            self.bytes,
            kib(self.bytes) / 1024.0,
            self.budget,
            kib(self.budget) / 1024.0,
            B_SIZE_FINAL_BYTES,
            if self.ok() { "OK" } else { "OVER BUDGET" }
        )
    }
}

/// Convert bytes to KiB for display.
#[allow(clippy::cast_precision_loss)]
pub fn kib(bytes: u64) -> f64 {
    bytes as f64 / 1024.0
}

/// Check a size against the current budget.
pub fn check(bytes: u64) -> SizeReport {
    SizeReport {
        bytes,
        budget: B_SIZE_SOFT_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_size_within_budget() {
        assert!(check(202_240).ok());
        assert!(check(B_SIZE_SOFT_BYTES).ok());
    }

    #[test]
    fn b_size_over_budget_fails() {
        let r = check(B_SIZE_SOFT_BYTES + 1);
        assert!(!r.ok());
        assert!(r.summary().contains("OVER BUDGET"));
    }

    #[test]
    fn kib_conversion() {
        assert!((kib(2048) - 2.0).abs() < f64::EPSILON);
    }
}
