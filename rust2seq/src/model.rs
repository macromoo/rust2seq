//! Data model for parsed diagram specs.

use std::path::PathBuf;

/// One sequence diagram, harvested from a `seq::diagram!` declaration plus
/// all the `#[seq::msg]`-annotated fns reachable from its `entry`.
#[derive(Debug, Clone)]
pub struct FlowSpec {
    pub name: String,
    pub title: Option<String>,
    pub participants: Vec<Participant>,
    /// Resolved absolute path the generated `.puml` should be written to.
    pub output: PathBuf,
    /// The conversation tree. Flat sequences of arrows are `Message`
    /// variants; control flow (if/match/for/while/loop) produces `Block`
    /// variants that contain nested events.
    pub events: Vec<FlowEvent>,
}

#[derive(Debug, Clone)]
pub struct Participant {
    pub alias: String,
    pub display: String,
    pub color: Option<String>,
}

/// One element of a flow's conversation tree.
#[derive(Debug, Clone)]
pub enum FlowEvent {
    /// One arrow between two participants.
    Message(MessageSpec),
    /// A control-flow block (alt / loop / opt) wrapping nested events.
    Block(Block),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    /// One branch per arm. For `alt` (if/match), multiple branches; for
    /// `loop`, exactly one. The first branch is emitted with the block's
    /// leading keyword; subsequent branches (alt-only) get `else`.
    pub branches: Vec<Branch>,
}

#[derive(Debug, Clone)]
pub struct Branch {
    /// Human-readable label rendered after the keyword. May be empty if
    /// the source didn't have a meaningful condition.
    pub label: String,
    pub events: Vec<FlowEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// `if`/`match` with multiple arms → `alt ... else ... end`.
    Alt,
    /// `if` without `else` → `opt cond ... end`. Single branch.
    Opt,
    /// `for`/`while`/`loop` → `loop ... end`. Single branch.
    Loop,
}

impl BlockKind {
    pub fn keyword(self) -> &'static str {
        match self {
            Self::Alt => "alt",
            Self::Opt => "opt",
            Self::Loop => "loop",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MessageSpec {
    pub from: String,
    pub to: String,
    pub label: String,
    pub style: ArrowStyle,
    pub note: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowStyle {
    Solid,
    AsyncSolid,
    Dashed,
    AsyncDashed,
}

impl ArrowStyle {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "->" => Some(Self::Solid),
            "->>" => Some(Self::AsyncSolid),
            "-->" => Some(Self::Dashed),
            "-->>" => Some(Self::AsyncDashed),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Solid => "->",
            Self::AsyncSolid => "->>",
            Self::Dashed => "-->",
            Self::AsyncDashed => "-->>",
        }
    }
}

impl Default for ArrowStyle {
    fn default() -> Self {
        Self::Solid
    }
}

