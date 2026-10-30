// SPDX-License-Identifier: AGPL-3.0-or-later
//! Visual flowchart editing: source rewrites (IDEA-05).
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use crate::edit::{FlowEdit, encode_label, op_text};
use crate::flow::{End, Flow, Line};
use crate::{Diagram, parse};

fn fe(src: &str) -> FlowEdit {
    FlowEdit::new(src).unwrap()
}

fn flow(src: &str) -> Flow {
    match parse(src) {
        Ok(Diagram::Flow(f)) => f,
        o => panic!("{o:?}\n{src}"),
    }
}

fn labels(f: &Flow) -> Vec<String> {
    f.nodes
        .iter()
        .map(|n| format!("{}={}", n.id, n.label))
        .collect()
}

fn edges(f: &Flow) -> Vec<String> {
    f.edges
        .iter()
        .map(|e| {
            let l = e.label.as_deref().unwrap_or("");
            format!(
                "{}>{}{}",
                f.nodes[e.from].id,
                f.nodes[e.to].id,
                if l.is_empty() {
                    String::new()
                } else {
                    format!(":{l}")
                }
            )
        })
        .collect()
}

const SRC: &str = "flowchart LR\n    A[Start] --> B{Is it?}\n    B -->|Yes| C[OK]\n    C --> D[Rethink]\n    D --> B\n    B ---->|No| E[End]\n";

#[test]
fn only_flowcharts() {
    assert!(FlowEdit::new("sequenceDiagram\n  A->>B: hi\n").is_none());
    assert!(
        FlowEdit::new("flowchart LR\n  A -->\n").is_none(),
        "syntax error"
    );
    assert!(FlowEdit::new("graph TD\n  A --> B\n").is_some());
}

#[test]
fn node_label_replaced_where_defined() {
    let e = fe(SRC);
    let b = e.node_index("B").unwrap();
    let out = e.set_node_label(b, "Is it ready?").unwrap();
    assert_eq!(out, SRC.replace("{Is it?}", "{Is it ready?}"));
    assert_eq!(flow(&out).nodes[b].label, "Is it ready?");
    // Plain words stay unquoted; a line break becomes <br>.
    let out = e
        .set_node_label(e.node_index("C").unwrap(), "All\ngood")
        .unwrap();
    assert!(out.contains("C[All<br>good]"), "{out}");
    assert_eq!(flow(&out).nodes[2].label, "All\ngood");
    // Quotes inside a label cannot end it.
    let out = e.set_node_label(0, "say \"hi\" [now]").unwrap();
    assert_eq!(flow(&out).nodes[0].label, "say 'hi' [now]");
}

#[test]
fn node_label_added_to_a_bare_node() {
    let src = "graph TD\n  A --> B\n  B --> C\n";
    let e = fe(src);
    let out = e.set_node_label(1, "Second").unwrap();
    assert_eq!(out, "graph TD\n  A --> B[Second]\n  B --> C\n");
    // Empty label: back to the id.
    let out = fe(&out).set_node_label(1, "  ").unwrap();
    assert_eq!(flow(&out).nodes[1].label, "B");
    // A node only named by `style` gets a new statement.
    let src = "graph TD\n  A --> B\n  style Z fill:#f00\n";
    let e = fe(src);
    let z = e.node_index("Z").unwrap();
    let out = e.set_node_label(z, "Zed").unwrap();
    assert!(out.ends_with("\n  Z[Zed]\n"), "{out:?}");
    assert_eq!(flow(&out).nodes[z].label, "Zed");
}

#[test]
fn edge_label_forms() {
    let e = fe(SRC);
    // Piped label: only the text changes.
    let out = e.set_edge_label(1, "Sure").unwrap();
    assert_eq!(out, SRC.replace("|Yes|", "|Sure|"));
    // No label: added after the operator.
    let out = e.set_edge_label(0, "go").unwrap();
    assert!(out.contains("A[Start] -->|go| B"), "{out}");
    assert_eq!(flow(&out).edges[0].label.as_deref(), Some("go"));
    // Removing keeps the operator's length.
    let out = e.set_edge_label(4, "").unwrap();
    assert!(out.contains("B ----> E[End]"), "{out}");
    let f = flow(&out);
    assert_eq!(f.edges[4].label, None);
    assert_eq!(f.edges[4].len, flow(SRC).edges[4].len);
    // `-- text -->` form becomes piped.
    let src = "graph LR\n  A -- old text --> B\n  C -. dotted .-> D\n";
    let e = fe(src);
    let out = e.set_edge_label(0, "new").unwrap();
    assert!(out.contains("A -->|new| B"), "{out}");
    let out = e.set_edge_label(1, "").unwrap();
    assert!(out.contains("C -.-> D"), "{out}");
    let f = flow(&out);
    assert_eq!(f.edges[1].line, Line::Dotted);
    assert_eq!(f.edges[1].label, None);
    // A label with a pipe is quoted.
    let out = e.set_edge_label(0, "a|b").unwrap();
    assert_eq!(flow(&out).edges[0].label.as_deref(), Some("a|b"));
}

