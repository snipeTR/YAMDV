// SPDX-License-Identifier: AGPL-3.0-or-later
//! `%%{init: {"theme": …, "themeVariables": {…}}}%%` (R-MMD-03 SHOULD): the five Mermaid themes
//! and the variable subset, resolved per theme role when a laid-out drawing is painted.

use yamdv_core::theme::{blend, contrast, luminance};

use crate::draw::{Ink, Role};

/// A parsed `init` directive.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Init {
    /// Role colors to use instead of the app palette.
    pub roles: Vec<(Role, u32)>,
    /// `fontSize` relative to the 14 px default.
    pub font_scale: Option<f32>,
}

const VARS: [(&str, &[Role]); 10] = [
    ("primaryColor", &[Role::NodeFill]),
    (
        "primaryTextColor",
        &[Role::Text, Role::Title, Role::LabelText],
    ),
    ("primaryBorderColor", &[Role::NodeStroke]),
    ("lineColor", &[Role::Edge, Role::Lifeline]),
    ("secondaryColor", &[Role::NoteFill, Role::Activation]),
    ("tertiaryColor", &[Role::ClusterFill]),
    ("clusterBkg", &[Role::ClusterFill]),
    ("clusterBorder", &[Role::ClusterStroke]),
    ("titleColor", &[Role::Title]),
    ("edgeLabelBackground", &[Role::LabelBg]),
];

fn theme(name: &str) -> Option<Vec<(Role, u32)>> {
    use Role::{
        Activation, ClusterFill, ClusterStroke, Edge, Frame, LabelBg, LabelText, Lifeline,
        NodeFill, NodeStroke, NoteFill, NoteStroke, Text, Title,
    };
    // (node fill, node stroke, text, line, cluster fill, cluster stroke, note fill, note stroke,
    // edge-label background)
    let t: [u32; 9] = match name {
        "default" => return Some(Vec::new()),
        "neutral" => [
            0xEEEEEE, 0x999999, 0x333333, 0x666666, 0xF5F5F5, 0xAAAAAA, 0xEAEAEA, 0x999999,
            0xFFFFFF,
        ],
        "dark" => [
            0x1F2020, 0xCCCCCC, 0xE0E0E0, 0xD3D3D3, 0x333333, 0x888888, 0x444444, 0x999999,
            0x585858,
        ],
        "forest" => [
            0xCDE498, 0x13540C, 0x000000, 0x008000, 0xCDFFB2, 0x6EAA49, 0xFFF5AD, 0xAAAA33,
            0xE8E8E8,
        ],
        "base" => [
            0xFFF4DD, 0xD9A45A, 0x333333, 0x333333, 0xFFFFDE, 0xAAAA33, 0xFFF5AD, 0xAAAA33,
            0xE8E8E8,
        ],
        _ => return None,
    };
    Some(vec![
        (NodeFill, t[0]),
        (NodeStroke, t[1]),
        (Text, t[2]),
        (Title, t[2]),
        (LabelText, t[2]),
        (LabelBg, t[8]),
        (Edge, t[3]),
        (Lifeline, t[3]),
        (Frame, t[3]),
        (ClusterFill, t[4]),
        (ClusterStroke, t[5]),
        (NoteFill, t[6]),
        (NoteStroke, t[7]),
        (Activation, t[4]),
    ])
}

/// The value of `key` in a loose JSON object (`"k": "v"`, `'k':'v'`, `k: 12`).
fn value_of(text: &str, key: &str) -> Option<String> {
    for (at, _) in text.match_indices(key) {
        let before = text.get(..at).and_then(|b| b.chars().next_back());
        if before.is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        let rest = text.get(at + key.len()..)?;
        let rest = rest.trim_start_matches(['"', '\'']).trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            continue;
        };
        let rest = rest.trim_start();
        let q = rest.chars().next()?;
        if q == '"' || q == '\'' {
            let body = rest.get(1..)?;
            return body.find(q).and_then(|e| body.get(..e)).map(str::to_string);
        }
        let end = rest
            .find(|c: char| c == ',' || c == '}' || c.is_whitespace())
            .unwrap_or(rest.len());
        return rest.get(..end).map(str::to_string);
    }
    None
}

/// Parses the first `%%{init: …}%%` (or `%%{initialize: …}%%`) directive of `src`.
pub fn init(src: &str) -> Option<Init> {
    let line = src
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("%%{") && l.contains("init"))?;
    let body = line.get(3..)?.trim_end_matches("%%").trim_end_matches('}');
    let mut out = Init::default();
    if let Some(t) = value_of(body, "theme") {
        out.roles = theme(t.trim()).unwrap_or_default();
    }
    for (var, roles) in VARS {
        if let Some(c) = value_of(body, var).and_then(|v| crate::color(&v)) {
            for r in roles {
                out.roles.retain(|(x, _)| x != r);
                out.roles.push((*r, c));
            }
        }
    }
    if let Some(f) = value_of(body, "fontSize") {
        out.font_scale = f
            .trim_end_matches("px")
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|v| v.is_finite())
            .map(|v| (v / 14.0).clamp(0.5, 3.0));
    }
    Some(out)
}

impl Init {
    fn get(&self, r: Role) -> Option<u32> {
        self.roles.iter().find(|(x, _)| *x == r).map(|(_, c)| *c)
    }

    /// The color of `i`: the directive's override, else `app` (the UI palette for `dark`).
    ///
    /// F-44/F-45: under a dark UI a light subgraph or edge-label surface from the directive
    /// falls back to the app's (with its border and text), and a title or label text that
    /// would not read on its surface (contrast < 3) is replaced by one that does.
    pub fn color(&self, i: Ink, dark: bool, app: &dyn Fn(Role) -> u32) -> u32 {
        let r = match i {
            Ink::Rgb(c) | Ink::Band(c, _) => return c,
            Ink::Role(r) => r,
        };
        let surface = match r {
            Role::ClusterFill | Role::ClusterStroke | Role::Title => Role::ClusterFill,
            Role::LabelBg | Role::LabelText => Role::LabelBg,
            _ => return self.get(r).unwrap_or_else(|| app(r)),
        };
        let keep = !(dark && self.get(surface).is_some_and(|c| luminance(c) > 0.4));
        let own = |r: Role| self.get(r).filter(|_| keep).unwrap_or_else(|| app(r));
        let c = own(r);
        if !matches!(r, Role::Title | Role::LabelText) {
            return c;
        }
        let bg = own(surface);
        if contrast(c, bg) >= 3.0 {
            c
        } else if contrast(0x1F2937, bg) >= contrast(0xF3F4F6, bg) {
            0x1F2937
        } else {
            0xF3F4F6
        }
    }
}

/// F-44/F-45 for a sequence `rect` band: the author's color `c` at alpha `a` over the diagram
/// background `bg`; when diagram `text` would read below 4.5:1 on it (e.g. a light band under a
/// dark UI), the band becomes a quiet tint of the author's color over `bg`: at most 20 % of it,
/// less if needed (5 % steps), so the text keeps ≥ 4.5:1 and the bands stay distinguishable.
pub fn band(c: u32, a: u8, bg: u32, text: u32) -> u32 {
    let over = blend(bg, c, u32::from(a) * 1000 / 255);
    if contrast(text, over) >= 4.5 {
        return over;
    }
    let mut t = 200u32;
    let mut out = blend(bg, over, t);
    while contrast(text, out) < 4.5 && t > 0 {
        t = t.saturating_sub(50);
        out = blend(bg, over, t);
    }
    out
}
