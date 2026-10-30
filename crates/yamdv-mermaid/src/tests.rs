// SPDX-License-Identifier: AGPL-3.0-or-later
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::float_cmp
)]

use crate::draw::MockMeasure;
use crate::flow::{Dir, Line, Shape};
use crate::seq::{Event, Frame, NotePos, Tip};
use crate::{Diagram, Failure, parse};

fn flow(src: &str) -> crate::flow::Flow {
    match parse(src) {
        Ok(Diagram::Flow(f)) => f,
        o => panic!("{o:?}"),
    }
}

fn seq(src: &str) -> crate::seq::Seq {
    match parse(src) {
        Ok(Diagram::Seq(s)) => s,
        o => panic!("{o:?}"),
    }
}

fn syntax(src: &str) -> crate::Error {
    match parse(src) {
        Err(Failure::Syntax(e)) => e,
        o => panic!("{o:?}"),
    }
}

#[test]
fn flow_shapes_and_edges() {
    let f = flow(
        "flowchart LR\n  a[Rect] --> b(Round)\n  b --- c([Stadium])\n  c -.-> d[[Sub]]\n  d ==> e[(Db)]\n  e -.- f((Circle))\n  f === g{Rhombus}\n  g --> h>Asym]\n  h --> i[/trap\\]\n  i --> j[\\alt/]\n  j --> k{{Hex}}\n  k --> l[/par/]\n  l --> m[\\alt\\]\n  m --> n(((dbl)))\n  %% comment\n  click a callback\n",
    );
    assert_eq!(f.dir, Dir::LR);
    let shapes: Vec<Shape> = f.nodes.iter().map(|n| n.shape).collect();
    assert_eq!(
        shapes,
        [
            Shape::Rect,
            Shape::Round,
            Shape::Stadium,
            Shape::Subroutine,
            Shape::Cylinder,
            Shape::Circle,
            Shape::Rhombus,
            Shape::Asymmetric,
            Shape::Trapezoid,
            Shape::TrapezoidAlt,
            Shape::Hexagon,
            Shape::Parallelogram,
            Shape::ParallelogramAlt,
            Shape::DoubleCircle
        ]
    );
    assert_eq!(f.nodes[6].label, "Rhombus");
    let kinds: Vec<(Line, bool)> = f
        .edges
        .iter()
        .map(|e| (e.line, e.head == crate::flow::End::Arrow))
        .collect();
    assert_eq!(
        kinds,
        [
            (Line::Solid, true),
            (Line::Solid, false),
            (Line::Dotted, true),
            (Line::Thick, true),
            (Line::Dotted, false),
            (Line::Thick, false),
            (Line::Solid, true),
            (Line::Solid, true),
            (Line::Solid, true),
            (Line::Solid, true),
            (Line::Solid, true),
            (Line::Solid, true),
            (Line::Solid, true)
        ]
    );
    // Edge ends, extra length, linkStyle (default + index, stroke-dasharray).
    use crate::flow::End;
    let f = flow(
        "graph TD\n a --o b\n b --x c\n c <--> d\n d o--o e\n e ---> f\n f -.-> g\n linkStyle default stroke:#f00\n linkStyle 1 stroke-width:4px,stroke-dasharray:5 5\n style a stroke-dasharray:3",
    );
    let ends: Vec<(End, End, u8)> = f.edges.iter().map(|e| (e.tail, e.head, e.len)).collect();
    assert_eq!(
        ends,
        [
            (End::None, End::Circle, 1),
            (End::None, End::Cross, 1),
            (End::Arrow, End::Arrow, 1),
            (End::Circle, End::Circle, 1),
            (End::None, End::Arrow, 2),
            (End::None, End::Arrow, 1)
        ]
    );
    assert_eq!(f.edges[0].style.stroke, Some(0xFF0000));
    assert_eq!(
        (f.edges[1].style.width, f.edges[1].style.dash),
        (Some(4.0), Some(true))
    );
    assert_eq!(f.nodes[0].style.dash, Some(true));
    let d = check_layout("graph TD\n a --o b\n a ---> c\n b --> c");
    assert!(
        d.boxes[2].1 > d.boxes[1].1 + d.boxes[1].3,
        "minlen 2 puts c below b"
    );
}