#[test]
fn operators_round_trip() {
    for (line, head, tail, len) in [
        (Line::Solid, End::Arrow, End::None, 1),
        (Line::Solid, End::None, End::None, 1),
        (Line::Solid, End::Arrow, End::None, 3),
        (Line::Dotted, End::Arrow, End::None, 1),
        (Line::Dotted, End::None, End::None, 2),
        (Line::Thick, End::Arrow, End::None, 1),
        (Line::Thick, End::None, End::None, 1),
        (Line::Solid, End::Circle, End::Circle, 1),
        (Line::Solid, End::Cross, End::None, 1),
        (Line::Solid, End::Arrow, End::Arrow, 1),
        (Line::Dotted, End::Cross, End::None, 1),
    ] {
        let op = op_text(line, head, tail, len);
        let f = flow(&format!("graph LR\n  A {op} B\n"));
        let e = &f.edges[0];
        assert_eq!(
            (e.line, e.head, e.tail, e.len),
            (line, head, tail, len),
            "{op}"
        );
    }
}

#[test]
fn encode_label_quotes_syntax() {
    assert_eq!(encode_label("plain words"), "plain words");
    assert_eq!(encode_label("a-b"), "\"a-b\"");
    assert_eq!(encode_label(" pad"), "\" pad\"");
    assert_eq!(encode_label("x\ny"), "x<br>y");
}

#[test]
fn add_node_and_edge() {
    let e = fe(SRC);
    let (out, id) = e.add_node(Some(2), "Next step").unwrap();
    assert_eq!(id, "n1");
    assert!(
        out.ends_with("    B ---->|No| E[End]\n    C --> n1[Next step]\n"),
        "{out:?}"
    );
    let f = flow(&out);
    assert_eq!(f.nodes.last().unwrap().label, "Next step");
    assert_eq!(edges(&f).last().unwrap(), "C>n1");
    // A taken id is skipped.
    let e2 = fe("graph TD\n  n1 --> n2\n");
    let (out, id) = e2.add_node(None, "").unwrap();
    assert_eq!(id, "n3");
    assert!(out.ends_with("  n3[n3]\n"), "{out:?}");
    let out = e.add_edge(4, 0).unwrap();
    assert_eq!(edges(&flow(&out)).last().unwrap(), "E>A");
}

#[test]
fn delete_edge_splits_a_chain() {
    let src = "graph LR\n  A[One] --> B[Two] --> C[Three] --> D\n  linkStyle 2 stroke:#f00\n";
    let e = fe(src);
    let out = e.delete_edge(1).unwrap();
    let f = flow(&out);
    assert_eq!(edges(&f), ["A>B", "C>D"]);
    assert_eq!(labels(&f), ["A=One", "B=Two", "C=Three", "D=D"]);
    // The style on old edge 2 (C --> D) follows it to index 1.
    assert!(out.contains("linkStyle 1 stroke:#f00"), "{out}");
    assert_eq!(f.edges[1].style.stroke, Some(0xFF0000));
    assert!(
        out.contains("  A[One] --> B[Two]\n  C[Three] --> D\n"),
        "{out}"
    );
}

#[test]
fn delete_edge_keeps_lone_nodes() {
    let e = fe("graph TD\n  A[Alpha] --> B\n");
    let out = e.delete_edge(0).unwrap();
    let f = flow(&out);
    assert!(f.edges.is_empty());
    assert_eq!(labels(&f), ["A=Alpha", "B=B"]);
    // Groups: only A → C goes; B → C stays.
    let e = fe("graph TD\n  A & B --> C\n");
    let out = e.delete_edge(0).unwrap();
    let f = flow(&out);
    assert_eq!(edges(&f), ["B>C"]);
    assert_eq!(f.nodes.len(), 3, "{out}");
    // Last style index removed with its edge.
    let e = fe("graph TD\n  A --> B\n  B --> C\n  linkStyle 0 stroke:#0f0\n");
    let out = e.delete_edge(0).unwrap();
    assert!(!out.contains("linkStyle"), "{out}");
    assert_eq!(edges(&flow(&out)), ["B>C"]);
}

#[test]
fn delete_node_removes_mentions() {
    let src = "flowchart TD\n  A[Start] --> B[Mid] --> C[End]\n  A --> C\n  class B,C hot\n  style B fill:#ff0\n  click B callback\n  classDef hot fill:#f00\n";
    let e = fe(src);
    let b = e.node_index("B").unwrap();
    let out = e.delete_node(b).unwrap();
    let f = flow(&out);
    assert!(f.nodes.iter().all(|n| n.id != "B"), "{out}");
    assert_eq!(edges(&f), ["A>C"]);
    assert!(out.contains("class C hot"), "{out}");
    assert!(!out.contains("style B"), "{out}");
    assert!(!out.contains("click B"), "{out}");
    // C keeps its label (its shape text was in the removed chain part).
    assert_eq!(f.nodes[e.node_index("C").map(|_| 1).unwrap()].label, "End");
}

