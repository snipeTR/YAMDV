// SPDX-License-Identifier: AGPL-3.0-or-later
//! HARD-01 security unit matrices (prompt 019 §2.3, S-TEST-02) for the v2 surfaces: HTML →
//! Markdown paste, CF_HTML, the ICO directory, the recovery journal, IPC records, the custom
//! icon path and editor argument quoting.
#![allow(clippy::indexing_slicing)]

use yamdv_core::cmdline::{command_line, editor_args, expand_template, quote_arg, split_args};
use yamdv_core::image::ico;
use yamdv_core::{ipc, journal, paste, path, progress};

/// XSS vectors for the paste converter (clipboard HTML from a hostile web page).
const XSS: &[&str] = &[
    "<script>alert(1)</script>",
    "<SCRIPT SRC=//x.example/x.js></SCRIPT>",
    "<scr<script>ipt>alert(1)</script>",
    "<img src=x onerror=alert(1)>",
    "<img src=javascript:alert(1)>",
    "<img src=\"jav&#x09;ascript:alert(1)\">",
    "<img src=\"&#106;&#97;&#118;&#97;&#115;&#99;&#114;&#105;&#112;&#116;&#58;alert(1)\">",
    "<img src=\"data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==\">",
    "<svg onload=alert(1)>",
    "<svg><script>alert(1)</script></svg>",
    "<iframe src=javascript:alert(1)></iframe>",
    "<iframe srcdoc=\"<script>alert(1)</script>\"></iframe>",
    "<object data=javascript:alert(1)></object>",
    "<embed src=javascript:alert(1)>",
    "<a href=\"javascript:alert(1)\">x</a>",
    "<a href=\"JaVaScRiPt:alert(1)\">x</a>",
    "<a href=\" javascript:alert(1)\">x</a>",
    "<a href=\"java\nscript:alert(1)\">x</a>",
    "<a href=\"java&#09;script:alert(1)\">x</a>",
    "<a href=\"&#x6A;avascript:alert(1)\">x</a>",
    "<a href=\"vbscript:msgbox(1)\">x</a>",
    "<a href=\"data:text/html,<script>alert(1)</script>\">x</a>",
    "<a href=\"x\" onclick=\"alert(1)\">x</a>",
    "<div onmouseover=\"alert(1)\">hover</div>",
    "<div style=\"background:url(javascript:alert(1))\">s</div>",
    "<style>body{background:url(javascript:alert(1))}</style>",
    "<link rel=stylesheet href=javascript:alert(1)>",
    "<meta http-equiv=refresh content=\"0;url=javascript:alert(1)\">",
    "<base href=javascript:alert(1)//>",
    "<form action=javascript:alert(1)><button>x</button></form>",
    "<math><mtext><script>alert(1)</script></mtext></math>",
    "<template><script>alert(1)</script></template>",
    "<noscript><p title=\"</noscript><script>alert(1)</script>\"></noscript>",
    "<table><tr><td><a href=\"javascript:alert(1)\">c</a></td></tr></table>",
    "<ul><li><a href='javascript:alert(1)'>i</a></li></ul>",
    "<pre><code>&lt;script&gt;alert(1)&lt;/script&gt;</code></pre>",
    "<a href=\"https://ok.example/\" onfocus=alert(1) autofocus>ok</a>",
    "<!--<script>alert(1)</script>-->",
    "<![CDATA[<script>alert(1)</script>]]>",
    "<div>[x](javascript:alert(1))</div>",
    "<p>&lt;img src=x onerror=alert(1)&gt;</p>",
];

fn md(html: &str) -> String {
    let mut save = |_: &[u8], _: &'static str| -> Option<String> { None };
    paste::html2md::convert(html, &mut save).unwrap_or_default()
}

/// Markdown output that re-parses into something live: a raw `<script`, an event handler, or a
/// link / image destination with a script scheme.
fn dangerous(out: &str) -> Option<&'static str> {
    // Fenced code is inert text (shown verbatim, never parsed as HTML).
    let mut fence = false;
    let live: Vec<&str> = out
        .lines()
        .filter(|l| {
            if l.starts_with("```") {
                fence = !fence;
                return false;
            }
            !fence
        })
        .collect();
    // An escaped `\<` starts no tag.
    let l = live.join("\n").to_ascii_lowercase().replace("\\<", "");
    // An escaped `\]` is literal text, not the end of a link label.
    let compact: String = l
        .replace("\\]", "")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if l.contains("<script") {
        return Some("<script");
    }
    if compact.contains("](javascript:") || compact.contains("](vbscript:") {
        return Some("script link");
    }
    if compact.contains("](data:text") {
        return Some("data: link");
    }
    if (l.contains("<a ") || l.contains("<img")) && (l.contains(" on") || l.contains("javascript:"))
    {
        return Some("raw html");
    }
    None
}