#[test]
fn flow_labels_chains_groups() {
    let f = flow(
        "graph TD\nA -->|yes| B\nA -- no --> C\nB -. maybe .-> D\nC == sure ==> D\nA --> B & C --> D;E-->F\nX[\"quoted [x]\"]\nY[a<br>b]",
    );
    assert_eq!(f.edges[0].label.as_deref(), Some("yes"));
    assert_eq!(f.edges[1].label.as_deref(), Some("no"));
    assert_eq!(f.edges[1].head, crate::flow::End::Arrow);
    assert_eq!(f.edges[2].label.as_deref(), Some("maybe"));
    assert_eq!(f.edges[2].line, Line::Dotted);
    assert_eq!(f.edges[3].label.as_deref(), Some("sure"));
    assert_eq!(f.edges[3].line, Line::Thick);
    // A --> B & C --> D gives 4 edges, then E-->F.
    assert_eq!(f.edges.len(), 4 + 4 + 1);
    let x = f.nodes.iter().find(|n| n.id == "X").unwrap();
    assert_eq!(x.label, "quoted [x]");
    let y = f.nodes.iter().find(|n| n.id == "Y").unwrap();
    assert_eq!(y.label, "a\nb");
}

#[test]
fn flow_subgraphs_and_styles() {
    let f = flow(
        "flowchart TB\nclassDef hot fill:#f96,stroke:#333,color:#fff\nsubgraph one [First]\n a1 --> a2\n subgraph inner\n  i1\n end\nend\nsubgraph two\n b1:::hot\nend\na2 --> b1\nclass a1 hot\nstyle a2 fill:#0f0\n",
    );
    assert_eq!(f.subs.len(), 3);
    assert_eq!(f.subs[0].title, "First");
    assert_eq!(f.subs[1].parent, Some(0));
    let id = |s: &str| f.nodes.iter().position(|n| n.id == s).unwrap();
    assert_eq!(f.nodes[id("a1")].sub, Some(0));
    assert_eq!(f.nodes[id("i1")].sub, Some(1));
    assert_eq!(f.nodes[id("b1")].sub, Some(2));
    assert_eq!(f.colors(id("b1")).fill, Some(0xFF9966));
    assert_eq!(f.colors(id("b1")).color, Some(0xFFFFFF));
    assert_eq!(f.colors(id("a1")).stroke, Some(0x333333));
    assert_eq!(f.colors(id("a2")).fill, Some(0x00FF00));
}

#[test]
fn flow_errors_have_positions() {
    let e = syntax("flowchart TD\n  A --> B\n  A -->\n");
    assert_eq!((e.line, e.col), (3, 8));
    let e = syntax("graph TD\nA[open --> B\n");
    assert_eq!(e.line, 2);
    let e = syntax("graph XY\n");
    assert_eq!(e.line, 1);
    let e = syntax("graph TD\nsubgraph s\nA\n");
    assert_eq!(e.line, 2);
    let e = syntax("graph TD\nend\n");
    assert_eq!(e.line, 2);
    let e = syntax("graph TD\nA --> B ??\n");
    assert_eq!((e.line, e.col), (2, 9));
}

#[test]
fn unsupported_types() {
    for k in ["timeline", "journey", "gitGraph", "quadrantChart"] {
        assert_eq!(
            parse(&format!("%% c\n{k}\n  title x")),
            Err(Failure::Unsupported(k.into()))
        );
    }
    assert!(matches!(parse(""), Err(Failure::Syntax(_))));
}

#[test]
fn seq_constructs() {
    let s = seq(
        "sequenceDiagram\ntitle: Login\nautonumber\nparticipant A as Alice\nactor B\nA->>+B: solid arrow\nB-->>-A: dotted arrow\nA->B: open\nA-->B: dotted open\nA-xB: cross\nA--xB: dotted cross\nA-)B: async\nA--)B: dotted async\nactivate A\ndeactivate A\nNote left of A: l\nNote right of B: r\nNote over A,B: both\nloop Every minute\nA->>A: self\nend\nalt ok\nA->>B: y\nelse fail\nA->>B: n\nend\nopt maybe\nB->>A: o\nend\npar one\nA->>B: p\nand two\nB->>A: q\nend\n",
    );
    assert_eq!(s.title.as_deref(), Some("Login"));
    assert!(s.autonumber);
    assert_eq!(s.parts[0].label, "Alice");
    assert!(s.parts[1].actor);
    let tips: Vec<(bool, Tip)> = s
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Msg(m) => Some((m.dotted, m.tip)),
            _ => None,
        })
        .take(8)
        .collect();
    assert_eq!(
        tips,
        [
            (false, Tip::Arrow),
            (true, Tip::Arrow),
            (false, Tip::None),
            (true, Tip::None),
            (false, Tip::Cross),
            (true, Tip::Cross),
            (false, Tip::Async),
            (true, Tip::Async)
        ]
    );
    let Event::Msg(m0) = &s.events[0] else {
        panic!()
    };
    assert_eq!(m0.act, Some(true));
    let Event::Msg(m1) = &s.events[1] else {
        panic!()
    };
    assert_eq!(m1.act, Some(false));
    assert!(s.events.iter().any(|e| matches!(
        e,
        Event::Note {
            pos: NotePos::Over,
            b: Some(1),
            ..
        }
    )));
    let frames: Vec<Frame> = s
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Start(f, _) => Some(*f),
            _ => None,
        })
        .collect();
    assert_eq!(frames, [Frame::Loop, Frame::Alt, Frame::Opt, Frame::Par]);
    let d = Diagram::Seq(s).layout(&mut MockMeasure::default(), 1.0);
    assert!(d.w > 0.0 && d.h > 0.0);
    // Head boxes do not overlap.
    for w in d.boxes.windows(2) {
        assert!(w[0].0 + w[0].2 <= w[1].0);
    }
}

