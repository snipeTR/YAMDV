// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask pdf-inspect <file.pdf> [--find <text>]` (018a §2-B): structural facts about a
//! PDF written by YAMDV, as one JSON object on stdout, for the `pdf-native` CI checks.
//! lopdf is an xtask (dev tool) dependency only and never linked into yamdv.exe.

use lopdf::{Dictionary, Document, Object, ObjectId};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// Keys whose presence anywhere makes the file active content (S-EXP-03).
const FORBIDDEN: [&[u8]; 6] = [
    b"JavaScript",
    b"JS",
    b"Launch",
    b"EmbeddedFile",
    b"EmbeddedFiles",
    b"AcroForm",
];

fn name(o: &Object) -> Option<&[u8]> {
    o.as_name().ok()
}

fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map_or(o, |(_, o)| o)
}

fn dict<'a>(doc: &'a Document, o: &'a Object) -> Option<&'a Dictionary> {
    match deref(doc, o) {
        Object::Dictionary(d) => Some(d),
        Object::Stream(s) => Some(&s.dict),
        _ => None,
    }
}

fn get<'a>(doc: &'a Document, d: &'a Dictionary, k: &[u8]) -> Option<&'a Object> {
    d.get(k).ok().map(|o| deref(doc, o))
}

/// Outline items reachable from /Outlines (First/Next walk, loop-safe).
fn outline_count(doc: &Document) -> usize {
    let Some(root) = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Outlines").ok())
        .and_then(|o| dict(doc, o))
    else {
        return 0;
    };
    let mut seen = BTreeSet::new();
    let mut stack: Vec<ObjectId> = root
        .get(b"First")
        .and_then(Object::as_reference)
        .into_iter()
        .collect();
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Ok(d) = doc.get_dictionary(id) {
            for k in [b"First".as_slice(), b"Next"] {
                if let Ok(r) = d.get(k).and_then(Object::as_reference) {
                    stack.push(r);
                }
            }
        }
    }
    seen.len()
}

fn walk_forbidden(o: &Object, found: &mut BTreeSet<String>) {
    let scan = |d: &Dictionary, found: &mut BTreeSet<String>| {
        for (k, v) in d.iter() {
            for f in FORBIDDEN {
                if k.as_slice() == f || name(v) == Some(f) {
                    found.insert(String::from_utf8_lossy(f).into_owned());
                }
            }
            walk_forbidden(v, found);
        }
    };
    match o {
        Object::Dictionary(d) => scan(d, found),
        Object::Stream(s) => scan(&s.dict, found),
        Object::Array(a) => a.iter().for_each(|v| walk_forbidden(v, found)),
        _ => {}
    }
}

fn fonts(doc: &Document) -> Vec<Value> {
    let mut out = Vec::new();
    for o in doc.objects.values() {
        let Some(d) = (match o {
            Object::Dictionary(d) => Some(d),
            _ => None,
        }) else {
            continue;
        };
        if d.get(b"Type").ok().and_then(name) != Some(b"Font") {
            continue;
        }
        let sub = d.get(b"Subtype").ok().and_then(name).unwrap_or_default();
        if sub == b"CIDFontType2" || sub == b"CIDFontType0" {
            continue; // reported through their Type0 parent
        }
        let base = d
            .get(b"BaseFont")
            .ok()
            .and_then(name)
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .unwrap_or_default();
        let subset = base.len() > 7
            && base.as_bytes().get(6) == Some(&b'+')
            && base.bytes().take(6).all(|b| b.is_ascii_uppercase());
        let descendant = get(doc, d, b"DescendantFonts")
            .and_then(|a| a.as_array().ok())
            .and_then(|a| a.first())
            .and_then(|o| dict(doc, o));
        let desc = descendant
            .unwrap_or(d)
            .get(b"FontDescriptor")
            .ok()
            .and_then(|o| dict(doc, o));
        let embedded = desc.is_some_and(|fd| {
            [b"FontFile".as_slice(), b"FontFile2", b"FontFile3"]
                .iter()
                .any(|k| fd.has(k))
        });
        out.push(json!({
            "base": base,
            "subtype": String::from_utf8_lossy(sub),
            "subset": subset,
            "embedded": embedded,
            "tounicode": d.has(b"ToUnicode"),
        }));
    }
    out
}

