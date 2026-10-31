// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask future`: extracts the `FUTURE.md` block from prompt 005 into `docs/FUTURE.md`
//! and marks the deferred IDs in `docs/TRACEABILITY.md` (prompt 005 §2.2, §2.4). Idempotent.

use std::path::Path;

/// Prompt that carries the owner's v1 scope decision.
pub const PROMPT_005: &str = "docs/prompts/005_V1A_SHELL_DAILY_USE.md";

/// Returns the body of the first ````` ````markdown ````` fence after the `## 5. FUTURE.md` heading.
pub fn extract(prompt: &str) -> Option<String> {
    let t = prompt.replace("\r\n", "\n");
    let start = t.find("\n## 5. FUTURE.md")?;
    let rest = t.get(start..)?;
    let open = rest.find("````markdown\n")? + "````markdown\n".len();
    let body = rest.get(open..)?;
    let close = body.find("\n````")?;
    Some(format!("{}\n", body.get(..close)?))
}

/// Expands one "Spec IDs" token (`R-SET-01…04`, `R-REN-10 (tooltip)`, `F.6`) to IDs.
pub fn expand(token: &str) -> Vec<String> {
    let tok = token.split(" (").next().unwrap_or("").trim();
    if let Some((a, b)) = tok.split_once('…') {
        let Some((prefix, from)) = a.rsplit_once('-') else {
            return vec![tok.to_string()];
        };
        let (Ok(f), Ok(l)) = (from.parse::<u32>(), b.parse::<u32>()) else {
            return vec![tok.to_string()];
        };
        let w = from.len();
        return (f..=l).map(|n| format!("{prefix}-{n:0w$}")).collect();
    }
    vec![tok.to_string()]
}

/// `(FUT id, spec IDs)` rows of the FUTURE.md table.
pub fn rows(future: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for line in future.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(id), Some(ids)) = (cells.get(1), cells.get(4)) else {
            continue;
        };
        if !id.starts_with("FUT-") {
            continue;
        }
        let v = ids.split(", ").flat_map(expand).collect();
        out.push(((*id).to_string(), v));
    }
    out
}

/// Applies the deferrals to the traceability table text; returns the new text and unmatched refs.
pub fn apply(trace: &str, rows: &[(String, Vec<String>)]) -> (String, Vec<String>) {
    let mut unmatched = Vec::new();
    let mut lines: Vec<String> = trace.lines().map(str::to_string).collect();
    for (fut, ids) in rows {
        for id in ids {
            let key = format!("| {id} |");
            let Some(line) = lines.iter_mut().find(|l| l.starts_with(&key)) else {
                unmatched.push(format!("{fut}: {id}"));
                continue;
            };
            if line.contains(fut.as_str()) {
                continue;
            }
            let Some(body) = line.strip_suffix(" |") else {
                continue;
            };
            let Some((head, status)) = body.rsplit_once(" | ") else {
                continue;
            };
            let new_status = if let Some(inner) = status
                .strip_prefix("deferred (")
                .and_then(|s| s.strip_suffix(')'))
            {
                format!("deferred ({inner}, {fut})")
            } else if status.starts_with("planned") {
                format!("deferred ({fut})")
            } else if status.contains("part deferred: ") {
                format!("{status}, {fut}")
            } else {
                format!("{status}; part deferred: {fut}")
            };
            *line = format!("{head} | {new_status} |");
        }
    }
    let mut s = lines.join("\n");
    s.push('\n');
    (s, unmatched)
}

/// Runs the extraction and the deferral pass.
pub fn run(root: &Path) -> Result<(), String> {
    let prompt = std::fs::read_to_string(root.join(PROMPT_005)).map_err(|e| e.to_string())?;
    let fut = extract(&prompt).ok_or("FUTURE.md block not found in prompt 005")?;
    std::fs::write(root.join("docs/FUTURE.md"), &fut).map_err(|e| e.to_string())?;
    println!("wrote docs/FUTURE.md");
    let tp = root.join("docs/TRACEABILITY.md");
    let trace = std::fs::read_to_string(&tp).map_err(|e| e.to_string())?;
    let (out, unmatched) = apply(&trace, &rows(&fut));
    std::fs::write(&tp, out).map_err(|e| e.to_string())?;
    println!("updated docs/TRACEABILITY.md");
    for u in unmatched {
        println!("  not a traceability row (section reference): {u}");
    }
    Ok(())
}

