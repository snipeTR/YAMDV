// SPDX-License-Identifier: AGPL-3.0-or-later
//! Timing and memory helpers for measurements (T1/T4, `mem-stats`).
#![allow(unsafe_code)]
// Pixel/DIP geometry: float<->int conversions are bounded by window/bitmap sizes (ADR-0007).
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::indexing_slicing
)]

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::ProcessStatus::{
    GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
    PROCESS_MEMORY_COUNTERS_EX2,
};
use windows::Win32::System::SystemInformation::GetSystemTimePreciseAsFileTime;
use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

/// Process CPU time (kernel + user) in milliseconds (driver `cpu_delta_ms`, T7 after a cancel).
pub fn cpu_ms() -> u64 {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid pseudo-handle and out-pointers.
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_err() {
            return 0;
        }
    }
    (ft(k) + ft(u)) / 10_000
}

/// Milliseconds since the process was created (first-paint metrics T1b/T1L/T2).
pub fn ms_since_process_start() -> u64 {
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: valid pseudo-handle and out-pointers.
    unsafe {
        if GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u).is_err() {
            return 0;
        }
        let now = GetSystemTimePreciseAsFileTime();
        ft(now).saturating_sub(ft(c)) / 10_000
    }
}

/// Current commit (`PrivateUsage`) in bytes.
pub fn commit_bytes() -> u64 {
    let mut ex = PROCESS_MEMORY_COUNTERS_EX {
        cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: correctly sized structure with cb set.
    let r = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut ex).cast::<PROCESS_MEMORY_COUNTERS>(),
            ex.cb,
        )
    };
    if r.is_ok() { ex.PrivateUsage as u64 } else { 0 }
}

/// Private working set (PWS) in bytes; 0 before Windows 10 1809.
pub fn private_ws_bytes() -> u64 {
    let mut ex = PROCESS_MEMORY_COUNTERS_EX2 {
        cb: u32::try_from(size_of::<PROCESS_MEMORY_COUNTERS_EX2>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: correctly sized structure with cb set.
    let r = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            (&raw mut ex).cast::<PROCESS_MEMORY_COUNTERS>(),
            ex.cb,
        )
    };
    if r.is_ok() {
        ex.PrivateWorkingSetSize as u64
    } else {
        0
    }
}

/// Percentile (nearest-rank) of `v` (sorted copy).
pub fn percentile(v: &[f64], p: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * s.len() as f64).ceil().max(1.0) as usize;
    s.get(rank - 1).copied().unwrap_or(0.0)
}

/// Text report of per-frame paint times (ms).
pub fn report(frames: &[f64]) -> String {
    let mut out = format!(
        "frames {}\np50_ms {:.3}\np95_ms {:.3}\np99_ms {:.3}\nmax_ms {:.3}\n",
        frames.len(),
        percentile(frames, 50.0),
        percentile(frames, 95.0),
        percentile(frames, 99.0),
        frames.iter().copied().fold(0.0, f64::max)
    );
    for f in frames {
        out.push_str(&format!("{f:.3}\n"));
    }
    out
}

/// Threads of this process right now (F-26 probe: watches must not add threads).
pub fn thread_count() -> u32 {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    // SAFETY: thread snapshot of the system, closed below.
    let Ok(snap) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) }) else {
        return 0;
    };
    let pid = std::process::id();
    let mut te = THREADENTRY32 {
        dwSize: std::mem::size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    let mut n = 0u32;
    // SAFETY: valid snapshot and a correctly sized entry.
    let mut ok = unsafe { Thread32First(snap, &mut te) }.is_ok();
    while ok {
        if te.th32OwnerProcessID == pid {
            n += 1;
        }
        // SAFETY: as above.
        ok = unsafe { Thread32Next(snap, &mut te) }.is_ok();
    }
    // SAFETY: opened above.
    let _ = unsafe { CloseHandle(snap) };
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 99.0), 99.0);
        assert_eq!(percentile(&v, 50.0), 50.0);
        assert!(report(&v).starts_with("frames 100\n"));
    }
}

/// Loaded modules of this process: (base name, image size in bytes), sorted by name
/// (`ui mem-report`, `ui modules-mark`; prompt 018a §1.1).
#[cfg(feature = "dev-tools")]
pub fn modules() -> Vec<(String, u32)> {
    use windows::Win32::Foundation::HMODULE;
    use windows::Win32::System::ProcessStatus::{
        EnumProcessModules, GetModuleBaseNameW, GetModuleInformation, MODULEINFO,
    };
    let mut hs = vec![HMODULE::default(); 512];
    let mut need = 0u32;
    let bytes = u32::try_from(hs.len() * size_of::<HMODULE>()).unwrap_or(0);
    // SAFETY: valid buffer of `bytes` bytes and out-pointer; pseudo handle of this process.
    if unsafe { EnumProcessModules(GetCurrentProcess(), hs.as_mut_ptr(), bytes, &mut need) }
        .is_err()
    {
        return Vec::new();
    }
    hs.truncate((need as usize / size_of::<HMODULE>()).min(512));
    let mut out: Vec<(String, u32)> = hs
        .iter()
        .map(|&h| {
            let mut name = [0u16; 260];
            let mut info = MODULEINFO::default();
            // SAFETY: module handles from EnumProcessModules; valid buffers.
            let (n, _) = unsafe {
                (
                    GetModuleBaseNameW(GetCurrentProcess(), Some(h), &mut name),
                    GetModuleInformation(
                        GetCurrentProcess(),
                        h,
                        &mut info,
                        u32::try_from(size_of::<MODULEINFO>()).unwrap_or(0),
                    ),
                )
            };
            (
                String::from_utf16_lossy(&name[..(n as usize).min(260)]).to_ascii_lowercase(),
                info.SizeOfImage,
            )
        })
        .collect();
    out.sort();
    out
}

/// Committed bytes of the default process heap (`HeapSummary`), 0 when unavailable.
#[cfg(feature = "dev-tools")]
pub fn process_heap_commit() -> u64 {
    use windows::Win32::System::Memory::{GetProcessHeap, HEAP_SUMMARY, HeapSummary};
    let mut s = HEAP_SUMMARY {
        cb: u32::try_from(size_of::<HEAP_SUMMARY>()).unwrap_or(0),
        ..Default::default()
    };
    // SAFETY: default heap handle and a correctly sized summary with cb set.
    unsafe {
        let Ok(h) = GetProcessHeap() else { return 0 };
        if !HeapSummary(h, 0, &mut s).as_bool() {
            return 0;
        }
    }
    s.cbCommitted as u64
}