#[test]
fn delete_node_in_a_cycle_keeps_the_others() {
    let e = fe("graph LR\n  A --> B\n  B --> A\n  B --> C\n");
    let out = e.delete_node(0).unwrap();
    let f = flow(&out);
    assert_eq!(labels(&f), ["B=B", "C=C"], "{out}");
    assert_eq!(edges(&f), ["B>C"]);
    // Two nodes that only knew each other: B stays as a lone node.
    let e = fe("graph LR\n  A --> B\n  B --> A\n");
    let out = e.delete_node(0).unwrap();
    assert_eq!(labels(&flow(&out)), ["B=B"], "{out}");
}

#[test]
fn statements_on_one_line() {
    let e = fe("graph LR\n  A-->B; B-->C; C-->D\n");
    let out = e.delete_edge(1).unwrap();
    assert_eq!(edges(&flow(&out)), ["A>B", "C>D"], "{out}");
    assert_eq!(flow(&out).nodes.len(), 4, "{out}");
    assert!(out.contains("  A-->B; C-->D"), "{out}");
    let out = e.set_node_label(2, "See").unwrap();
    assert_eq!(flow(&out).nodes[2].label, "See");
}

#[test]
fn crlf_and_subgraphs() {
    let src =
        "flowchart TB\r\n  subgraph one [First]\r\n    a1[x] --> a2\r\n  end\r\n  a2 --> b1\r\n";
    let e = fe(src);
    let out = e.delete_edge(0).unwrap();
    let f = flow(&out);
    assert_eq!(edges(&f), ["a2>b1"]);
    assert_eq!(f.subs.len(), 1);
    assert_eq!(f.nodes[0].sub, Some(0), "{out:?}");
    let out = e.delete_node(1).unwrap();
    let f = flow(&out);
    assert!(f.edges.is_empty(), "{out:?}");
    assert_eq!(labels(&f), ["a1=x", "b1=b1"], "{out:?}");
    assert!(out.contains("\r\n"), "line breaks kept");
}

/// Every rewrite of every node / edge of a varied chart still parses.
#[test]
fn all_rewrites_parse() {
    let src = "flowchart LR\n  A([Start]) -- go --> B[(DB)] & C{{Hex}}\n  B -.-> D>Flag] ==> E((Done))\n  C --x E\n  E o--o A\n  subgraph S [Sub]\n    F[/In/] --- G[\\Out\\]\n  end\n  G --> A\n  linkStyle 0,3 stroke:#00f\n  class A,F big\n";
    let e = fe(src);
    let f = e.flow().clone();
    for i in 0..f.nodes.len() {
        for out in [
            e.set_node_label(i, "new: label").unwrap(),
            e.delete_node(i).unwrap(),
            e.add_node(Some(i), "child").unwrap().0,
        ] {
            assert!(FlowEdit::new(&out).is_some(), "node {i}:\n{out}");
        }
        let d = flow(&e.delete_node(i).unwrap());
        assert_eq!(d.nodes.len(), f.nodes.len() - 1, "node {i}");
    }
    for k in 0..f.edges.len() {
        for out in [
            e.set_edge_label(k, "x").unwrap(),
            e.set_edge_label(k, "").unwrap(),
            e.delete_edge(k).unwrap(),
        ] {
            assert!(FlowEdit::new(&out).is_some(), "edge {k}:\n{out}");
        }
        let d = flow(&e.delete_edge(k).unwrap());
        assert_eq!(d.edges.len(), f.edges.len() - 1, "edge {k}");
        assert_eq!(d.nodes.len(), f.nodes.len(), "edge {k}");
    }
}

#[test]
fn label_input_edits_on_char_boundaries() {
    use crate::edit::LabelInput;
    let mut l = LabelInput::new("Start");
    assert!(l.all);
    l.insert("Çağ");
    assert_eq!((l.text.as_str(), l.caret, l.all), ("Çağ", 5, false));
    l.backspace();
    assert_eq!(l.text, "Ça");
    l.left();
    l.insert("x\n");
    assert_eq!(l.text, "Çxa");
    l.home();
    l.delete();
    assert_eq!(l.text, "xa");
    l.end();
    l.right();
    assert_eq!(l.caret, 2);
    l.select_all();
    l.delete();
    assert_eq!((l.text.as_str(), l.caret), ("", 0));
    l.backspace();
    let mut e = LabelInput::new("");
    assert!(!e.all);
    e.insert("é");
    e.left();
    e.left();
    assert_eq!(e.caret, 0);
}
