// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pure output checks shared by the unit tests and the dev-tools driver (018a §2-C/§2-D):
//! the interactive HTML export (CSP hash, external references, forbidden script APIs),
//! `CF_HTML` offsets and PNG validity.

use crate::sha256::{base64_32, sha256};

/// `sha256-…` CSP source of `script` (runtime twin of the build-time hash).
#[must_use]
pub fn hash_source(script: &str) -> String {
    format!(
        "sha256-{}",
        String::from_utf8_lossy(&base64_32(&sha256(script.as_bytes())))
    )
}

/// The contents of every inline `<script>` element of `html`.
#[must_use]
pub fn inline_scripts(html: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(i) = rest.find("<script>") {
        let after = rest.get(i + 8..).unwrap_or_default();
        let Some(j) = after.find("</script>") else {
            break;
        };
        out.push(after.get(..j).unwrap_or_default());
        rest = after.get(j + 9..).unwrap_or_default();
    }
    out
}

/// The CSP meta `content` of `html`, if any.
#[must_use]
pub fn csp(html: &str) -> Option<&str> {
    let k = "http-equiv=\"Content-Security-Policy\" content=\"";
    let i = html.find(k)? + k.len();
    let rest = html.get(i..)?;
    rest.get(..rest.find('"')?)
}

/// The interactive export carries exactly one inline script, and the CSP allows exactly that
/// script by its SHA-256 (and nothing else: no `unsafe-inline` / `unsafe-eval` for scripts).
#[must_use]
pub fn csp_hash_ok(html: &str) -> bool {
    let scripts = inline_scripts(html);
    let (Some(c), [s]) = (csp(html), scripts.as_slice()) else {
        return false;
    };
    let Some(src) = c
        .split(';')
        .map(str::trim)
        .find_map(|d| d.strip_prefix("script-src "))
    else {
        return false;
    };
    src.trim() == format!("'{}'", hash_source(s))
}

/// References that would make a browser load something from outside the file: `src`/`srcset`/
/// `poster`/`data` attributes that are not `data:image/`, `<link>`, `<iframe>`, `<object>`,
/// `<embed>`, `<base>`, CSS `@import` and `url(` that is not `data:`. Links (`<a href>`) and SVG
/// namespace URIs do not load anything and are not counted.
#[must_use]
pub fn external_refs(html: &str) -> usize {
    let l = html.to_ascii_lowercase();
    let mut n = 0;
    for attr in [
        " src=\"",
        " srcset=\"",
        " poster=\"",
        " data=\"",
        " xlink:href=\"http",
    ] {
        for (i, _) in l.match_indices(attr) {
            let v = l.get(i + attr.len()..).unwrap_or_default();
            if attr.ends_with("http") || !v.starts_with("data:image/") {
                n += 1;
            }
        }
    }
    for tag in ["<link", "<iframe", "<object", "<embed", "<base"] {
        n += l.matches(tag).count();
    }
    // CSS: `<style>` elements and `style="…"` attributes (document text is escaped, so a
    // "url(" there is inert and not CSS).
    let mut css: Vec<&str> = Vec::new();
    let mut rest = l.as_str();
    while let Some(i) = rest.find("<style") {
        let a = rest.get(i..).unwrap_or_default();
        let Some(j) = a.find("</style>") else { break };
        css.push(a.get(..j).unwrap_or_default());
        rest = a.get(j..).unwrap_or_default();
    }
    for (i, _) in l.match_indices(" style=\"") {
        let v = l.get(i + 8..).unwrap_or_default();
        css.push(v.get(..v.find('"').unwrap_or(v.len())).unwrap_or_default());
    }
    for c in css {
        n += c.matches("@import").count();
        for (i, _) in c.match_indices("url(") {
            let v = c
                .get(i + 4..)
                .unwrap_or_default()
                .trim_start_matches(['"', '\'']);
            if !v.starts_with("data:") && !v.starts_with('#') {
                n += 1;
            }
        }
    }
    n
}

/// Forbidden APIs found in a script source (S-EXP-02), as whole identifiers.
#[must_use]
pub fn forbidden_apis(script: &str) -> Vec<&'static str> {
    const API: [&str; 6] = [
        "eval",
        "Function",
        "innerHTML",
        "fetch",
        "XMLHttpRequest",
        "WebSocket",
    ];
    let ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '$';
    let mut found = Vec::new();
    for a in API {
        let hit = script.match_indices(a).any(|(i, _)| {
            let before = script.get(..i).and_then(|s| s.chars().next_back());
            let after = script.get(i + a.len()..).and_then(|s| s.chars().next());
            !before.is_some_and(ident) && !after.is_some_and(ident)
        });
        if hit {
            found.push(a);
        }
    }
    found
}

