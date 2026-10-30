// SPDX-License-Identifier: AGPL-3.0-or-later
//! Allocation-site tags (prompt 019 F-39, `alloc-sites` measurement builds only): the global
//! allocator records the call stack of every live Rust allocation, grouped by site. With
//! `YAMDV_ALLOC_DUMP=<file>` (and `YAMDV_ALLOC_DUMP_MS`, default 8000) a thread writes the sites
//! by live bytes as `live_bytes count rva rva …` lines (RVAs in this exe, `-` elsewhere); the
//! diag workflow symbolizes them with `llvm-symbolizer --relative-address`. Never in releases.
//! Self-contained so the F-39 diag can drop it into a `phase-v2g` tree unchanged.
#![allow(unsafe_code)]
// Measurement-only code: every index is reduced modulo its table size.
#![allow(clippy::indexing_slicing)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::UnsafeCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use windows::Win32::System::Diagnostics::Debug::RtlCaptureStackBackTrace;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;

const FRAMES: usize = 16;
const LIVE_CAP: usize = 1 << 18;
const SITE_CAP: usize = 1 << 14;

#[derive(Clone, Copy)]
struct Live {
    ptr: usize,
    size: usize,
    site: u32,
}

#[derive(Clone, Copy)]
struct Site {
    hash: u32,
    used: bool,
    frames: [usize; FRAMES],
    live: usize,
    count: usize,
}

struct Tables {
    live: [Live; LIVE_CAP],
    sites: [Site; SITE_CAP],
}

struct Shared(UnsafeCell<Tables>);
// SAFETY: every access holds LOCK.
unsafe impl Sync for Shared {}

static TABLES: Shared = Shared(UnsafeCell::new(Tables {
    live: [Live {
        ptr: 0,
        size: 0,
        site: 0,
    }; LIVE_CAP],
    sites: [Site {
        hash: 0,
        used: false,
        frames: [0; FRAMES],
        live: 0,
        count: 0,
    }; SITE_CAP],
}));
static LOCK: AtomicBool = AtomicBool::new(false);
/// Thread that is writing the dump: its own allocations are not tracked.
static BYPASS: AtomicU32 = AtomicU32::new(0);
static UNTRACKED: AtomicUsize = AtomicUsize::new(0);

fn lock() {
    while LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        std::hint::spin_loop();
    }
}

fn unlock() {
    LOCK.store(false, Ordering::Release);
}

fn bypass() -> bool {
    let b = BYPASS.load(Ordering::Relaxed);
    // SAFETY: no preconditions.
    b != 0 && b == unsafe { GetCurrentThreadId() }
}

fn slot(p: usize) -> usize {
    (p >> 4).wrapping_mul(0x9E37_79B9_7F4A_7C15) >> (64 - 18)
}

fn track(p: *mut u8, size: usize) {
    if bypass() {
        return;
    }
    let mut frames = [std::ptr::null_mut::<c_void>(); FRAMES];
    let mut hash = 0u32;
    // SAFETY: valid output buffer and hash pointer; the call does not allocate.
    let n = usize::from(unsafe { RtlCaptureStackBackTrace(2, &mut frames, Some(&mut hash)) });
    let mut key = [0usize; FRAMES];
    for (k, f) in key.iter_mut().zip(&frames[..n]) {
        *k = *f as usize;
    }
    lock();
    // SAFETY: LOCK held.
    let t = unsafe { &mut *TABLES.0.get() };
    let mut s = hash as usize % SITE_CAP;
    let mut site = None;
    for _ in 0..SITE_CAP {
        let e = &mut t.sites[s];
        if !e.used {
            *e = Site {
                hash,
                used: true,
                frames: key,
                live: 0,
                count: 0,
            };
        }
        if e.hash == hash && e.frames == key {
            site = Some(s);
            break;
        }
        s = (s + 1) % SITE_CAP;
    }
    let mut i = slot(p as usize);
    let mut placed = false;
    if let Some(s) = site {
        for _ in 0..LIVE_CAP / 2 {
            if t.live[i].ptr == 0 {
                t.live[i] = Live {
                    ptr: p as usize,
                    size,
                    site: u32::try_from(s).unwrap_or(0),
                };
                t.sites[s].live += size;
                t.sites[s].count += 1;
                placed = true;
                break;
            }
            i = (i + 1) % LIVE_CAP;
        }
    }
    unlock();
    if !placed {
        UNTRACKED.fetch_add(size, Ordering::Relaxed);
    }
}

