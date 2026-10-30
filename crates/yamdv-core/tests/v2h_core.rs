// SPDX-License-Identifier: AGPL-3.0-or-later
//! 018a §2-D unit tests: encoding round trips and editor argument quoting.
#![allow(clippy::indexing_slicing, clippy::unwrap_used)]

use yamdv_core::cmdline::{Editor, command_line, editor_args, split_args};
use yamdv_core::decode::{REOPEN, decode, decode_as};
use yamdv_core::encode::encode;

#[test]
fn tr_1254_fixture_reopens_and_saves_byte_exact() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/tr-1254.md"
    ))
    .unwrap();
    // Detection guesses windows-1252 (Ş/ş are Þ/þ there): the case "Reopen with encoding" fixes.
    assert_eq!(decode(&bytes).encoding, "windows-1252");
    let d = decode_as(&bytes, "windows-1254").unwrap();
    assert_eq!(d.encoding, "windows-1254");
    assert!(
        d.text.contains("Şu an") && d.text.contains("Menü"),
        "{}",
        d.text
    );
    assert_eq!(encode(&d.text, d.encoding, d.had_bom).unwrap(), bytes);
}

#[test]
fn every_reopen_encoding_round_trips() {
    let text = "# Başlık\r\n\r\nabc 123 — x\r\n";
    let mut n = 0;
    for (_, label) in REOPEN {
        let name = encoding_rs::Encoding::for_label(label.as_bytes())
            .unwrap()
            .name();
        let Ok(b) = encode(text, name, false).or_else(|_| encode("abc\r\nxyz\r\n", name, false))
        else {
            panic!("{label}: no encoder");
        };
        let d = decode_as(&b, label).unwrap();
        assert_eq!(d.encoding, name, "{label}");
        assert_eq!(
            encode(&d.text, d.encoding, d.had_bom).unwrap(),
            b,
            "{label}"
        );
        n += 1;
    }
    assert_eq!(n, REOPEN.len());
    // BOMs survive a round trip for the Unicode encodings.
    for label in ["UTF-8", "UTF-16LE", "UTF-16BE"] {
        let b = encode(text, label, true).unwrap();
        let d = decode(&b);
        assert!(d.had_bom, "{label}");
        assert_eq!(d.text, text, "{label}");
        assert_eq!(
            encode(&d.text, d.encoding, d.had_bom).unwrap(),
            b,
            "{label}"
        );
    }
}

#[test]
fn editor_arguments_are_quoted_for_create_process() {
    let exe = r"C:\Users\A B\AppData\Local\Programs\Microsoft VS Code\Code.exe";
    let tricky = [
        r"C:\docs\plain.md",
        r"C:\my docs\a b.md",
        r#"C:\x\quote"in.md"#,
        r"C:\trailing slash\",
        r"C:\percent %PATH% & | ^ ( ).md",
        r"\\server\share\ünï cödé.md",
    ];
    for e in [
        Editor::VsCode,
        Editor::Sublime,
        Editor::EmEditor,
        Editor::Typora,
        Editor::NotepadPp,
        Editor::Notepad,
    ] {
        for f in tricky {
            let args = editor_args(e, f, 12, 3);
            let cl = command_line(exe, &args);
            let mut want = vec![exe.to_string()];
            want.extend(args.iter().cloned());
            assert_eq!(split_args(&cl), want, "{e:?} {f}: {cl}");
        }
    }
    assert_eq!(
        command_line(exe, &editor_args(Editor::VsCode, r"C:\my docs\a.md", 7, 1)),
        format!("\"{exe}\" --goto \"C:\\my docs\\a.md:7:1\"")
    );
    assert_eq!(
        editor_args(Editor::NotepadPp, "f.md", 4, 2),
        vec!["-n4", "-c2", "f.md"]
    );
}