/// Prompt 023: `rect` bands (owner example, tests/fixtures/mermaid/seq-rect.md) parse and lay
/// out, nest around `alt`, and keep every other construct of the example.
#[test]
fn seq_rect_owner_example() {
    let md = include_str!("../../../tests/fixtures/mermaid/seq-rect.md");
    let src = md
        .split("```mermaid\n")
        .nth(1)
        .and_then(|b| b.split("```").next())
        .unwrap();
    let s = seq(src);
    assert!(s.autonumber);
    let labels: Vec<&str> = s.parts.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Koordinatör",
            "TEKMER Rota",
            "Google Calendar / Meet",
            "Bildirim Motoru",
            "Girişimci (Mobil)"
        ]
    );
    let rects: Vec<(u32, u8)> = s
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Rect(c, a) => Some((*c, *a)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rects,
        [
            (0xEEF3FB, 255),
            (0xFFFAF0, 255),
            (0xF2FAF5, 255),
            (0xEEF3FB, 255)
        ]
    );
    let alts = s
        .events
        .iter()
        .filter(|e| matches!(e, Event::Start(Frame::Alt, _)))
        .count();
    let ends = s.events.iter().filter(|e| matches!(e, Event::End)).count();
    assert_eq!((alts, ends), (2, 6));
    assert!(s.events.iter().any(|e| matches!(
        e,
        Event::Msg(m) if m.text == "Kesinleştir (K-25: hedef +12s, tavan +24s)" && m.from == m.to
    )));
    assert!(s.events.iter().any(|e| matches!(
        e,
        Event::Msg(m) if m.text == "N-04 · T-15 dk \"Katıl\" (sessiz saat istisnası)" && m.dotted
    )));
    assert!(s.events.contains(&Event::Deactivate(1)));
    assert!(s.events.iter().any(|e| matches!(
        e,
        Event::Msg(m) if m.act == Some(true) && m.from == 0 && m.to == 1
    )));
    assert!(s.events.iter().any(|e| matches!(
        e,
        Event::Msg(m) if m.act == Some(false) && m.from == 2 && m.dotted
    )));
    assert!(s.events.iter().any(|e| matches!(
        e,
        Event::Note { pos: NotePos::Over, a: 0, b: Some(4), text } if text.contains("(≤ 12 saat)")
    )));
    let d = Diagram::Seq(s).layout(&mut MockMeasure::default(), 1.0);
    let bands = d
        .prims
        .iter()
        .filter(|p| matches!(p, crate::draw::Prim::Path(p) if matches!(p.fill, Some(crate::draw::Ink::Band(..)))))
        .count();
    assert_eq!(bands, 4);
    // Bands paint before everything else (behind lifelines, messages and notes).
    assert!(d.prims[..4].iter().all(|p| matches!(p, crate::draw::Prim::Path(p) if matches!(p.fill, Some(crate::draw::Ink::Band(..))))));
    // Nested rects and other color forms.
    let s = seq(
        "sequenceDiagram\nrect rgba(0, 0, 255, .1)\nrect #ffcc00\nA->>B: x\nend\nrect LightBlue\nB-->>A: y\nend\nend\n",
    );
    let rects: Vec<(u32, u8)> = s
        .events
        .iter()
        .filter_map(|e| match e {
            Event::Rect(c, a) => Some((*c, *a)),
            _ => None,
        })
        .collect();
    assert_eq!(rects, [(0x0000FF, 26), (0xFFCC00, 255), (0xADD8E6, 255)]);
    let e = syntax("sequenceDiagram\nrect nocolor\nA->>B: x\nend\n");
    assert_eq!(e.line, 2);
    let e = syntax("sequenceDiagram\nrect rgb(1,2,3)\nA->>B: x\n");
    assert_eq!(e.line, 2);
    // F-44/F-45: a light band under a dark UI is blended toward the background until the
    // light body text reads ≥ 4.5:1; in light mode the author's color stays as written.
    let dark = crate::theme::band(0xEEF3FB, 255, 0x2D2D2D, 0xE5E7EB);
    assert!(yamdv_core::theme::contrast(0xE5E7EB, dark) >= 4.5);
    assert_ne!(dark, 0x2D2D2D);
    assert_eq!(
        crate::theme::band(0xEEF3FB, 255, 0xF5F5F5, 0x333333),
        0xEEF3FB
    );
}

