//! # rust2seq
//!
//! Facade crate. Two responsibilities:
//!
//! 1. **Re-export the user-facing macros** from `rust2seq-macros`. Any crate
//!    that wants to annotate flows depends on `rust2seq` (not the macros
//!    crate).
//!
//! 2. **Define the shared data model and output format** consumed by the
//!    `rust2seq-driver` crate.

pub use rust2seq_macros::{diagram, label, msg, participant};

mod config;
mod emit;
mod model;

pub use config::{Config, StyleConfig};
pub use emit::emit_plantuml;
pub use model::{
    ArrowStyle, Block, BlockKind, Branch, FlowEvent, FlowSpec, MessageSpec, Participant,
};