/// First line of a `docs/FUTURE.md` written by `cargo xtask future-md` (prompt 022 §3).
pub const GENERATED: &str = "# Possible future developments (open `post-v2` issues)";

/// `spec-check` part: `docs/FUTURE.md` is either generated from the live issues
/// (`cargo xtask future-md`) or equals the historical block extracted from prompt 005.
pub fn check(root: &Path) -> Result<bool, String> {
    let cur = std::fs::read_to_string(root.join("docs/FUTURE.md")).unwrap_or_default();
    if cur.starts_with(GENERATED) {
        return Ok(true);
    }
    let Ok(prompt) = std::fs::read_to_string(root.join(PROMPT_005)) else {
        return Ok(true);
    };
    let fut = extract(&prompt).ok_or("FUTURE.md block not found in prompt 005")?;
    Ok(cur == fut)
}

/// One open issue: number, title, labels.
pub type Issue = (u64, String, Vec<String>);

/// `docs/FUTURE.md` text for the open `post-v2` issues (sorted by number).
pub fn render(repo: &str, issues: &[Issue]) -> String {
    let mut v: Vec<&Issue> = issues.iter().collect();
    v.sort_by_key(|i| i.0);
    let mut out = format!(
        "{GENERATED}\n\n\
         Generated by `cargo xtask future-md` from the open issues of milestone `post-v2` in \
         `{repo}`. The issue tracker is the live list; do not edit this file by hand.\n\n\
         | Issue | Title | Labels |\n|---|---|---|\n"
    );
    for (n, title, labels) in v {
        let t = title.replace('|', "\\|");
        out.push_str(&format!("| #{n} | {t} | {} |\n", labels.join(", ")));
    }
    out
}

