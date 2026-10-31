# 022_REVIEW_FIXES — YAMDV: fixes from an external code review (verified by the reviewer)

> **Prompt 022.** Repository `github.com/snipeTR/YAMDV` (v2.1.1).
> - Branch `fix/review-2`
> - Report `docs/reports/review2_REPORT.md` (≤ 40 lines)
> - Store this file as received in `docs/prompts/`
>
> **Precedence:** `docs/PROCESS.md` (A-1…A-12) > this prompt > `docs/*`.
> **A-12 still applies:** CI runs no tests. Add the small unit tests named below in the pure crates and run them **locally** (`cargo test -p <crate>`); do not add driver scripts or goldens.

The reviewer has already checked every item below against the code at 531f205; file and function names are given. Fix them in the order below.

---

## 1. Security

**S1. Opening a document has no network / reparse check.**
- **Finding:**
  - `open_inner` (`yamdv-platform-win/src/window.rs`) calls only `path::validate` after `full_path`.
  - `validate` accepts UNC paths and does not look at reparse points.
  - A relative `.md` link (or a wikilink) that resolves through a junction or symlink to `\\server\share` makes `CreateFile` touch the network, which can leak NTLM credentials.
- **Fix:**
  - Move the S-RES-02 check from `image.rs` (`resolve_policy`: `is_network_form`, `GetDriveTypeW == DRIVE_REMOTE`, `reparse_target` → network) into **one shared function** in `yamdv-core::path`, e.g. `local_policy(path, fs)`.
  - Use it for **every** filesystem entry: document open, link targets, wikilink candidates, the Files tab, drops, images, the custom icon.
  - **Exception (spec):** a document the user opened himself (Open dialog, CLI, Explorer, drop) may be on a network share. Resources and links inside it are allowed only on the **same server and share**.
  - **Blocked:** links and wikilinks that lead to any other network location are blocked, with the existing "blocked network path" message.
- **Unit tests:** UNC, `\\?\UNC\`, WebDAV, a mapped drive, a junction → UNC, and the same-share exception.

**S2. `find_wiki` candidates are not checked.**
- **Finding:** in `find_wiki` (`sys.rs`), the directory walk already skips junctions (`is_dir() && !is_symlink()` — correct). But the **candidate file itself** is tested with `is_file()`, which follows reparse points, and `name` may contain `..`.
- **Fix:**
  - Reject names that contain `..`, `:` or a leading `\`.
  - Run each candidate through the S1 function before `is_file()`.

**S3. Device names are not checked the same way everywhere.**
- **Finding:**
  - `path::is_reserved` lacks `CONIN$` / `CONOUT$`.
  - `image::is_device_name` lacks the superscript `COM¹²³` / `LPT¹²³`.
- **Fix:** one function in `path.rs` used by both callers: CON, PRN, AUX, NUL, CONIN$, CONOUT$, COM1–9, LPT1–9, plus the superscript forms, with any extension. Add a unit test.

**S4. Credential URLs open without a prompt in `direct` mode.**
- **Finding:** in `link_click`, the credentials note is shown only inside the Ask popover. `must_ask` (`link.rs`) ignores `userinfo`, so with `general.open_external_links = direct`, `https://user:pass@host` opens silently.
- **Fix:**
  - `must_ask` also returns true for userinfo, IDN/punycode hosts and non-default ports. These always ask.
  - If `ShellExecuteExW` fails, show a toast "Could not open link" instead of failing silently.

**S5. The HTML sanitizer is a denylist.**
- **Finding:** `html::url_allowed` lets everything through except javascript, vbscript, livescript and SVG `data:` (for example `file:` and `ms-msdt:` pass). A second gate (`link::classify`) saves it today.
- **Fix:** turn it into an **allowlist**:
  - `href`: `http`, `https`, `mailto`, `#fragment`, and relative paths with no scheme;
  - `src`: relative or local paths, plus `data:image/(png|jpeg|gif|bmp|webp|x-icon)`.

  Everything else is removed. Add a unit test with `file:`, `ms-msdt:`, `search-ms:` and `\\host`.

