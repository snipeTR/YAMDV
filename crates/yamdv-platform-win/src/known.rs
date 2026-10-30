// SPDX-License-Identifier: AGPL-3.0-or-later
//! Known folders and temp staging (HARD-05 #93, S-PROC-08, S-FILE-06 / P-55).
//!
//! Release builds resolve `%APPDATA%`, `%LOCALAPPDATA%` and the Program Files folders only with
//! `SHGetKnownFolderPath`, never from the process environment. `dev-tools` builds (CI and the
//! owner test pack only, never shipped) still honour `APPDATA` / `LOCALAPPDATA` so the test
//! scripts can isolate each run in a scratch profile.
//!
//! Temp and staging files live in `<LocalAppData>\YAMDV\tmp\<random>\`: the folder is created
//! with create-new semantics (an existing or planted folder is an error), removed when its
//! [`Staging`] guard drops (success and failure) and, as a last resort, at exit ([`cleanup`]).
#![allow(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
    FOLDERID_RoamingAppData, KF_FLAG_DEFAULT, SHGetKnownFolderPath,
};
use windows::core::GUID;

fn known(id: &GUID) -> String {
    // SAFETY: `id` is a static FOLDERID GUID; the returned buffer is freed below.
    let Ok(p) = (unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None) }) else {
        return String::new();
    };
    // SAFETY: p is a valid NUL-terminated string allocated by the shell.
    let s = unsafe { p.to_string() }.unwrap_or_default();
    // SAFETY: p was allocated with CoTaskMemAlloc by SHGetKnownFolderPath.
    unsafe { CoTaskMemFree(Some(p.0.cast_const().cast())) };
    s
}

/// Test-isolation override: only in `dev-tools` builds (never shipped, S-PROC-08).
fn dev_override(var: &str) -> Option<String> {
    if cfg!(feature = "dev-tools") {
        std::env::var(var).ok().filter(|v| !v.is_empty())
    } else {
        let _ = var;
        None
    }
}

/// `%LOCALAPPDATA%` (FOLDERID_LocalAppData); empty if unavailable.
pub fn local_app_data() -> String {
    dev_override("LOCALAPPDATA").unwrap_or_else(|| known(&FOLDERID_LocalAppData))
}

/// `%APPDATA%` (FOLDERID_RoamingAppData); empty if unavailable.
pub fn roaming_app_data() -> String {
    dev_override("APPDATA").unwrap_or_else(|| known(&FOLDERID_RoamingAppData))
}

/// `Program Files` (FOLDERID_ProgramFiles).
pub fn program_files() -> String {
    known(&FOLDERID_ProgramFiles)
}

/// `Program Files (x86)` (FOLDERID_ProgramFilesX86).
pub fn program_files_x86() -> String {
    known(&FOLDERID_ProgramFilesX86)
}

/// `<LocalAppData>\YAMDV\tmp` (P-55).
pub fn tmp_root() -> Option<PathBuf> {
    let l = local_app_data();
    (!l.is_empty()).then(|| Path::new(&l).join("YAMDV").join("tmp"))
}

static LIVE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// A private staging folder `<LocalAppData>\YAMDV\tmp\<random>\`, removed on drop.
#[derive(Debug)]
pub struct Staging {
    dir: PathBuf,
}

impl Staging {
    /// Creates a new folder with an unguessable name; never reuses an existing one.
    pub fn new() -> Option<Staging> {
        let root = tmp_root()?;
        std::fs::create_dir_all(&root).ok()?;
        for _ in 0..4 {
            let dir = root.join(format!(
                "{}-{:016x}",
                std::process::id(),
                crate::dialog_host::nonce()
            ));
            // create_dir fails with AlreadyExists for an existing folder or link (CREATE_NEW).
            if std::fs::create_dir(&dir).is_ok() {
                if let Ok(mut l) = LIVE.lock() {
                    l.push(dir.clone());
                }
                return Some(Staging { dir });
            }
        }
        None
    }

    /// The folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// A path inside the folder.
    pub fn join(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Keeps the folder after this guard (a detached helper owns it); still removed at exit
    /// unless the process ends first.
    pub fn keep(self) -> PathBuf {
        let d = self.dir.clone();
        if let Ok(mut l) = LIVE.lock() {
            l.retain(|p| p != &d);
        }
        std::mem::forget(self);
        d
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        if let Ok(mut l) = LIVE.lock() {
            l.retain(|p| p != &self.dir);
        }
    }
}

/// Removes every staging folder this process still owns (called at exit).
pub fn cleanup() {
    let dirs = LIVE
        .lock()
        .map(|mut l| std::mem::take(&mut *l))
        .unwrap_or_default();
    for d in dirs {
        let _ = std::fs::remove_dir_all(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hard05_release_ignores_environment() {
        let real = known(&FOLDERID_LocalAppData);
        assert!(!real.is_empty());
        // Only the dev-tools build honours the variables (test isolation).
        if cfg!(feature = "dev-tools") {
            return;
        }
        // SAFETY: test-only; nothing else in this test binary reads LOCALAPPDATA concurrently
        // with a different expectation.
        unsafe { std::env::set_var("LOCALAPPDATA", r"X:\planted") };
        assert_eq!(local_app_data(), real);
        assert!(tmp_root().is_some_and(|p| p.starts_with(&real)));
    }

    #[test]
    fn hard05_staging_is_private_and_removed() {
        let s = Staging::new().expect("staging");
        let d = s.dir().to_path_buf();
        assert!(d.is_dir());
        assert!(d.parent().is_some_and(|p| p.ends_with(r"YAMDV\tmp")));
        std::fs::write(s.join("x.job"), b"x").expect("write");
        drop(s);
        assert!(!d.exists());
        let s2 = Staging::new().expect("staging");
        let d2 = s2.dir().to_path_buf();
        cleanup();
        assert!(!d2.exists());
        drop(s2);
    }
}