#[test]
fn hard01_paste_xss_matrix_yields_no_script() {
    assert!(XSS.len() >= 40);
    for v in XSS {
        let out = md(v);
        assert_eq!(dangerous(&out), None, "{v:?} -> {out:?}");
        // CF_HTML framing around the same vector.
        let raw = format!(
            "Version:0.9\r\nStartHTML:0\r\nEndHTML:0\r\n<html><body><!--StartFragment-->{v}<!--EndFragment--></body></html>"
        );
        let out = md(paste::cf_html_fragment(&raw));
        assert_eq!(dangerous(&out), None, "cf {v:?} -> {out:?}");
    }
    // Safe content still converts.
    assert_eq!(
        md("<a href=\"https://ok.example/\">ok</a>"),
        "[ok](https://ok.example/)"
    );
}

#[test]
fn hard01_cf_html_offsets_matrix() {
    let body = "<b>x</b>";
    for raw in [
        // Offsets past the end, reversed, non-digits, huge, inside a UTF-8 sequence.
        "StartFragment:999999\r\nEndFragment:9999999\r\n".to_string(),
        "StartFragment:50\r\nEndFragment:10\r\n<b>x</b>".to_string(),
        "StartFragment:-1\r\nEndFragment:x\r\n".to_string(),
        format!("StartFragment:{}\r\nEndFragment:{}\r\n", u64::MAX, u64::MAX),
        "StartFragment:1\r\nEndFragment:2\r\né".to_string(),
        format!("StartFragment:0\r\nEndFragment:0\r\n{body}"),
        String::new(),
    ] {
        let f = paste::cf_html_fragment(&raw);
        assert!(f.len() <= raw.len());
        let _ = md(f);
    }
}

fn ico_file(n: u16, entries: &[(u8, u8, u16, u32, u32)], tail: usize) -> Vec<u8> {
    let mut b = vec![0, 0, 1, 0];
    b.extend_from_slice(&n.to_le_bytes());
    for &(w, h, bpp, len, off) in entries {
        b.extend_from_slice(&[w, h, 0, 0, 1, 0]);
        b.extend_from_slice(&bpp.to_le_bytes());
        b.extend_from_slice(&len.to_le_bytes());
        b.extend_from_slice(&off.to_le_bytes());
    }
    b.resize(b.len() + tail, 0);
    b
}

#[test]
fn hard01_ico_directory_matrix() {
    // One valid entry: 16 bytes of payload right after the directory.
    let ok = ico_file(1, &[(16, 16, 32, 16, 22)], 16);
    assert_eq!(ico::directory(&ok).map(|d| d.len()), Some(1));
    let bad: Vec<(&str, Vec<u8>)> = vec![
        ("empty", vec![]),
        ("short header", vec![0, 0, 1]),
        ("cursor type", {
            let mut b = ok.clone();
            b[2] = 2;
            b
        }),
        ("reserved set", {
            let mut b = ok.clone();
            b[0] = 1;
            b
        }),
        ("zero entries", ico_file(0, &[], 16)),
        ("count beyond data", ico_file(3, &[(16, 16, 32, 16, 54)], 0)),
        ("too many entries", ico_file(300, &[], 0)),
        ("zero length", ico_file(1, &[(16, 16, 32, 0, 22)], 16)),
        (
            "offset inside directory",
            ico_file(1, &[(16, 16, 32, 16, 6)], 16),
        ),
        (
            "offset past end",
            ico_file(1, &[(16, 16, 32, 16, 4000)], 16),
        ),
        ("length past end", ico_file(1, &[(16, 16, 32, 17, 22)], 16)),
        (
            "offset + length overflows",
            ico_file(1, &[(16, 16, 32, u32::MAX, u32::MAX)], 16),
        ),
    ];
    for (what, b) in bad {
        assert_eq!(ico::directory(&b), None, "{what}");
    }
    // Payload of a garbage entry never panics and never reads outside the file.
    if let Some(d) = ico::directory(&ok) {
        for e in d {
            let _ = ico::payload(&ok, &e);
        }
    }
}

