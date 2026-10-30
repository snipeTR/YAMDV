# Threat model (S-TEST-04) — completed in Phase V2-I (HARD-01, prompt 019 §2.2)

STRIDE per entry point (Spoofing, Tampering, Repudiation, Information disclosure, Denial of
service, Elevation of privilege). Mitigations name rules from `docs/SECURITY.md`; evidence is the
row in `docs/SECURITY_AUDIT.md`. Status **active** = implemented and tested; **gap** = open issue.

**Assets:** user documents and files next to them; settings/state/recovery data; NTLM credentials
(network-path leaks); the user's session integrity (no code execution, no network use); viewer
availability. **Attacker:** the author of any document, file name, folder, dropped item, clipboard
content or command line the user hands to YAMDV; another process of the same user is
semi-trusted (it can already act as the user), other users and lower-integrity processes are not.
**Trust boundary:** everything entering the process. Repudiation is not applicable to a
single-user offline viewer (no accounts, no transactions); it is marked "—" below.

## 1. Document content — Markdown, HTML subset, math, Mermaid
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | Link text hides its target; homograph host | S-LINK-02 (punycode), S-UI-01 Ask popover | active |
| T | Links/paths that write or launch | S-FILE-01, S-LINK-03, S-EXEC-02 | active |
| I | Remote images/fonts reveal the open; UNC paths leak NTLM | S-NET-02, S-RES-02/03, S-RES-06 | active |
| D | Parser/layout/regex/math/Mermaid blow-ups | S-PARSE-01…08, S-HTML-03, fuzz (18 targets) | active |
| E | Script/HTML execution | S-HTML-01/02 (no engine), S-EXEC-01 | active |

## 2. Document content — SVG and raster images (PNG/JPEG/GIF/ICO/BMP/TIFF/WebP)
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | — | — | — |
| T | External references in SVG (`href`, `url()`, `@import`) | S-RES-07 (same-document `#id` only) | active |
| I | `![](C:/…/id_rsa)` exfiltrated through exports; UNC image paths | S-RES-05 (decoded images only), S-RES-02 | active |
| D | Decompression/pixel bombs, `use` bombs/cycles, deep nesting, many GIF frames, malformed ICO directories | S-RES-04 (header checks before decode; SVG raster ≤ 16 384² and 64 MP — fixed V2-I), S-RES-07 limits, `gif::MAX_FRAMES`; fuzz `svg_parse`, `ico_dir` | active |
| E | Codec exploits (WIC in-proc) | decode in a worker, ACG + image-load policy (S-PROC-04, ADR-0015/0022) | active |

## 3. Dropped files and folders; Files tab
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | `.lnk`/`.url` or reparse points disguising the target | S-FILE-09 (never followed), S-FILE-04 (final path) | active |
| T | Writes into the dropped folder | read-only listing (S-FILE-02) | active |
| I | UNC / WebDAV / mapped network drive enumeration (NTLM) | S-FILE-04, S-RES-02 (`DRIVE_REMOTE`) | active |
| D | Huge folders, device files, watcher storms | `files::MAX_ENTRIES = 5000`, `MAX_WATCHED = 32` non-recursive (S-FILE-08), device names rejected | active |
| E | Drop from a lower-integrity process | UIPI (no `ChangeWindowMessageFilter` widening) | active |

## 4. Clipboard — HTML→MD, CF_HTML, bitmaps, TSV/CSV
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | — | — | — |
| T | Malicious HTML becoming live Markdown/HTML | html2md allowlist; `hard01_paste_xss_matrix` (41 vectors); fuzz `html_to_md`, `cf_html` | active |
| I | Clipboard read without user intent | S-PRIV-05 (read on paste only) | active |
| D | Huge CF_HTML offsets, DIB sizes, TSV with 10⁶ cells | `MAX_HTML`, `MAX_DIB`, `table::MAX_CELLS`; `hard01_cf_html_offsets_matrix`; fuzz `tsv_csv`, `table_model` | active |
| E | Pasted image written outside the assets folder | S-FILE-05 sandbox | active |

## 5. Command line
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | Internal flags (`--dialog-host <kind> <pid> <nonce>`, `--uninstall-helper <pid>`) invoked by a third party | strict argv grammar (`hard01_jumplist_host_argv_matrix`); results accepted only by the waiting owner (S-IPC-05); the helper recomputes its folder and checks its suffix | active |
| T | `-o` export over arbitrary files | explicit user CLI action; atomic write (S-EXP-01) | active |
| D | Thousands of paths | `ipc::MAX_PATHS = 1000` | active |
| E | `--register/--install` via a launch | S-REG-03 (explicit action only) | active |

