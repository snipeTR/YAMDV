// SPDX-License-Identifier: AGPL-3.0-or-later
//! IPC payloads (S-IPC-01/05, HARD-02): dialog-host / forward replies and worker progress
//! records. Strict decoders: no panic; anything accepted re-encodes byte-identically.
#![no_main]
use libfuzzer_sys::fuzz_target;
use yamdv_core::{ipc, progress};

fuzz_target!(|data: &[u8]| {
    if let Ok(r) = ipc::decode(data) {
        if let ipc::Outcome::Paths(p) = &r.outcome {
            assert!(p.len() <= ipc::MAX_PATHS);
            assert!(p.iter().all(|s| s.encode_utf16().count() <= ipc::MAX_PATH_UNITS));
        }
        assert_eq!(ipc::decode(&ipc::encode(&r)), Ok(r));
    }
    if let Some(rec) = progress::decode(data) {
        assert!(rec.total == 0 || rec.n <= rec.total);
        assert_eq!(&progress::encode(&rec)[..], data);
    }
});