#[test]
fn hard01_journal_parsing_matrix() {
    let h = journal::Header {
        path: "C:\\d\\a.md".into(),
        mtime: 7,
        hash: 0xabc,
        encoding: "utf-8".into(),
    };
    let good = journal::encode(&h, "text").unwrap_or_default();
    assert_eq!(journal::decode(&good), Some((h.clone(), "text".into())));
    // Header injection through a path with line breaks is neutralized on encode.
    let evil = journal::Header {
        path: "C:\\a.md\nlen: 0".into(),
        ..h.clone()
    };
    let e = journal::encode(&evil, "x").unwrap_or_default();
    let (eh, et) = journal::decode(&e).unwrap_or_default();
    assert_eq!((eh.path.as_str(), et.as_str()), ("C:\\a.mdlen: 0", "x"));
    let s = String::from_utf8(good.clone()).unwrap_or_default();
    for bad in [
        String::new(),
        "YAMDV-RECOVERY 2\npath: x\nlen: 1\n\nx".to_string(),
        s.replace("len: 4", "len: 5"),
        s.replace("len: 4", "len: 3"),
        s.replace("len: 4", "len: -1"),
        s.replace("len: 4", "len: 99999999999999999999999"),
        s.replace("mtime: 7", "mtime: x"),
        s.replace("hash: 0000000000000abc", "hash: zz"),
        s.replace("\n\n", "\n"),
        s.trim_end_matches("text").to_string() + "tex",
        // HARD-02 regression (fuzz `journal`): a CR inside a header value.
        "YAMDV-RECOVERY 1\npath: \nmtime: 0\nhash: 0000000000000000\nencoding: utf-1\rle\nlen: 0\n\n"
            .to_string(),
        s.replace("path: ", "path: a\rb"),
    ] {
        assert_eq!(journal::decode(bad.as_bytes()), None, "{bad:?}");
    }
    // Invalid UTF-8 in the body or header.
    let mut b = good.clone();
    let n = b.len();
    b[n - 1] = 0xff;
    assert_eq!(journal::decode(&b), None);
    let mut b = good;
    b[0] = 0xff;
    assert_eq!(journal::decode(&b), None);
}