/// The `StartHTML`/`EndHTML`/`StartFragment`/`EndFragment` byte offsets of a `CF_HTML` payload
/// are consistent: `<html`/`<!doctype` at StartHTML, the fragment markers right around the
/// fragment, EndHTML = length.
#[must_use]
pub fn cf_html_offsets_ok(c: &str) -> bool {
    let num = |k: &str| -> Option<usize> {
        let i = c.find(k)? + k.len();
        c.get(i..i + 10)?.parse().ok()
    };
    let (Some(sh), Some(eh), Some(sf), Some(ef)) = (
        num("StartHTML:"),
        num("EndHTML:"),
        num("StartFragment:"),
        num("EndFragment:"),
    ) else {
        return false;
    };
    let starts_html = c
        .get(sh..)
        .is_some_and(|s| s.starts_with("<html") || s.to_ascii_lowercase().starts_with("<!doctype"));
    let frag_marked = c
        .get(..sf)
        .is_some_and(|s| s.ends_with("<!--StartFragment-->"))
        && c.get(ef..)
            .is_some_and(|s| s.starts_with("<!--EndFragment-->"));
    starts_html && frag_marked && sh <= sf && sf <= ef && ef <= eh && eh == c.len()
}

fn crc32(data: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
    }
    !c
}

/// A structurally valid PNG: signature, IHDR first (non-zero size), every chunk CRC correct,
/// at least one IDAT, IEND last. Returns `(width, height)`.
#[must_use]
pub fn png_ok(b: &[u8]) -> Option<(u32, u32)> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if b.get(..8)? != SIG {
        return None;
    }
    let be = |s: &[u8]| -> Option<u32> { Some(u32::from_be_bytes(s.try_into().ok()?)) };
    let (mut p, mut first, mut idat, mut size) = (8usize, true, false, None);
    while p < b.len() {
        let len = be(b.get(p..p + 4)?)? as usize;
        let kind = b.get(p + 4..p + 8)?;
        let body = b.get(p + 8..p + 8 + len)?;
        let crc = be(b.get(p + 8 + len..p + 12 + len)?)?;
        if crc32(b.get(p + 4..p + 8 + len)?) != crc {
            return None;
        }
        if first {
            if kind != b"IHDR" || len != 13 {
                return None;
            }
            let (w, h) = (be(body.get(..4)?)?, be(body.get(4..8)?)?);
            if w == 0 || h == 0 {
                return None;
            }
            size = Some((w, h));
            first = false;
        }
        idat |= kind == b"IDAT";
        p += 12 + len;
        if kind == b"IEND" {
            return (p == b.len() && idat).then_some(size?);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_hash_recomputed_and_compared() {
        // Known answer (FIPS 180-2 "abc").
        assert_eq!(
            hash_source("abc"),
            "sha256-ungWv48Bz+pBQUDeXa4iI7ADYaOWF3qctBD/YfIAFa0="
        );
        assert_eq!(hash_source(crate::html::SCRIPT), crate::html::script_hash());
    }

    #[test]
    fn script_has_no_forbidden_api() {
        assert_eq!(forbidden_apis(crate::html::SCRIPT), Vec::<&str>::new());
        assert_eq!(
            forbidden_apis("x.innerHTML=1;eval(a);new Function('');fetch(u)"),
            ["eval", "Function", "innerHTML", "fetch"]
        );
        assert!(forbidden_apis("retrieval; evaluate(); myFetch; FunctionX").is_empty());
    }

    #[test]
    fn external_refs_counts_loads_not_links() {
        assert_eq!(
            external_refs(
                "<a href=\"https://e.com\">x</a><svg xmlns=\"http://www.w3.org/2000/svg\"></svg><img src=\"data:image/png;base64,AA\">"
            ),
            0
        );
        assert_eq!(
            external_refs(
                "<img src=\"https://e.com/a.png\"><link rel=stylesheet href=x><style>@import 'y';b{background:url(z.png)}</style>"
            ),
            4
        );
    }

    #[test]
    fn csp_hash_ok_only_for_the_matching_script() {
        let good = format!(
            "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; script-src '{}'\"><script>a()</script>",
            hash_source("a()")
        );
        assert!(csp_hash_ok(&good));
        assert!(!csp_hash_ok(&good.replace("a()</", "b()</")));
        assert!(!csp_hash_ok(&format!("{good}<script>a()</script>")));
    }

    #[test]
    fn cf_html_offsets_match_bytes() {
        assert!(cf_html_offsets_ok(&crate::html::cf_html("<b>é ü</b>")));
        let d = "<!doctype html><html><head></head><body>\n<p>Ğ</p>\n</body></html>";
        assert!(cf_html_offsets_ok(&crate::html::cf_html_document(d)));
        assert!(!cf_html_offsets_ok(
            &crate::html::cf_html("<b>é</b>").replacen("é", "e", 1)
        ));
    }

    #[test]
    fn png_validity() {
        // 1×1 grey PNG.
        let png: [u8; 67] = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00, 0x00,
            0x00, 0x3A, 0x7E, 0x9B, 0x55, 0x00, 0x00, 0x00, 0x0A, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x63, 0x68, 0x00, 0x00, 0x00, 0x82, 0x00, 0x81, 0x77, 0xCD, 0x72, 0xB6, 0x00,
            0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ];
        assert_eq!(png_ok(&png), Some((1, 1)));
        let mut bad = png;
        if let Some(b) = bad.get_mut(41) {
            *b ^= 1;
        }
        assert_eq!(png_ok(&bad), None, "IDAT CRC");
        assert_eq!(png_ok(png.get(..61).unwrap_or_default()), None, "truncated");
    }
}