fn untrack(p: *mut u8) {
    if bypass() {
        return;
    }
    lock();
    // SAFETY: LOCK held.
    let t = unsafe { &mut *TABLES.0.get() };
    let mut i = slot(p as usize);
    for _ in 0..LIVE_CAP {
        let e = t.live[i];
        if e.ptr == 0 {
            break;
        }
        if e.ptr == p as usize {
            let s = &mut t.sites[e.site as usize];
            s.live -= e.size;
            s.count -= 1;
            // Backward-shift deletion keeps the linear probes intact.
            let mut hole = i;
            let mut j = (i + 1) % LIVE_CAP;
            while t.live[j].ptr != 0 {
                let home = slot(t.live[j].ptr);
                let dist_j = (j + LIVE_CAP - home) % LIVE_CAP;
                let dist_hole = (hole + LIVE_CAP - home) % LIVE_CAP;
                if dist_hole <= dist_j {
                    t.live[hole] = t.live[j];
                    hole = j;
                }
                j = (j + 1) % LIVE_CAP;
            }
            t.live[hole] = Live {
                ptr: 0,
                size: 0,
                site: 0,
            };
            break;
        }
        i = (i + 1) % LIVE_CAP;
    }
    unlock();
}

/// The system allocator with per-site live counters.
pub struct Tagged;