#[test]
fn hard01_ipc_records_matrix() {
    let r = ipc::Reply {
        kind: ipc::Kind::Open,
        nonce: 42,
        outcome: ipc::Outcome::Paths(vec!["C:\\a.md".into()]),
    };
    let good = ipc::encode(&r);
    assert_eq!(ipc::decode(&good), Ok(r));
    // Truncation at every length, trailing bytes, bad magic / version / kind / status.
    for cut in 0..good.len() {
        assert!(ipc::decode(&good[..cut]).is_err(), "cut {cut}");
    }
    let mut t = good.clone();
    t.push(0);
    assert!(ipc::decode(&t).is_err());
    for (at, v) in [(0usize, 0u8), (4, 9), (6, 3), (6, 99), (16, 7)] {
        let mut b = good.clone();
        b[at] = v;
        assert!(ipc::decode(&b).is_err(), "byte {at} = {v}");
    }
    // A length field pointing past the end, and an unpaired surrogate.
    let mut b = good.clone();
    b[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(ipc::decode(&b).is_err());
    let mut b = good.clone();
    b[24..26].copy_from_slice(&0xd800u16.to_le_bytes());
    assert_eq!(ipc::decode(&b), Err(ipc::DecodeError::Encoding));
    // Forward messages are capped at 64 KB (S-IPC-01).
    let many: Vec<String> = (0..2000)
        .map(|i| format!("C:\\docs\\file-{i:05}.md"))
        .collect();
    let fwd = ipc::encode_forward(&many, 3);
    assert!(fwd.len() <= ipc::FORWARD_MAX_BYTES);
    assert!(ipc::decode(&fwd).is_ok());

    // Progress records (worker → window, S-IPC-05).
    let rec = progress::Record {
        nonce: 9,
        phase: progress::Phase::Printing,
        n: 3,
        total: 10,
    };
    let pb = progress::encode(&rec);
    assert_eq!(progress::decode(&pb), Some(rec));
    for cut in 0..pb.len() {
        assert_eq!(progress::decode(&pb[..cut]), None);
    }
    for (at, v) in [(0usize, 0u8), (4, 2), (6, 0), (6, 3)] {
        let mut b = pb;
        b[at] = v;
        assert_eq!(progress::decode(&b), None, "byte {at}");
    }
    let mut b = pb;
    b[16..20].copy_from_slice(&11u32.to_le_bytes());
    assert_eq!(progress::decode(&b), None, "n > total");
}

#[test]
fn hard01_custom_icon_path_matrix() {
    for ok in [
        "C:\\Users\\me\\icons\\app.ico",
        "D:\\x\\A.ICO",
        "\\\\?\\C:\\long\\a.ico",
    ] {
        assert!(path::icon_candidate(ok), "{ok}");
    }
    for bad in [
        "",
        "C:\\icons\\app.png",
        "C:\\icons\\app.ico.exe",
        "C:\\icons\\app",
        "\\\\server\\share\\app.ico",
        "//server/share/app.ico",
        "\\\\?\\UNC\\server\\share\\app.ico",
        "\\\\host@SSL\\DavWWWRoot\\app.ico",
        "\\\\host@8080\\x\\app.ico",
        "\\\\.\\C:\\app.ico",
        "\\\\?\\GLOBALROOT\\Device\\x.ico",
        "C:\\icons\\app.ico:evil",
        "C:\\icons\\a.ico:Zone.Identifier",
        "C:\\icons\\CON.ico",
        "C:\\nul\\a.ico",
        "C:\\icons\\a\0.ico",
    ] {
        assert!(!path::icon_candidate(bad), "{bad:?}");
    }
}

#[test]
fn hard01_editor_argument_quoting_matrix() {
    let files = [
        "C:\\a b\\c.md",
        "C:\\x\\\"quoted\".md",
        "C:\\trail\\",
        "C:\\dir with space\\",
        "C:\\a\\b.md\" --evil \"x",
        "C:\\a\\b.md & calc.exe",
        "C:\\a\\%PATH%.md",
        "C:\\a\\tab\there.md",
        "C:\\ünï\\ğüşiöç.md",
        "\\\\\\\"",
    ];
    use yamdv_core::cmdline::Editor;
    for f in files {
        for e in [
            Editor::VsCode,
            Editor::Sublime,
            Editor::EmEditor,
            Editor::NotepadPp,
            Editor::Notepad,
            Editor::Typora,
        ] {
            let args = editor_args(e, f, 12, 3);
            let line = command_line("C:\\Program Files\\Ed\\ed.exe", &args);
            let back = split_args(&line);
            assert_eq!(back.get(1..), Some(&args[..]), "{e:?} {f:?}");
        }
        // Custom template: placeholders expand inside one token only.
        let args = expand_template("--open {file} -l {line}", f, 5, 1);
        assert_eq!(args.len(), 4, "{f:?}");
        assert_eq!(args[1], f);
        let back = split_args(&command_line("C:\\e.exe", &args));
        assert_eq!(back.get(1..), Some(&args[..]));
        assert_eq!(split_args(&quote_arg(f)), vec![f.to_string()]);
    }
}

/// HARD-02 fuzz-long regressions (run 41016337092): every committed `regress-*` seed of
/// `html_to_md` converts to Markdown whose raw-HTML events hold no `<script` tag (the fuzz
/// target's oracle).
#[test]
fn hard02_html_to_md_regression_seeds() {
    use pulldown_cmark::{Event, Options, Parser};
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/html_to_md");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).expect("seeds") {
        let p = e.expect("entry").path();
        if !p
            .file_name()
            .is_some_and(|f| f.to_string_lossy().starts_with("regress-"))
        {
            continue;
        }
        let s = String::from_utf8_lossy(&std::fs::read(&p).expect("read")).into_owned();
        let out = md(&s);
        let opts =
            Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
        for ev in Parser::new_ext(&out, opts) {
            if let Event::Html(h) | Event::InlineHtml(h) = ev {
                assert!(
                    !h.to_ascii_lowercase().contains("<script"),
                    "{}: {h:?}",
                    p.display()
                );
            }
        }
        n += 1;
    }
    assert!(n >= 7, "{n} regression seeds");
}

/// HARD-02 fuzz-long `ini`: every committed seed keeps the settings round trip of the target.
#[test]
fn hard02_ini_round_trip_seeds() {
    use yamdv_core::ini::Ini;
    use yamdv_core::settings::Settings;
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz/seeds/ini");
    let mut n = 0;
    for e in std::fs::read_dir(&dir).expect("seeds") {
        let p = e.expect("entry").path();
        let ini = Ini::parse(&std::fs::read(&p).expect("read"));
        let s = Settings::from_ini(&ini);
        let again = Ini::parse(ini.write().as_bytes());
        assert_eq!(Settings::from_ini(&again), s, "{}", p.display());
        n += 1;
    }
    assert!(n >= 3, "seeds missing: {n}");
}
