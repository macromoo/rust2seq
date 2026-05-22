//! Unit tests for the PlantUML emitter.

use std::path::PathBuf;

use rust2seq::{
    emit_plantuml, ArrowStyle, Block, BlockKind, Branch, FlowEvent, FlowSpec, MessageSpec,
    Participant, StyleConfig,
};

fn participant(alias: &str, display: &str) -> Participant {
    Participant {
        alias: alias.into(),
        display: display.into(),
        color: None,
    }
}

fn msg(from: &str, to: &str, label: &str, style: ArrowStyle) -> MessageSpec {
    MessageSpec {
        from: from.into(),
        to: to.into(),
        label: label.into(),
        style,
        note: None,
        color: None,
    }
}

fn flow(events: Vec<FlowEvent>) -> FlowSpec {
    FlowSpec {
        name: "demo".into(),
        title: Some("Demo".into()),
        participants: vec![participant("Alice", "Alice"), participant("Bob", "Bob")],
        output: PathBuf::from("/tmp/demo.puml"),
        events,
    }
}

// ---------------------------------------------------------------------------
// minimum: header + participants + one arrow
// ---------------------------------------------------------------------------

#[test]
fn renders_minimum_flow() {
    let f = flow(vec![FlowEvent::Message(msg(
        "Alice",
        "Bob",
        "hello",
        ArrowStyle::Solid,
    ))]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(out.contains("@startuml demo"));
    assert!(out.contains("title Demo"));
    assert!(out.contains("autonumber"));
    assert!(out.contains("participant \"Alice\" as Alice"));
    assert!(out.contains("participant \"Bob\" as Bob"));
    assert!(out.contains("Alice -> Bob : hello"));
    assert!(out.trim_end().ends_with("@enduml"));
}

// ---------------------------------------------------------------------------
// arrow styles
// ---------------------------------------------------------------------------

#[test]
fn renders_all_arrow_styles() {
    let f = flow(vec![
        FlowEvent::Message(msg("Alice", "Bob", "solid", ArrowStyle::Solid)),
        FlowEvent::Message(msg("Alice", "Bob", "async", ArrowStyle::AsyncSolid)),
        FlowEvent::Message(msg("Bob", "Alice", "dashed", ArrowStyle::Dashed)),
        FlowEvent::Message(msg("Bob", "Alice", "async-dash", ArrowStyle::AsyncDashed)),
    ]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(out.contains("Alice -> Bob : solid"));
    assert!(out.contains("Alice ->> Bob : async"));
    assert!(out.contains("Bob --> Alice : dashed"));
    assert!(out.contains("Bob -->> Alice : async-dash"));
}

// ---------------------------------------------------------------------------
// notes
// ---------------------------------------------------------------------------

#[test]
fn renders_note_under_arrow() {
    let mut m = msg("Alice", "Bob", "hello", ArrowStyle::Solid);
    m.note = Some("important context".into());
    let f = flow(vec![FlowEvent::Message(m)]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(out.contains("Alice -> Bob : hello"));
    assert!(out.contains("note over Alice, Bob: important context"));
}

// ---------------------------------------------------------------------------
// alt block
// ---------------------------------------------------------------------------

#[test]
fn renders_alt_block_with_else() {
    let f = flow(vec![FlowEvent::Block(Block {
        kind: BlockKind::Alt,
        branches: vec![
            Branch {
                label: "x > 0".into(),
                events: vec![FlowEvent::Message(msg(
                    "Alice",
                    "Bob",
                    "positive",
                    ArrowStyle::Solid,
                ))],
            },
            Branch {
                label: String::new(),
                events: vec![FlowEvent::Message(msg(
                    "Alice",
                    "Bob",
                    "non-positive",
                    ArrowStyle::Solid,
                ))],
            },
        ],
    })]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(out.contains("alt x > 0"), "missing alt header: {out}");
    assert!(out.contains("    Alice -> Bob : positive"));
    // empty-label else should not print a trailing space or extra text
    let lines: Vec<_> = out.lines().collect();
    assert!(
        lines.iter().any(|l| l.trim_end() == "else"),
        "missing bare `else` line: {out}"
    );
    assert!(out.contains("    Alice -> Bob : non-positive"));
    assert!(out.contains("\nend\n"));
}

// ---------------------------------------------------------------------------
// loop block
// ---------------------------------------------------------------------------

#[test]
fn renders_loop_block() {
    let f = flow(vec![FlowEvent::Block(Block {
        kind: BlockKind::Loop,
        branches: vec![Branch {
            label: "while running".into(),
            events: vec![FlowEvent::Message(msg(
                "Alice",
                "Bob",
                "tick",
                ArrowStyle::Solid,
            ))],
        }],
    })]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(out.contains("loop while running"));
    assert!(out.contains("    Alice -> Bob : tick"));
}

// ---------------------------------------------------------------------------
// nested blocks: alt inside loop
// ---------------------------------------------------------------------------

#[test]
fn renders_nested_blocks() {
    let inner_alt = FlowEvent::Block(Block {
        kind: BlockKind::Alt,
        branches: vec![
            Branch {
                label: "ok".into(),
                events: vec![FlowEvent::Message(msg(
                    "Alice",
                    "Bob",
                    "go",
                    ArrowStyle::Solid,
                ))],
            },
            Branch {
                label: String::new(),
                events: vec![FlowEvent::Message(msg(
                    "Alice",
                    "Bob",
                    "stop",
                    ArrowStyle::Solid,
                ))],
            },
        ],
    });
    let f = flow(vec![FlowEvent::Block(Block {
        kind: BlockKind::Loop,
        branches: vec![Branch {
            label: "forever".into(),
            events: vec![inner_alt],
        }],
    })]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    // depth-1 indent for the alt under loop; depth-2 for arrows inside alt
    assert!(out.contains("loop forever"));
    assert!(out.contains("    alt ok"));
    assert!(out.contains("        Alice -> Bob : go"));
    assert!(out.contains("    else"));
    assert!(out.contains("        Alice -> Bob : stop"));
}

// ---------------------------------------------------------------------------
// multi-line participant display + escaping
// ---------------------------------------------------------------------------

#[test]
fn escapes_multiline_participants_and_labels() {
    let mut f = flow(vec![FlowEvent::Message(msg(
        "Alice",
        "Bob",
        "click \"Log in\"",
        ArrowStyle::Solid,
    ))]);
    f.participants[0].display = "Alice\n(human)".into();
    let out = emit_plantuml(&f, &StyleConfig::default());
    // Newline in display → `\n` in the puml line
    assert!(out.contains("participant \"Alice\\n(human)\" as Alice"));
    // Embedded quote in label → preserved unescaped in arrow label position
    // (plantuml accepts unescaped quotes after the colon)
    assert!(out.contains("click \"Log in\""));
}

// ---------------------------------------------------------------------------
// style overrides
// ---------------------------------------------------------------------------

#[test]
fn style_overrides_propagate_into_skinparam_block() {
    let mut style = StyleConfig::default();
    style.default_font_size = 18;
    style.shadowing = true;
    let f = flow(vec![]);
    let out = emit_plantuml(&f, &style);
    assert!(out.contains("skinparam defaultFontSize 18"));
    assert!(out.contains("skinparam shadowing true"));
}

// ---------------------------------------------------------------------------
// opt block (if without else)
// ---------------------------------------------------------------------------

#[test]
fn renders_opt_block() {
    let f = flow(vec![FlowEvent::Block(Block {
        kind: BlockKind::Opt,
        branches: vec![Branch {
            label: "x > 0".into(),
            events: vec![FlowEvent::Message(msg(
                "Alice",
                "Bob",
                "positive",
                ArrowStyle::Solid,
            ))],
        }],
    })]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(out.contains("opt x > 0"), "missing opt header: {out}");
    assert!(out.contains("    Alice -> Bob : positive"));
    assert!(
        !out.lines().any(|l| l.trim_end() == "else"),
        "opt block must not emit an `else`: {out}"
    );
    assert!(out.contains("\nend\n"));
}

// ---------------------------------------------------------------------------
// participant + message coloring
// ---------------------------------------------------------------------------

#[test]
fn renders_participant_with_color() {
    let mut f = flow(vec![FlowEvent::Message(msg(
        "Alice",
        "Bob",
        "hello",
        ArrowStyle::Solid,
    ))]);
    f.participants[0].color = Some("#ABCDEF".into());
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(
        out.contains("participant \"Alice\" as Alice #ABCDEF"),
        "missing colored participant line: {out}"
    );
    assert!(out.contains("participant \"Bob\" as Bob\n"));
}

#[test]
fn renders_message_with_color() {
    let mut solid = msg("Alice", "Bob", "go", ArrowStyle::Solid);
    solid.color = Some("#FF0000".into());
    let mut dashed = msg("Bob", "Alice", "back", ArrowStyle::Dashed);
    dashed.color = Some("#00AA00".into());
    let f = flow(vec![
        FlowEvent::Message(solid),
        FlowEvent::Message(dashed),
    ]);
    let out = emit_plantuml(&f, &StyleConfig::default());
    assert!(
        out.contains("Alice -[#FF0000]> Bob : go"),
        "missing colored solid arrow: {out}"
    );
    assert!(
        out.contains("Bob -[#00AA00]-> Alice : back"),
        "missing colored dashed arrow: {out}"
    );
}

// ---------------------------------------------------------------------------
// full byte-equality snapshot (smoke for end-to-end emission shape)
// ---------------------------------------------------------------------------

#[test]
fn full_snapshot() {
    let f = FlowSpec {
        name: "demo".into(),
        title: Some("Demo flow".into()),
        participants: vec![participant("A", "Alice"), participant("B", "Bob")],
        output: PathBuf::new(),
        events: vec![
            FlowEvent::Message(msg("A", "B", "ping", ArrowStyle::Solid)),
            FlowEvent::Message(msg("B", "A", "pong", ArrowStyle::Dashed)),
        ],
    };
    let actual = emit_plantuml(&f, &StyleConfig::default());
    let expected = "\
@startuml demo
' !!! GENERATED by rust2seq — do not edit; regenerated on `cargo rust2seq` !!!
title Demo flow
skinparam shadowing false
skinparam roundCorner 8
skinparam defaultFontName \"Helvetica\"
skinparam defaultFontSize 12
skinparam sequenceArrowThickness 1.4
skinparam sequenceMessageAlign center
skinparam responseMessageBelowArrow true
skinparam ParticipantPadding 24
skinparam BoxPadding 12

autonumber

participant \"Alice\" as A
participant \"Bob\" as B

A -> B : ping
B --> A : pong

@enduml
";
    assert_eq!(
        actual, expected,
        "snapshot mismatch:\n--- expected:\n{expected}\n--- actual:\n{actual}"
    );
}
