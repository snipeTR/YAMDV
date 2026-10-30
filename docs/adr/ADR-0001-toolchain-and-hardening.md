# ADR-0001: Toolchain 1.99.0 and build hardening flags

- Status: accepted (Phase 001)

## Context
The spec requires a pinned stable toolchain, edition 2024, the `x86_64-pc-windows-msvc` target and build hardening (S-PROC-05, S-TEST-03).

## Decision
- `rust-toolchain.toml` pins **1.99.0** (current stable when Phase 001 started), with rustfmt, clippy and the MSVC target. The workspace sets `rust-version = "1.99"`.
- Release profile: `opt-level = "s"`, `lto = "fat"`, `codegen-units = 1`, `panic = "abort"`, `overflow-checks = true`, `strip = "symbols"`. A `release-dbg` profile keeps symbols for profiling.
- `.cargo/config.toml` for the MSVC target:
  - `-C target-feature=+crt-static` (static CRT, no VC++ redistributable)
  - `-C control-flow-guard`
  - linker `/DYNAMICBASE /HIGHENTROPYVA /NXCOMPAT /CETCOMPAT /OPT:REF /OPT:ICF`
- `cargo xtask check-imports` verifies the result in CI: DllCharacteristics `DYNAMIC_BASE | HIGH_ENTROPY_VA | NX_COMPAT | GUARD_CF`, the CET-compat debug entry and no W+X sections.
- Cross-building from the Linux dev host uses `cargo-xwin` (lld-link). It accepted every flag above. Windows CI uses link.exe.
- **Deferred:** `ProcessDynamicCodePolicy` (S-PROC-04 SHOULD). DirectWrite and D2D behaviour under ProhibitDynamicCode must be verified on real hardware first, which is Phase 002 work.

## Alternatives
- Floating `stable`: not reproducible.
- `opt-level = "z"`: re-evaluate in Phase 013 with measurements.

## Consequences
- The static CRT adds roughly 100 KB.
- CFG adds a little code size.
- Cross-builds and native builds can differ slightly in size, because they use different linkers.

## Revisit
In Phase 013 (size tuning), or when a new stable release brings a needed feature.

## Amendment (Phase 002, F-06): ProcessDynamicCodePolicy

**Decision: on.** `hardening::apply` sets `ProcessDynamicCodePolicy { ProhibitDynamicCode }` (Arbitrary Code Guard, S-PROC-04) as its last step, after the image-load and strict-handle policies.

Verification on CI `windows-latest` (run 38932189793 and later; the release and dev-tools builds both apply the policy before anything else):

| Area | Result |
|---|---|
| DirectWrite text, font fallback (CJK, Arabic, Hebrew) and color emoji (`TranslateColorGlyphRun`) | works — all 32 goldens render, including `unicode-*` with emoji |
| WIC PNG **encode** (`--render-png`) | works |
| WIC **decode** | NOT VERIFIED — not used before Phase 003 (images); re-check then |
| `IFileOpenDialog` open + cancel (M6) | works, process exits cleanly |
| Dialog in a shell-extension-heavy folder | NOT VERIFIED on CI (no extensions installed) → owner kit step 2 |
| Drag & drop (`WM_DROPFILES`) | NOT VERIFIED on CI (no input injection) → owner kit step 2 |
| IME composition | NOT VERIFIED (no IME on the runner; the viewer has no text input yet) → owner kit step 2 |

Revisit if a user reports a shell extension failing to load in the Open dialog. Extensions that JIT (rare) cannot run under ACG, which is the intended protection.

## Amendment (Phase V2-I, prompt 019 §4): ACG matrix re-run

**Decision: unchanged — ACG stays on** in the UI process and in every helper except printing to a
real printer. New: the `--print-job` / `--export-pdf` worker turns ACG on late
(`hardening::acg_late`) when the target is the native PDF writer, because no printer driver loads
there. Evidence: CI run 41023143760 (PR #94) (`windows-latest`, dev-tools build, ACG active as in release).

| Area | Process / ACG | Result | Evidence |
|---|---|---|---|
| SVG images (native renderer, ADR-0021) | UI, ACG on | works | `acg-matrix` row `svg` (new) |
| ICO (PNG + BMP frames), GIF, PNG, JPEG, BMP, TIFF via WIC decode | UI, ACG on | works (closes the 002 "WIC decode NOT VERIFIED" row) | `acg-matrix` rows |
| WebP (no system codec on the runner) | UI, ACG on | placeholder, as designed (ADR-0022) | `acg-matrix` rows `webp-*` |
| DirectWrite font streams + subsetting in the native PDF | print-job worker, **ACG on (new, late)** | works | driver `pdf-native` + `pdf-native.ps1` (UI and headless) |
| Printing to a real printer | print-job worker, ACG **off** (ADR-0018: drivers run in-process and may JIT) | unchanged | owner QA item 11 |
| Interactive HTML export (CSP hash, inline script written, never run) | UI, ACG on | works | driver `html-interactive` |
| Jump-list helper (`--dialog-host jumplist`, P-53) | helper, ACG on — it only calls `ICustomDestinationList`/`IObjectCollection`/`IShellLinkW`/property store, no driver or extension code | works | driver `jumplist-icon` |
| Settings font picker (DirectWrite system font collection enumeration) | UI, ACG on | works (> 5 families listed) | driver `high-contrast` (`font_list > 5`) |
| UI Automation provider (`UIAutomationCore.dll`, lazy, System32) | UI, ACG on | works | driver `uia-tree` (`uia-client` exit 0) |
| Open dialog with shell extensions, drag & drop, IME | UI, ACG on | NOT VERIFIED on CI | owner QA items 6, 8, 9 |