/// Parses `gh issue list --json number,title,labels` output.
pub fn parse_issues(json: &str) -> Result<Vec<Issue>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let arr = v.as_array().ok_or("gh: not an array")?;
    Ok(arr
        .iter()
        .filter_map(|i| {
            let n = i.get("number")?.as_u64()?;
            let t = i.get("title")?.as_str()?.to_string();
            let l = i
                .get("labels")
                .and_then(|l| l.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| Some(x.get("name")?.as_str()?.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            Some((n, t, l))
        })
        .collect())
}

fn gh(root: &Path, args: &[&str]) -> Result<String, String> {
    let o = std::process::Command::new("gh")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|e| format!("gh: {e}"))?;
    if !o.status.success() {
        return Err(format!("gh: {}", String::from_utf8_lossy(&o.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&o.stdout).into_owned())
}

/// `cargo xtask future-md`: regenerates `docs/FUTURE.md` from the open `post-v2` issues. An
/// empty result leaves the file unchanged (a wrong repository or login must not wipe it).
pub fn run_md(root: &Path) -> Result<(), String> {
    let repo = gh(
        root,
        &[
            "repo",
            "view",
            "--json",
            "nameWithOwner",
            "-q",
            ".nameWithOwner",
        ],
    )?;
    let repo = repo.trim();
    let json = gh(
        root,
        &[
            "issue",
            "list",
            "--repo",
            repo,
            "--milestone",
            "post-v2",
            "--state",
            "open",
            "--limit",
            "1000",
            "--json",
            "number,title,labels",
        ],
    )?;
    let issues = parse_issues(&json)?;
    if issues.is_empty() {
        return Err(format!(
            "no open post-v2 issues in {repo}; docs/FUTURE.md left unchanged"
        ));
    }
    std::fs::write(root.join("docs/FUTURE.md"), render(repo, &issues))
        .map_err(|e| e.to_string())?;
    println!("wrote docs/FUTURE.md ({} issues)", issues.len());
    Ok(())
}

/// Traceability rows `deferred (FUT-xx …)` whose FUT issue is closed: `(row id, FUT id)`.
pub fn stale_deferrals(trace: &str, closed: &[String]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in trace.lines() {
        let cells: Vec<&str> = line.split('|').map(str::trim).collect();
        let (Some(id), Some(status)) = (cells.get(1), cells.get(3)) else {
            continue;
        };
        let Some(inner) = status.find("deferred (").and_then(|i| status.get(i + 10..)) else {
            continue;
        };
        let inner = inner.split(')').next().unwrap_or("");
        for fut in inner.split(',').map(str::trim) {
            if closed.iter().any(|c| c == fut) {
                out.push(((*id).to_string(), fut.to_string()));
            }
        }
    }
    out
}

/// `spec-check` warning (prompt 022 §3): deferred rows whose FUT issue is closed. Never
/// fails; without `gh` (or offline) it says so and skips.
pub fn warn_closed(root: &Path) {
    let json = gh(
        root,
        &[
            "issue",
            "list",
            "--state",
            "closed",
            "--search",
            "FUT- in:title",
            "--limit",
            "1000",
            "--json",
            "number,title,labels",
        ],
    );
    let Ok(issues) = json.and_then(|j| parse_issues(&j)) else {
        println!("deferred-FUT check: skipped (gh not available)");
        return;
    };
    let closed: Vec<String> = issues
        .iter()
        .filter_map(|(_, t, _)| t.split_once(':').map(|(id, _)| id.trim().to_string()))
        .filter(|id| id.starts_with("FUT-"))
        .collect();
    let trace = std::fs::read_to_string(root.join("docs/TRACEABILITY.md")).unwrap_or_default();
    let stale = stale_deferrals(&trace, &closed);
    if stale.is_empty() {
        println!("deferred-FUT check: no deferred row points to a closed FUT issue");
    }
    for (id, fut) in stale {
        println!("warning: {id} is `deferred ({fut})` but the {fut} issue is closed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_md_and_stale_deferrals() {
        let j = r#"[{"number":7,"title":"FUT-27: a|b","labels":[{"name":"future"}]},{"number":3,"title":"PERF-02: x","labels":[]}]"#;
        let v = parse_issues(j).unwrap_or_default();
        let md = render("o/r", &v);
        assert!(md.starts_with(GENERATED));
        assert!(md.contains("| #3 | PERF-02: x |  |\n| #7 | FUT-27: a\\|b | future |"));
        let t = "| R-A | 1 | deferred (FUT-23) |\n| R-B | 1 | deferred (FUT-01, FUT-47) |\n| R-C | 1 | implemented (1) |\n";
        let closed = vec!["FUT-47".to_string(), "FUT-23".to_string()];
        assert_eq!(
            stale_deferrals(t, &closed),
            vec![
                ("R-A".to_string(), "FUT-23".to_string()),
                ("R-B".to_string(), "FUT-47".to_string())
            ]
        );
    }

    #[test]
    fn ranges_expand() {
        assert_eq!(expand("R-SET-01…04").len(), 4);
        assert_eq!(expand("R-SET-01…04")[3], "R-SET-04");
        assert_eq!(expand("R-REN-10 (tooltip)"), vec!["R-REN-10"]);
    }

    #[test]
    fn deferral_rules() {
        let t = "| A-1 | 004 | planned |\n| A-2 | 003 | implemented (003) |\n";
        let r = vec![
            (
                "FUT-01".to_string(),
                vec!["A-1".into(), "A-2".into(), "F.6".into()],
            ),
            ("FUT-02".to_string(), vec!["A-1".into(), "A-2".into()]),
        ];
        let (o, un) = apply(t, &r);
        assert!(o.contains("| A-1 | 004 | deferred (FUT-01, FUT-02) |"));
        assert!(o.contains("| A-2 | 003 | implemented (003); part deferred: FUT-01, FUT-02 |"));
        assert_eq!(un, vec!["FUT-01: F.6"]);
        let (o2, _) = apply(&o, &r);
        assert_eq!(o, o2, "idempotent");
    }

    #[test]
    fn extracts_block() {
        let p = "x\n## 5. FUTURE.md (y)\n````markdown\n# T\n| FUT-01 | a | b | R-X-01 |\n````\n";
        let f = extract(p).unwrap_or_default();
        assert_eq!(f, "# T\n| FUT-01 | a | b | R-X-01 |\n");
        assert_eq!(rows(&f)[0].1, vec!["R-X-01"]);
    }
}