/// Fill colour of the first full-page rectangle on page 1, `None` when there is none (white).
fn page1_bg(doc: &Document, page: ObjectId) -> Option<[f32; 3]> {
    let content = lopdf::content::Content::decode(&doc.get_page_content(page)).ok()?;
    let mut fill = None;
    for op in content.operations.iter().take(12) {
        match op.operator.as_str() {
            "rg" => {
                let v: Vec<f32> = op
                    .operands
                    .iter()
                    .filter_map(|o| o.as_float().ok())
                    .collect();
                if let [r, g, b] = v.as_slice() {
                    fill = Some([*r, *g, *b]);
                }
            }
            "re" => {
                let w = op
                    .operands
                    .get(2)
                    .and_then(|o| o.as_float().ok())
                    .unwrap_or(0.0);
                if w >= 500.0 {
                    return fill;
                }
            }
            "cm" | "BT" | "Do" => return None,
            _ => {}
        }
    }
    None
}

fn squash(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Entry point.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut it = args.iter();
    let file = it
        .next()
        .ok_or("usage: pdf-inspect <file.pdf> [--find <text>]")?;
    let mut find = None;
    while let Some(a) = it.next() {
        if a == "--find" {
            find = it.next().cloned();
        }
    }
    let bytes = std::fs::read(file).map_err(|e| format!("{file}: {e}"))?;
    let doc = Document::load_mem(&bytes).map_err(|e| format!("{file}: parse: {e}"))?;
    let pages = doc.get_pages();
    let (mut goto, mut uris, mut bad_uris) = (0usize, Vec::new(), Vec::new());
    for o in doc.objects.values() {
        let Object::Dictionary(d) = o else { continue };
        if d.get(b"Subtype").ok().and_then(name) != Some(b"Link") {
            continue;
        }
        if d.has(b"Dest") {
            goto += 1;
        }
        let Some(a) = get(&doc, d, b"A").and_then(|o| dict(&doc, o)) else {
            continue;
        };
        match a.get(b"S").ok().and_then(name) {
            Some(b"GoTo") => goto += 1,
            Some(b"URI") => {
                let u = a
                    .get(b"URI")
                    .ok()
                    .and_then(|o| o.as_str().ok())
                    .map(|s| String::from_utf8_lossy(s).into_owned())
                    .unwrap_or_default();
                let l = u.to_ascii_lowercase();
                if !(l.starts_with("http://")
                    || l.starts_with("https://")
                    || l.starts_with("mailto:"))
                {
                    bad_uris.push(u.clone());
                }
                uris.push(u);
            }
            Some(other) => bad_uris.push(format!("/S {}", String::from_utf8_lossy(other))),
            None => {}
        }
    }
    let mut forbidden = BTreeSet::new();
    if let Ok(c) = doc.catalog() {
        for k in [b"OpenAction".as_slice(), b"AA"] {
            if c.has(k) {
                forbidden.insert(String::from_utf8_lossy(k).into_owned());
            }
        }
    }
    for o in doc.objects.values() {
        walk_forbidden(o, &mut forbidden);
    }
    let fonts = fonts(&doc);
    let fonts_ok = !fonts.is_empty()
        && fonts.iter().all(|f| {
            f["subset"].as_bool() == Some(true)
                && f["embedded"].as_bool() == Some(true)
                && f["tounicode"].as_bool() == Some(true)
        });
    let found = find.as_ref().map(|needle| {
        let nums: Vec<u32> = pages.keys().copied().collect();
        let text = doc.extract_text(&nums).unwrap_or_default();
        squash(&text).contains(&squash(needle))
    });
    let bg = pages.values().next().and_then(|&p| page1_bg(&doc, p));
    let white = bg.is_none_or(|c| c.iter().all(|v| *v >= 0.99));
    let out = json!({
        "file": file,
        "bytes": bytes.len(),
        "pages": pages.len(),
        "outline_count": outline_count(&doc),
        "goto_links": goto,
        "uri_links": uris.len(),
        "uris": uris,
        "bad_uris": bad_uris,
        "forbidden": forbidden.into_iter().collect::<Vec<_>>(),
        "fonts": fonts,
        "fonts_ok": fonts_ok,
        "find": find,
        "found": found,
        "page1_bg": bg,
        "page1_white": white,
    });
    println!("{out}");
    Ok(())
}
