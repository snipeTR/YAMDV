# Security audit — Phase V2-I (HARD-01, prompt 019 §2.1)

One row per rule in `docs/SECURITY.md` (81 rules). Status: **OK** = enforced and evidenced ·
**fixed** = gap found and fixed in this phase · **gap** = open, tracked by an issue · **manual** =
release-QA step. Tests named `hard01_*` are the §2.3 matrices added in this phase
(`crates/yamdv-core/tests/v2i_security.rs`, `crates/yamdv-svg/tests/v2i_svg_rules.rs`,
`crates/yamdv-app/src/cli.rs`). Fuzz targets: `fuzz/fuzz_targets/` (18; long run in `fuzz-long.yml`).

| Rule | Where enforced | Test / evidence | Status |
|---|---|---|---|
| S-NET-01 | no net code; `deny.toml` bans | `cargo deny check`; check-imports (below) | OK |
| S-NET-02 | `core::image` source policy, `core::path` network forms; WIC from memory only | `images` unit tests, `hard01_custom_icon_path_matrix` | OK |
| S-NET-03 | `xtask check-imports` | CI job `check-imports` + output below | OK |
| S-NET-04 | `sys::shell_open_url` (validated URL), constant `ms-settings:` | `link` unit tests | OK |
| S-NET-05 | release QA | `OWNER_QA.md` item (firewall + netstat) | manual |
| S-PROC-01 | `hardening::apply` first in `main` (`SetDefaultDllDirectories(SYSTEM32)`) | `hardening` step log (driver `hardening`) | OK |
| S-PROC-02 | `hardening.rs` `SetDllDirectoryW("")`, `SetSearchPathMode(SAFE|PERMANENT)` | same | OK |
| S-PROC-03 | `hardening.rs` `HeapSetInformation` | same | OK |
| S-PROC-04 | `hardening.rs` image-load + strict-handle; ACG per ADR-0001; V2-I: `acg_late()` turns ACG on in the native-PDF worker | ADR-0001 ACG matrix (§4 re-run) | OK |
| S-PROC-05 | `.cargo/config.toml` (/CETCOMPAT, CFG, static CRT); `Cargo.toml` `panic="abort"`, `overflow-checks` | `xtask` PE check (below) | OK |
| S-PROC-06 | manifest `asInvoker` (`build_support/res.rs`); `sys::is_elevated` → banner | driver `banners` | OK |
| S-PROC-07 | `cli.rs` internal flags; `job.rs`, `dialog_host.rs`, `jumplist.rs` (own exe by absolute path) | `hard01_jumplist_host_argv_matrix`, dialog-host tests | OK |
| S-PROC-08 | known folders for logs/crash; config/install/data/recovery/editor-detection folders via `known.rs` (`SHGetKnownFolderPath`), env override only in `dev-tools` builds (test isolation); test `hard05_release_ignores_environment` | V2-J (prompt 020 §1.4) | **fixed (HARD-05 #93)** |
| S-FILE-01 | `edit.rs` save paths; read mode never saves | `buffer` tests, driver `read-only` | OK |
| S-FILE-02 | `images.rs`, `files.rs` (names/attributes only) | `files` tests | OK |
| S-FILE-03 | `shell::atomic_write` (`CREATE_NEW`, flush, `ReplaceFileW`/`MoveFileExW`) | `atomic` tests, driver `save` | OK |
| S-FILE-04 | `core::path::validate` (+ `is_network_form`, `icon_candidate`) | path matrix tests, `hard01_custom_icon_path_matrix` | OK |
| S-FILE-05 | `core::paste::assets` (sanitize, inside-doc check via final path) | assets sandbox tests | OK |
| S-FILE-06 | print job file and uninstall helper | V2-I: unguessable names + `CREATE_NEW`. V2-J (P-55): both in `<LocalAppData>\YAMDV\tmp\<pid>-<nonce>\` (`known::Staging`: create-new folder, removed on drop and at exit; the uninstall helper's folder outlives the process); test `hard05_staging_is_private_and_removed` | **fixed** |
| S-FILE-07 | `core::ini` limits; `app::load_ini` | `hard01_corrupt_files_are_detected`; corrupt `settings/state/custom-theme.ini` now renamed `*.corrupt-<ts>` (was: silently defaulted) | **fixed** |
| S-FILE-08 | `watch.rs` (non-recursive), `files::MAX_WATCHED = 32` | `watch`/`files` tests, driver `files-watch` | OK |
| S-FILE-09 | `link.rs` (`.lnk`/`.url` inert) | `link` tests (`y.lnk`) | OK |
| S-FILE-10 | `buffer.rs`, `encode.rs` (byte-exact round trip) | encode/buffer property tests | OK |
| S-RES-01 | `core::image` source classification | image source tests | OK |
| S-RES-02 | `core::path` network forms + `GetDriveTypeW == DRIVE_REMOTE` before open | path tests; custom icon now also checks `DRIVE_REMOTE` | **fixed** (custom icon) |
| S-RES-03 | offline placeholder in `images.rs` | image tests | OK |
| S-RES-04 | `core::image` `MAX_FILE/MAX_DATA/MAX_DIM/MAX_PIXELS`, `gif::MAX_FRAMES` | header-before-decode tests; SVG raster now 16 384² / 64 MP (was 100 MP) | **fixed** (SVG) |
| S-RES-05 | exports embed decoded images only (`export::html`, `pdf.rs`) | `tests/xss.rs`, export tests | OK |
| S-RES-06 | DirectWrite system collection only | design; no font loading API | OK |
| S-RES-07 | `yamdv-svg` (`xml.rs` DTD reject, `render.rs` limits) | `hard01_svg_*` (inert, events, DTD, depth, count, size, dims, use-bomb/cycle) | **fixed** (pixel limit) |
| S-LINK-01 | `core::link` scheme allowlist | link matrix tests | OK |
| S-LINK-02 | `core::link::MAX_URL = 2048`, re-serialize, punycode | link tests | OK |
| S-LINK-03 | `core::link` extension allowlist + confirmation | link matrix | OK |
| S-LINK-04 | `window.rs` Zone.Identifier (ZoneId ≥ 3) → banner + Ask | driver `motw` | OK |
| S-HTML-01 | `core::html` tokenizer, `DROP` list | sanitizer tests, `hard01_paste_xss_matrix_yields_no_script` | OK |
| S-HTML-02 | `core::html` attribute/style allowlist | sanitizer tests | OK |
| S-HTML-03 | `html::MAX_DEPTH/MAX_TAGS/MAX_ATTR` | limit tests; fuzz `html` | OK |
| S-EXEC-01 | no scripting; HTML export script is ours (CSP hash) | design; `tests/xss.rs` | OK |
| S-EXEC-02 | `core::link` S-LINK-03 path only | link matrix | OK |
| S-EXEC-03 | `core::cmdline::{quote_arg, forbidden_target}`, `shell::spawn` (`CreateProcessW`) | `hard01_editor_argument_quoting_matrix` | OK |
| S-EXEC-04 | `sys.rs` `ShellExecuteExW` (no `NOZONECHECKS`) | grep + link tests | OK |
| S-IPC-01 | `shell.rs` mutex `Local\YAMDV-<sid hash>`; `core::ipc` ≤ 64 KB | `hard01_ipc_records_matrix`; fuzz `ipc`, `ipc_records` | OK |
| S-IPC-02 | `dialog_host::same_user_pid` (`EqualSid`) | dialog-host tests | OK |
| S-IPC-03 | forwarded paths → `path::validate`; open only | ipc tests | OK |
| S-IPC-04 | no pipes/sockets/sections/COM servers; V2-I UIA providers are in-process objects handed to UIAutomationCore only on `WM_GETOBJECT(UiaRootObjectId)` (no class registration, read-only, THREAT_MODEL §13) | check-imports (no UIAutomationCore import); driver `uia-tree` | OK |
| S-IPC-05 | `dialog_host` magic + nonce + PID (`GetWindowThreadProcessId`) | ipc/dialog-host tests | OK |
| S-REG-01 | `core::reg` plan (single list) | reg tests | OK |
| S-REG-02 | `core::reg` + `shell::remove` | `--register/--unregister` driver, v1d-checks | OK |
| S-REG-03 | explicit actions only (`cli.rs`, settings) | driver `register` | OK |
| S-REG-04 | theme/HC, App Paths, editor keys, assoc queries | code review (`reg.rs`, `editors.rs`) | OK |
| S-PARSE-01 | depth limits (`doc::MAX_DEPTH = 64`, …) | 18 fuzz targets; `journal` decode now rejects CR in header (fuzz finding) | OK (+1 fuzz fix) |
| S-PARSE-02 | `decode::HARD/SOFT_LIMIT`, `table::MAX_COLS/MAX_CELLS`, index limits | limit tests | OK |
| S-PARSE-03 | `diagram::BUDGET = 200 ms`, block budget placeholder | layout tests, T5 | OK |
| S-PARSE-04 | `regex-lite` (linear) + time guard in `replace.rs` | fuzz `regex_template` | OK |
| S-PARSE-05 | `yamdv-math` `MAX_CHARS/MAX_STEPS/MAX_TOKENS`, `rich::MAX_FORMULAS` | math tests; fuzz `math` | OK |
| S-PARSE-06 | `mermaid::MAX_SRC`, `diagram::SOFT/HARD_*`, `rich::MAX_DIAGRAMS` | mermaid tests; fuzz `mermaid` | OK |
| S-PARSE-07 | checked/saturating arithmetic; `overflow-checks = true` | clippy `indexing_slicing` (workspace lints); 18 fuzz targets | OK |
| S-PARSE-08 | `decode.rs` lossy → U+FFFD, edit disabled | decode tests | OK |
| S-PRIV-01 | no telemetry code | check-imports; design | OK |
| S-PRIV-02 | `log.rs` off by default, local; **rotation ≤ 1 MiB × 3** now applied by the sink | `core::log` tests (model) | **fixed** (sink did not rotate) |
| S-PRIV-03 | `crash.rs` (no document text) | crash tests | OK |
| S-PRIV-04 | settings panel clear history/favorites/positions | driver `settings` | OK |
| S-PRIV-05 | clipboard read only in `paste.rs` / `sys::paste_text` on Ctrl+V | code review (3 call sites) | OK |
| S-PRIV-06 | `pdf.rs` document info (Title, Producer "YAMDV <ver>") | `lopdf` parse-back test | OK |
| S-SUP-01 | `Cargo.lock`, `cargo deny`, `--locked` | CI `deny` job | OK |
| S-SUP-02 | ADR per dependency | `lopdf` (dev, 018a) had none → **ADR-0024** | **fixed** |
| S-SUP-03 | `deny.toml` bans | `cargo deny check` | OK |
| S-SUP-04 | `deny.toml` licences | `cargo deny check` | OK |
| S-SUP-05 | `build_support/res.rs` pure-Rust `.res` | build scripts reviewed | OK |
| S-SUP-06 | workflows pinned by SHA, `contents: read` | `ci.yml`, `fuzz-long.yml` | OK |
| S-EXP-01 | atomic export write to the chosen path | export tests | OK |
| S-EXP-02 | `export::html` CSP + `check.rs` | `tests/xss.rs` (81 vectors) | OK |
| S-EXP-03 | `pdfparts::safe_uri`, no JS/launch/forms | pdf tests; fuzz `pdf_doc` | OK |
| S-EXP-04 | `export::csv` guard | csv tests; fuzz `tsv_csv` | OK |
| S-UI-01 | status tooltip + Ask popover | driver `links` | OK |
| S-UI-02 | `popover.rs` default focus Cancel | popover tests | OK |
| S-UI-03 | banners: MotW, elevated, decoding | driver `banners` | OK |
| S-TEST-01 | `fuzz/` 18 targets, `fuzz-long.yml` 1 h each | fuzz-long run 41016337092: 15.0 h, 1.98 G execs, 4 findings (below) | OK |
| S-TEST-02 | security matrices | `hard01_*` + existing matrices | OK |
| S-TEST-03 | CI `check-imports` | output below | OK |
| S-TEST-04 | `docs/THREAT_MODEL.md` | STRIDE per v2 entry point (this phase) | OK |

**Summary:** 81 rules · **72 OK** · **7 fixed** in this phase (S-FILE-06, S-FILE-07, S-RES-02,
S-RES-04, S-RES-07, S-PRIV-02, S-SUP-02) · **1 open gap** (S-PROC-08 → HARD-05 #93, fixed in V2-J) · **1 manual**
(S-NET-05). Pre-run fuzz findings (local stable mini-fuzz of the 10 new targets), both fixed with
regression tests and seeds: (1) `journal::decode` accepted a CR inside a header value (no round
trip); (2) paste HTML→MD wrapped block content (`<pre>`) in inline markers or link brackets
(`~~```…```~~`, `[```…```](u)`), so the "code" became live Markdown text, including a raw
`<script` tag (S-HTML-01 defence in depth; the viewer and exports still drop it).

**fuzz-long (run 41016337092, `ubuntu-latest`, libFuzzer + ASan, 1 h budget per target):** 14
targets ran 1 h clean; 4 stopped early on a finding, all handled:
(1) `svg_parse` (46 s): tiny-skia 0.12 index panic on a path ~1.7e18 px → device-bounds guard
(paths beyond 30 000 px are skipped), test `hard02_device_bounds_guard`, seed;
(2) `html_to_md` (177 s): a backtick run inside `<code>` closed the span early, so `<script` became
live inline HTML → fence longer than the longest inner run, test + seed;
(3) `ini` (1 354 s): a key starting with `[` was written back as `[k = v]` and re-read as a section
→ such lines are malformed, tests `hard02_bracket_key_does_not_become_a_section`,
`hard02_ini_round_trip_seeds`;
(4) `cf_html` (2 119 s, 8.06 M execs): libFuzzer RSS limit (2 560 MB) on the empty input — harness
growth, not a product leak: natively 30 × 4 485 corpus inputs keep RSS flat at 4.8 MiB (VmHWM
4.8 MiB). Corpus samples (3.4 MB) are committed under `fuzz/seeds/*/cmin-*`.

## Binary checks (S-NET-03, S-TEST-03)

`cargo xtask check-imports` on the release `yamdv.exe` (local xwin build of this branch; the
MSVC CI output of the PR run is in the job log `check-imports`):

```
20 DLLs: gdi32, user32, KERNEL32 (148 fn), bcrypt, combase, ole32, dwrite, shell32, dwmapi,
advapi32, api-ms-win-shcore-scaling-l1-1-1, oleaut32, psapi, normaliz, imm32, comdlg32,
bcryptprimitives, propsys, ntdll, api-ms-win-core-synch-l1-2-0
DllCharacteristics = 0xc160 (HIGH_ENTROPY_VA | DYNAMIC_BASE | NX_COMPAT | GUARD_CF |
TERMINAL_SERVER_AWARE), CET_COMPAT = true, W+X sections = []
check-imports: OK (20 DLLs, no forbidden imports, hardening flags present)
```