**S6. PDF URIs keep credentials.**
- **Finding:** `pdfparts::safe_uri` accepts `https://user:pass@host`.
- **Fix:** reject URIs with userinfo (or strip it), as the HTML export does. Add a unit test.

**S7. The custom editor denylist is short** (`cmdline::forbidden_target`).
- **Fix:**
  - Add `rundll32`, `regsvr32`, `reg`, `msiexec`, `wsl`, `bash`, `hh`, `explorer`, `certutil`, `bitsadmin`, `schtasks`, `forfiles`, `msbuild`, `installutil` and `regasm` (`.exe`).
  - **First use:** the first time F4 uses a custom editor (or after its path changes), show a confirm with the full command line; store the approved path + hash.
- **Note:** this is defense in depth; the threat is a modified portable `settings.ini`.

**S8. CSV guard and leading whitespace** (`csv::field`). Guard when the first **non-whitespace** character is `= + - @`, as well as when the cell starts with tab or CR. Add a unit test.

## 2. Bugs

**B1. Interactive HTML find breaks on `İ` / `ß`.**
- **Finding:** `assets/interactive.js` searches in `n.data.toLowerCase()` but splits the original node at that index. `"İ".toLowerCase()` is 2 UTF-16 units, so the index shifts: the wrong text is highlighted, or `splitText` throws.
- **Fix:** build a per-node index map (original offset ↔ folded offset) or fold character by character (`ch.toLowerCase()` per code point, keeping a map). Also add Turkish equivalence: `i/İ/ı/I` match each other, as the in-app find bar does.
- **After the change:** the CSP hash must be recomputed (it is computed at build time; check it).

**B2. Wikilinks in PDF become anchors** (`pdf.rs` ≈ 880).
- **Finding:** `[[Page]]` is resolved with `anchor_block` inside the same document, or dropped.
- **Fix:** a wikilink is an anchor only when it is `[[#Heading]]` or when it matches a heading of the current document; otherwise draw it as plain text (no link annotation), the same as HTML export.

**B3. Mermaid visual label edit is lossy** (`yamdv-mermaid/src/edit.rs`, `encode_label`).
- **Finding:** `"` becomes `'`, and newlines become `<br>`. A shared operator (`A & B --> C`) shares one label.
- **Fix:**
  - **Quotes:** encode `"` as `#quot;` (Mermaid entity), so a decode → encode round trip is exact.
  - **Shared operators:** editing the label of one edge of a shared operator first splits that edge into its own statement.
  - **Undo:** "add node + rename" becomes one undo step.
  - **Unit test:** round trip.

## 3. Docs (by script)
- `docs/FUTURE.md`: regenerate from the open `post-v2` issues; it still shows the 2026-10-17 v1 list.
- `docs/TRACEABILITY.md`: set the rows that are now DONE to DONE (R-MMD-08 today marker, R-EXP-06, R-IMG-02, R-THEME-08, …). Add a check to `xtask spec-check` that warns about any `deferred (FUT-xx)` row whose FUT issue is closed.
- `tools/backlog/issues.jsonl`: mark it as historical (header line); it is not the live list.

## 4. Maintenance (small, no behavior change)
- **Lint scope:** in `window.rs`, narrow the file-wide `#![allow(clippy::indexing_slicing, …)]` to the functions that need it, adding `// SAFETY/bounds:` comments where the arena guarantees the index. Do the same for `html.rs`, `link.rs`, the highlighter and export, where reasonable. If it is too large, do only `window.rs` and open an issue for the rest.
- **Fuzz corpus:** `fuzz/` has 14 050 files (57 MB). Minimize the corpora (`cargo fuzz cmin`) and keep ≤ 5 MB in the repo, per the V2-I rule.

## 5. Release and report
- **Release:** after the build is green and the PR is merged, release **v2.1.2**: version, CHANGELOG, `chore(release)` commit, tag, draft.
- **Report (≤ 40 lines):** one line per item S1–S8, B1–B3 and §3–§4 (done / skipped + reason), the local unit-test result, and the draft link + SHA-256.

**END OF PROMPT 022.** Start with §1 S1 now.
