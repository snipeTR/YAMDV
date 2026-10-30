// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask notices [<out>]` (prompt 020 §1.3): THIRD_PARTY_NOTICES.txt for yamdv.exe.
//!
//! The crate set is the normal (non-dev, non-build) dependency closure of `yamdv-app` for
//! x86_64-pc-windows-msvc from `cargo metadata`, i.e. what is linked into the shipped exe. For
//! each crate: name, version, SPDX expression, repository, then the license / notice / copyright
//! files it ships (identical texts are printed once and referenced afterwards). MPL-2.0 crates
//! must not carry "Incompatible With Secondary Licenses" (AGPL compatibility).
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

const TARGET: &str = "x86_64-pc-windows-msvc";

fn is_notice_file(name: &str) -> bool {
    let n = name.to_ascii_uppercase();
    [
        "LICENSE",
        "LICENCE",
        "COPYING",
        "NOTICE",
        "COPYRIGHT",
        "AUTHORS",
        "UNLICENSE",
    ]
    .iter()
    .any(|p| n.starts_with(p))
}

/// Notices text from `cargo metadata` JSON. Pure apart from reading the crates' license files.
pub fn build(meta: &Value, root_name: &str) -> Result<(String, usize), String> {
    let pkgs: HashMap<&str, &Value> = meta["packages"]
        .as_array()
        .ok_or("metadata: no packages")?
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let nodes: HashMap<&str, &Value> = meta
        .get("resolve")
        .and_then(|r| r.get("nodes"))
        .and_then(Value::as_array)
        .ok_or("metadata: no resolve")?
        .iter()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();
    let root = pkgs
        .values()
        .find(|p| {
            p.get("name").is_some_and(|n| n == root_name)
                && p.get("source").is_none_or(Value::is_null)
        })
        .and_then(|p| p["id"].as_str())
        .ok_or_else(|| format!("{root_name} not in metadata"))?;
    // Normal-dependency closure.
    let mut seen = BTreeSet::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        for d in nodes
            .get(id)
            .and_then(|n| n["deps"].as_array())
            .into_iter()
            .flatten()
        {
            let normal = d["dep_kinds"]
                .as_array()
                .is_some_and(|k| k.iter().any(|k| k["kind"].is_null()));
            if let (true, Some(pid)) = (normal, d["pkg"].as_str()) {
                stack.push(pid);
            }
        }
    }
    let mut third: BTreeMap<(String, String), &Value> = BTreeMap::new();
    for id in &seen {
        let Some(p) = pkgs.get(id) else { continue };
        if p["source"].is_null() {
            continue; // workspace crates: AGPL-3.0-or-later, LICENSE
        }
        let name = p["name"].as_str().unwrap_or("?").to_string();
        let ver = p["version"].as_str().unwrap_or("?").to_string();
        third.insert((name, ver), p);
    }
    let mut out = String::new();
    out.push_str("YAMDV - third-party notices\n");
    out.push_str("YAMDV itself is licensed under AGPL-3.0-or-later (see LICENSE);\n");
    out.push_str("source: https://github.com/snipeTR/YAMDV\n\n");
    out.push_str(&format!(
        "yamdv.exe statically links the following {} crates (target {TARGET}).\n",
        third.len()
    ));
    out.push_str("Where a crate offers a choice of licenses, YAMDV uses it under an AGPL-3.0-compatible\none (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode, BSL-1.0, CC0-1.0 or MPL-2.0).\n\n");
    for ((n, v), p) in &third {
        out.push_str(&format!(
            "  {n} {v}  {}\n",
            p["license"].as_str().unwrap_or("(none)")
        ));
    }
    let mut texts: HashMap<String, String> = HashMap::new();
    let mut errors = Vec::new();
    for ((n, v), p) in &third {
        let lic = p["license"].as_str().unwrap_or("");
        out.push_str(&format!("\n{}\n{n} {v}\nLicense: {lic}\n", "=".repeat(78)));
        if let Some(r) = p["repository"].as_str() {
            out.push_str(&format!("Repository: {r}\n"));
        }
        let dir = Path::new(p["manifest_path"].as_str().unwrap_or("./Cargo.toml"))
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file() && is_notice_file(&e.file_name().to_string_lossy()))
            .map(|e| e.path())
            .collect();
        if let Some(lf) = p["license_file"].as_str() {
            let lf = dir.join(lf);
            if lf.is_file() && !files.contains(&lf) {
                files.push(lf);
            }
        }
        files.sort();
        if files.is_empty() {
            out.push_str("(The crate ships no license file; the standard text of the license above applies.)\n");
        }
        for f in files {
            let text = std::fs::read_to_string(&f)
                .unwrap_or_default()
                .replace("\r\n", "\n");
            let fname = f
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            if lic.contains("MPL-2.0") && text.contains("Incompatible With Secondary Licenses") {
                errors.push(format!(
                    "{n} {v}: MPL-2.0 with \"Incompatible With Secondary Licenses\""
                ));
            }
            let key = text.trim().to_string();
            match texts.get(&key) {
                Some(first) => out.push_str(&format!("\n--- {fname}: same text as {first}\n")),
                None => {
                    out.push_str(&format!("\n--- {fname}\n{}\n", text.trim_end()));
                    texts.insert(key, format!("{fname} of {n} {v}"));
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(errors.join("\n"));
    }
    Ok((out, third.len()))
}

pub fn run(root: &Path, args: &[String]) -> Result<(), String> {
    let out = args
        .first()
        .map(|s| s.into())
        .unwrap_or_else(|| root.join("target").join("THIRD_PARTY_NOTICES.txt"));
    let o = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--filter-platform",
            TARGET,
        ])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !o.status.success() {
        return Err(String::from_utf8_lossy(&o.stderr).into_owned());
    }
    let meta: Value = serde_json::from_slice(&o.stdout).map_err(|e| e.to_string())?;
    let (text, n) = build(&meta, "yamdv-app")?;
    if let Some(d) = Path::new(&out).parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&out, text.as_bytes())
        .map_err(|e| format!("{}: {e}", Path::new(&out).display()))?;
    println!(
        "notices: {n} crates -> {} ({} bytes)",
        Path::new(&out).display(),
        text.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn notice_file_names() {
        assert!(super::is_notice_file("LICENSE-MIT"));
        assert!(super::is_notice_file("COPYRIGHT"));
        assert!(super::is_notice_file("license.txt"));
        assert!(!super::is_notice_file("README.md"));
    }
}