#[test]
fn seq_errors() {
    let e = syntax("sequenceDiagram\nA->>B: x\nelse\n");
    assert_eq!(e.line, 3);
    let e = syntax("sequenceDiagram\nloop x\nA->>B: x\n");
    assert_eq!(e.line, 2);
    let e = syntax("sequenceDiagram\nA=>B: x\n");
    assert_eq!(e.line, 2);
    let e = syntax("sequenceDiagram\nA-~B: x\n");
    assert_eq!((e.line, e.col), (2, 2));
    let e = syntax("sequenceDiagram\nNote above A: x\n");
    assert_eq!(e.line, 2);
}

fn overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    a.0 < b.0 + b.2 - 0.01
        && b.0 < a.0 + a.2 - 0.01
        && a.1 < b.1 + b.3 - 0.01
        && b.1 < a.1 + a.3 - 0.01
}

pub(crate) fn check_layout(src: &str) -> crate::Drawing {
    let d = parse(src).unwrap();
    let (Diagram::Flow(f) | Diagram::Graph(f)) = &d else {
        panic!()
    };
    let out = d.layout(&mut MockMeasure::default(), 1.0);
    assert_eq!(out.boxes.len(), f.nodes.len());
    for i in 0..out.boxes.len() {
        for j in i + 1..out.boxes.len() {
            assert!(
                !overlap(out.boxes[i], out.boxes[j]),
                "{i} {j} {:?}",
                out.boxes
            );
        }
    }
    // Every edge connects its endpoints (starts/ends on the node boxes).
    let near = |p: (f32, f32), b: (f32, f32, f32, f32)| {
        p.0 >= b.0 - 0.5 && p.0 <= b.0 + b.2 + 0.5 && p.1 >= b.1 - 0.5 && p.1 <= b.1 + b.3 + 0.5
    };
    for (e, pts) in f.edges.iter().zip(&out.edges) {
        assert!(pts.len() >= 2);
        assert!(near(pts[0], out.boxes[e.from]), "{pts:?}");
        assert!(near(*pts.last().unwrap(), out.boxes[e.to]), "{pts:?}");
    }
    out
}

#[test]
fn layout_invariants() {
    check_layout("graph TD\nA-->B\nA-->C\nB-->D\nC-->D\nD-->A\nA-->D\nD-->D\nE");
    check_layout(
        "flowchart LR\nsubgraph s1\na-->b\nend\nsubgraph s2\nc-->d\nsubgraph s3\ne\nend\nend\nb-->c\na-->e\nx-->|label|b\n",
    );
    let mut big = String::from("graph TD\n");
    for i in 0..60 {
        big.push_str(&format!(
            "n{i} --> n{}\nn{i} --> n{}\n",
            (i * 7 + 3) % 60,
            (i * 13 + 5) % 60
        ));
    }
    check_layout(&big);
    check_layout("graph BT\nA{Decide}-->B((x))\nA-->C[(db)]\nB-->D([s])\nC-->D[[sub]]");
}

