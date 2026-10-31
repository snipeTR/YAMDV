# Review round 2 (prompt 022) — report

PR https://github.com/snipeTR/YAMDV/pull/2 (CI build job green, run 41214427076), squash-merged as
b943420; release commit cf993c8 `chore(release): v2.1.2`, tag v2.1.2 (release run 41215270804 green).

| Item | Status |
|---|---|
| S1 | done: `path::local_policy` (Scope User/Doc/Local) before any FS access for drops, open, links, wikilinks, images, recent files, app icon; UNC/device/mapped/junction→UNC blocked |
| S2 | done: `wiki_name_ok`, every candidate policy-checked, no symlink-dir walk, network toast |
| S3 | done: one `is_reserved` check (adds CONIN$/CONOUT$) |
| S4 | done: userinfo / IDN / non-default port always ask; `LinkOpenFailed` toast |
| S5 | done: href/src scheme allowlist; `sanitize_paste` keeps remote img for HTML→Markdown |
| S6 | done: PDF `safe_uri` rejects `@` / `\` in the authority |
| S7 | done: 15 LOLBins forbidden; custom editor confirmed once per path+args+FNV file fingerprint |
| S8 | done: CSV formula guard after leading space / tab / CR |
| B1 | done: per-code-point fold + offset map (İ/ı/I → i); CSP hash recomputed (sha256-ZQoiIny/…) |
| B2 | done: PDF wikilinks link to heading anchors only |
| B3 | done: exact label round-trip (#quot; #35; #lt;), shared-operator edge split, add+rename = 1 undo |
| §3 | done except FUTURE.md: 5 TRACEABILITY rows → implemented; spec-check warns on deferred rows with a closed FUT issue; issues.jsonl historical header; `xtask future-md` added. FUTURE.md **skipped** (owner: leave it for now; snipeTR/YAMDV has no post-v2 issues) |
| §4 | done: `indexing_slicing` per function with SAFETY/bounds notes (window.rs, core html/link, highlighter, export), so no follow-up issue; `cargo fuzz cmin` on 18 targets: −488 files, 13 541 files / 3.3 MB content (57 MB was du 4 KiB blocks); mermaid fuzz target fixed |

Local tests (`cargo test -p`): yamdv-core 181 + integration, yamdv-export 48, yamdv-mermaid 35,
yamdv-highlight 21, yamdv-svg, xtask 32 — 0 failed. fmt, xwin clippy -D warnings, cargo deny,
check-imports, spec-check (exit 0): OK.

Exe size: PR CI 3 603 968 B, release zip exe 3 603 456 B — over the 3 600 000 B B-SIZE soft budget
(+8 192 B vs v2.1.1: .text +6 592, .rdata +1 436). **Owner decision 2026-10-31: ship v2.1.2 over the
budget; the limit stays 3 600 000 B**; trim tracked in https://github.com/snipeTR/YAMDV/issues/3.
The B-SIZE gate runs only in the workflow_dispatch `windows` job, so PR/release CI is unaffected.

Draft release (not published): https://github.com/snipeTR/YAMDV/releases/tag/untagged-d0acd24f4b4cf68e3418
YAMDV-2.1.2-win-x64.zip: 1 872 131 B, SHA-256 634f095a05674acc1d9e89c09e092e7633a4cf79c609c6d096f6895232ce8e32