// SAFETY: every call forwards to `System` with the same layout; the tables are side data.
unsafe impl GlobalAlloc for Tagged {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            track(p, l.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let p = unsafe { System.alloc_zeroed(l) };
        if !p.is_null() {
            track(p, l.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        untrack(p);
        // SAFETY: forwarded unchanged.
        unsafe { System.dealloc(p, l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, new: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged.
        let q = unsafe { System.realloc(p, l, new) };
        if !q.is_null() {
            untrack(p);
            track(q, new);
        }
        q
    }
}

/// Starts the dump thread when `YAMDV_ALLOC_DUMP` is set.
pub fn start() {
    let Ok(path) = std::env::var("YAMDV_ALLOC_DUMP") else {
        return;
    };
    // F-41: a comma list ("3000,8000") writes one dump per time, as `<file>.<i>`.
    let times: Vec<u64> = std::env::var("YAMDV_ALLOC_DUMP_MS")
        .ok()
        .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .filter(|v: &Vec<u64>| !v.is_empty())
        .unwrap_or_else(|| vec![8000]);
    let _ = std::thread::Builder::new()
        .name("alloc-sites".into())
        .spawn(move || {
            let t0 = std::time::Instant::now();
            let many = times.len() > 1;
            for (i, ms) in times.iter().enumerate() {
                let at = std::time::Duration::from_millis(*ms);
                if let Some(wait) = at.checked_sub(t0.elapsed()) {
                    std::thread::sleep(wait);
                }
                if many {
                    dump(&format!("{path}.{i}"));
                } else {
                    dump(&path);
                }
            }
        });
}

fn dump(path: &str) {
    // SAFETY: no preconditions.
    BYPASS.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);
    // SAFETY: handle of this exe.
    let base = unsafe { GetModuleHandleW(None) }.map_or(0, |h| h.0 as usize);
    let mut rows: Vec<(usize, usize, [usize; FRAMES])> = Vec::with_capacity(SITE_CAP);
    lock();
    // SAFETY: LOCK held; this thread's allocations bypass the tables (no re-entry).
    let t = unsafe { &*TABLES.0.get() };
    let mut total = 0usize;
    for s in t.sites.iter().filter(|s| s.used && s.live > 0) {
        total += s.live;
        rows.push((s.live, s.count, s.frames));
    }
    unlock();
    rows.sort_by_key(|a| std::cmp::Reverse(a.0));
    let mut out = format!(
        "# live_tracked {total} untracked {} sites {}\n",
        UNTRACKED.load(Ordering::Relaxed),
        rows.len()
    );
    for (live, count, frames) in rows.iter().take(400) {
        out.push_str(&format!("{live} {count}"));
        for f in frames.iter().take_while(|f| **f != 0) {
            // The exe image is far smaller than 64 MiB.
            if *f >= base && *f - base < 64 << 20 {
                out.push_str(&format!(" 0x{:x}", *f - base - 1));
            } else {
                out.push_str(" -");
            }
        }
        out.push('\n');
    }
    out.push_str(&native_summary());
    let _ = std::fs::write(path, out);
}

/// Native side: busy bytes per process heap (`HeapWalk`) and private committed regions
/// (`VirtualQuery`), so a gap outside the Rust heap is visible too.
fn native_summary() -> String {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::Memory::{
        GetProcessHeaps, HeapLock, HeapUnlock, HeapWalk, MEM_COMMIT, MEM_PRIVATE,
        MEMORY_BASIC_INFORMATION, PROCESS_HEAP_ENTRY, VirtualQuery,
    };
    let mut out = String::new();
    let mut heaps = [HANDLE::default(); 64];
    // SAFETY: valid output slice.
    let n = (unsafe { GetProcessHeaps(&mut heaps) } as usize).min(heaps.len());
    for (i, h) in heaps[..n].iter().enumerate() {
        let (mut busy, mut blocks, mut free) = (0usize, 0usize, 0usize);
        // SAFETY: heap handle from GetProcessHeaps; walked under its lock.
        unsafe {
            if HeapLock(*h).is_err() {
                continue;
            }
            let mut e = PROCESS_HEAP_ENTRY::default();
            while HeapWalk(*h, &mut e).is_ok() {
                if e.wFlags & 0x0004 != 0 {
                    busy += e.cbData as usize;
                    blocks += 1;
                } else if e.wFlags == 0 {
                    free += e.cbData as usize;
                }
            }
            let _ = HeapUnlock(*h);
        }
        // F-41 (prompt 020): committed vs allocated per heap (decommit timing shows up here).
        let mut hs = windows::Win32::System::Memory::HEAP_SUMMARY {
            cb: u32::try_from(std::mem::size_of::<
                windows::Win32::System::Memory::HEAP_SUMMARY,
            >())
            .unwrap_or(0),
            ..Default::default()
        };
        // SAFETY: heap handle from GetProcessHeaps; valid, sized output struct.
        let _ = unsafe { windows::Win32::System::Memory::HeapSummary(*h, 0, &mut hs) };
        out.push_str(&format!(
            "# heap {i} busy {busy} blocks {blocks} free {free} committed {} allocated {} reserved {}\n",
            hs.cbCommitted, hs.cbAllocated, hs.cbReserved
        ));
    }
    let mut addr = 0usize;
    let mut total = 0usize;
    let mut mbi = MEMORY_BASIC_INFORMATION::default();
    let size = std::mem::size_of::<MEMORY_BASIC_INFORMATION>();
    // SAFETY: walks this process's address space with a valid output buffer.
    while unsafe { VirtualQuery(Some(addr as *const _), &mut mbi, size) } == size {
        if mbi.State == MEM_COMMIT && mbi.Type == MEM_PRIVATE {
            total += mbi.RegionSize;
            if mbi.RegionSize >= 128 << 10 {
                out.push_str(&format!(
                    "# region 0x{:x} base 0x{:x} size {} protect 0x{:x}\n",
                    mbi.BaseAddress as usize,
                    mbi.AllocationBase as usize,
                    mbi.RegionSize,
                    mbi.Protect.0
                ));
            }
        }
        let next = (mbi.BaseAddress as usize).saturating_add(mbi.RegionSize);
        if next <= addr {
            break;
        }
        addr = next;
    }
    out.push_str(&format!("# private_commit {total}\n"));
    out.push_str(&format!("# threads {}\n", thread_count()));
    out
}

/// Threads of this process (F-41: a worker that is still alive holds its stack).
fn thread_count() -> usize {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    let pid = std::process::id();
    // SAFETY: snapshot handle closed below; THREADENTRY32 sized before use.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else {
            return 0;
        };
        let mut e = THREADENTRY32 {
            dwSize: u32::try_from(std::mem::size_of::<THREADENTRY32>()).unwrap_or(0),
            ..Default::default()
        };
        let mut n = 0;
        let mut ok = Thread32First(snap, &mut e).is_ok();
        while ok {
            if e.th32OwnerProcessID == pid {
                n += 1;
            }
            ok = Thread32Next(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
        n
    }
}