/// F-46/F-47: subgraph boxes are disjoint or nested, nodes never straddle a subgraph border,
/// and everything lies inside the drawing (owner diagram `docs/qa/owner/mermaid-*.png`).
#[test]
fn subgraph_bounds_nest_and_never_overlap() {
    let body = "subgraph CL[Clients]\nMOB[Mobile app]\nWEB[Admin web]\nend\n\
        subgraph CORE[Core monolith]\nGW[API gateway]\nMOD[MOD]\nK[Audit]\n\
        subgraph MODS[Modules]\nM1[Program]\nM2[People]\nM3[Attendance]\nM4[Task]\nend\n\
        BUS[Event bus]\nN[Notify]\nend\n\
        subgraph EXT[External]\nVT[(DB)]\nCAL[Calendar]\nOBJ[(Objects)]\nP[Push]\nQ[(Queue)]\nend\n\
        MOB -->|REST · push token| GW\nWEB -->|REST| GW\nGW --> MOD\nK -.-> MOD\nK -.-> N\n\
        MOD --> VT\nMOD -->|events| BUS\nBUS --> N\nN --> P\nN --> Q\n\
        M1 -->|meet| CAL\nM3 --> OBJ\nM4 --> OBJ\n";
    for dir in ["TB", "LR"] {
        let out = check_layout(&format!("flowchart {dir}\n{body}"));
        let clusters: Vec<(f32, f32, f32, f32)> = out
            .prims
            .iter()
            .filter_map(|p| match p {
                crate::Prim::Path(p)
                    if p.fill == Some(crate::Ink::Role(crate::Role::ClusterFill)) =>
                {
                    let pts: Vec<(f32, f32)> = p
                        .segs
                        .iter()
                        .filter_map(|s| match *s {
                            crate::Seg::M(x, y) | crate::Seg::L(x, y) => Some((x, y)),
                            crate::Seg::C(_, _, _, _, x, y) => Some((x, y)),
                            crate::Seg::Z => None,
                        })
                        .collect();
                    let (x0, y0) = pts
                        .iter()
                        .fold((f32::MAX, f32::MAX), |a, p| (a.0.min(p.0), a.1.min(p.1)));
                    let (x1, y1) = pts
                        .iter()
                        .fold((f32::MIN, f32::MIN), |a, p| (a.0.max(p.0), a.1.max(p.1)));
                    Some((x0, y0, x1 - x0, y1 - y0))
                }
                _ => None,
            })
            .collect();
        assert_eq!(clusters.len(), 4, "{dir}");
        let inside = |a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)| {
            a.0 >= b.0 - 0.01
                && a.1 >= b.1 - 0.01
                && a.0 + a.2 <= b.0 + b.2 + 0.01
                && a.1 + a.3 <= b.1 + b.3 + 0.01
        };
        for (i, &a) in clusters.iter().enumerate() {
            assert!(inside(a, (0.0, 0.0, out.w, out.h)), "{dir} clipped {a:?}");
            for &b in &clusters[i + 1..] {
                assert!(
                    !overlap(a, b) || inside(a, b) || inside(b, a),
                    "{dir} {a:?} {b:?}"
                );
            }
            for &n in &out.boxes {
                assert!(!overlap(n, a) || inside(n, a), "{dir} node {n:?} in {a:?}");
            }
        }
        for &n in &out.boxes {
            assert!(inside(n, (0.0, 0.0, out.w, out.h)), "{dir} clipped {n:?}");
        }
    }
}

#[test]
fn lr_is_rotation_of_tb() {
    // Square nodes (circles) and no labels: LR = TB with x/y swapped.
    let body = "A((A))-->B((B))\nA-->C((C))\nB-->D((D))\nC-->D\nC-->E((E))\n";
    let tb = check_layout(&format!("graph TB\n{body}"));
    let lr = check_layout(&format!("graph LR\n{body}"));
    for (a, b) in tb.boxes.iter().zip(&lr.boxes) {
        let ca = (a.0 + a.2 / 2.0, a.1 + a.3 / 2.0);
        let cb = (b.0 + b.2 / 2.0, b.1 + b.3 / 2.0);
        assert!(
            (ca.0 - cb.1).abs() < 0.5 && (ca.1 - cb.0).abs() < 0.5,
            "{ca:?} {cb:?}"
        );
    }
    assert!((tb.w - lr.h).abs() < 0.5 && (tb.h - lr.w).abs() < 0.5);
}

#[test]
fn scale_shrinks() {
    let d = parse("graph LR\nA[a long label here]-->B[another long label]-->C").unwrap();
    let a = d.layout(&mut MockMeasure::default(), 1.0);
    let b = d.layout(&mut MockMeasure::default(), 0.5);
    assert!((b.w - a.w * 0.5).abs() < 4.0, "{} {}", a.w, b.w);
}

#[test]
fn cycle_heavy_graph_is_bounded() {
    let mut s = String::from("graph TD\n");
    for i in 0..300 {
        s.push_str(&format!("a{i}-->a{}\n", i + 1));
    }
    for i in 0..300 {
        s.push_str(&format!("a300-->a{i}\n"));
    }
    let d = parse(&s).unwrap();
    let out = d.layout(&mut MockMeasure::default(), 1.0);
    assert!(out.complex || out.w > 0.0);
}
