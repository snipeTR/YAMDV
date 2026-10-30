# ADR-0024: Dev dependency `lopdf` (=0.46.0)

- Status: accepted (Phase V2-I, prompt 019 HARD-01 audit; the crate was added in 018a without an ADR — S-SUP-02 gap, closed here)

Context/Decision: the PDF export tests parse the written file back (xref, objects, annotations, outline, document info) to prove S-EXP-03 / S-PRIV-06 on real bytes. `default-features = false` (no `chrono`, `rayon`, `time` features); its decryption and filter crates (aes, cbc, flate2, brotli-decompressor, …) come along but stay test-only.

Alternatives: the hand-written xref walker used by the `pdf_doc` fuzz target (enough for offsets, not for object graphs); `pdf` crate (larger, async features).

Licence: MIT (transitive crates are on the S-SUP-04 allowlist; `cargo deny check` green). Never linked into `yamdv.exe` (dev-dependency of `yamdv-export` only; `cargo tree -e normal -p yamdv-app` does not contain it).

Consequences: test build time only; no runtime or size impact.