## 6. IPC — single-instance forward, dialog host, print/PDF worker, jump-list host
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | Foreign `WM_COPYDATA`; forged dialog/worker results | S-IPC-01 magic + version; S-IPC-02 same-user SID; S-IPC-05 nonce + expected PID | active |
| T | Truncated/overlong records, bad UTF-16 | `core::ipc` strict decode; `hard01_ipc_records_matrix`; fuzz `ipc`, `ipc_records` | active |
| I | Job file read by another user | `%TEMP%` (per-user ACL), `YAMDV-print-<pid>-<seq>-<nonce>.job`, `CREATE_NEW` (fixed V2-I) | active |
| D | Message floods | ≤ 64 KB payload, accepted only while waiting | active |
| E | IPC asking for export/register/settings | S-IPC-03 (open only), S-IPC-04 (no pipes/COM servers) | active |

## 7. Config and state files — settings.ini, state.ini, custom-theme.ini, recovery journal
| | Threat | Mitigations | Status |
|---|---|---|---|
| T | Injected keys or header lines | S-FILE-07 (unknown keys ignored, values clamped); journal header CR/LF neutralized (fuzz fix V2-I) | active |
| I | Paths in logs | S-PRIV-02 (off by default; rotation fixed V2-I) | active |
| D | Huge/corrupt files | ≤ 4 MB / 64 KB per line; corrupt files renamed `*.corrupt-<ts>` (fixed V2-I); fuzz `ini`, `journal` | active |
| E | Values that execute | nothing executed; editor path resolved per S-PROC-08 | active |
| — | Folder location from `%APPDATA%`/`%LOCALAPPDATA%` | S-PROC-08 | **gap → HARD-05 #93** |

## 8. Registry — registration, DefaultIcon / custom icon, App Paths reads
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | Custom icon on a share (NTLM) or non-ICO | `path::icon_candidate` + `DRIVE_REMOTE` check (fixed V2-I), ≤ 1 MB, ICO directory validation | active |
| T | Writes outside the listed keys | S-REG-01/02 single plan, idempotent | active |
| I | — | S-REG-04 reads limited | active |
| E | Registration on a normal launch | S-REG-03 | active |

## 9. External editor launch
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | `cmd.exe`/script targets, network or reparse exe | S-EXEC-03 `forbidden_target`, absolute existing `.exe` | active |
| T | Argument injection via file names | `quote_arg` round trip; `hard01_editor_argument_quoting_matrix` | active |
| E | `PATH`/env resolution | App Paths + known locations (S-PROC-08) | active |

## 10. Exports — PDF, HTML (interactive script + CSP), CSV, PNG
| | Threat | Mitigations | Status |
|---|---|---|---|
| T | Script in exported HTML | S-EXP-02 CSP hash + escaping; `tests/xss.rs` (81 vectors) | active |
| I | Personal metadata; local files embedded | S-PRIV-06, S-RES-05 | active |
| D | Spreadsheet formula injection | S-EXP-04 CSV guard | active |
| E | PDF JavaScript/launch/forms | S-EXP-03, `safe_uri`; fuzz `pdf_doc` (xref integrity) | active |

## 11. Regular expressions (find/replace)
| | Threat | Mitigations | Status |
|---|---|---|---|
| D | Catastrophic backtracking, huge compiled programs, match floods | S-PARSE-04 `regex-lite` (linear), `SIZE_LIMIT = 1 MiB`, `MAX_MATCHES = 100 000`, time guard; fuzz `regex_template` | active |
| T | Template expansion surprises (`$name`) | `expand_template` tests | active |

## 12. Process environment
| | Threat | Mitigations | Status |
|---|---|---|---|
| E | DLL planting next to a portable exe | S-PROC-01/02 | active |
| E | Code injection / dynamic code | S-PROC-04 (image-load policy, ACG per ADR-0001), S-PROC-05 (CFG, CET, no W+X) | active |
| E | Running elevated | S-PROC-06 banner | active |

## 13. UI Automation (V2-I §4) — `WM_GETOBJECT(UiaRootObjectId)` from any process on the desktop
| | Threat | Mitigations | Status |
|---|---|---|---|
| S | A process posing as an assistive client | UIA is a desktop-wide accessibility contract (same session, UIPI integrity rules); nothing to authenticate; all providers read-only | accepted |
| T | A client changing the document or settings | no Invoke/Value/Toggle/Selection patterns; TextPattern read-only (`IsReadOnly`, `Select` → `UIA_E_INVALIDOPERATION`); `SetFocus` is a no-op | active |
| R | — | no actions to repudiate | n/a |
| I | Document text readable by other processes | the same text is on screen; reading text only (no file paths beyond window title); a screen reader needs it | accepted |
| D | Call floods, huge documents | per-call snapshot, no cache; text snapshot shared by a range and its clones, freed on release; providers return `UIA_E_ELEMENTNOTAVAILABLE` while the window is busy (`try_borrow`); driver `uia-tree` PWS gate | active |
| E | Loading code into YAMDV | `UIAutomationCore.dll` from System32 only (`LOAD_LIBRARY_SEARCH_SYSTEM32`), on the first UIA request; image-load policy + ACG still apply (ADR-0001); no static import (check-imports, 20 DLLs) | active |

