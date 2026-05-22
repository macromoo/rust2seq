//! # rust2seq-driver
//!
//! Custom rustc driver + cargo subcommand that walks the typed AST of an
//! annotated crate and emits PlantUML sequence diagrams.
//!
//! Module breakdown:
//!
//! - [`plugin`] — `impl RustcPlugin for Rust2SeqPlugin` (CLI arg parsing,
//!   driver name, top-level lifecycle)
//! - [`callbacks`] — `impl rustc_driver::Callbacks` (`after_analysis` hook
//!   that hands `TyCtxt` to the rest of the pipeline)
//! - [`discover`] — HIR scan; finds `seq::diagram!` invocations,
//!   `#[seq::msg]` fns, `#[seq::principal]` types, `#[seq::label("...")]`
//!   overrides
//! - [`walker`] — typed-HIR traversal rooted at each diagram's entry point;
//!   emits `MessageSpec`s in execution order; wraps `if`/`match`/loops in
//!   `alt`/`loop` blocks; soft-stop recursion safety net

#![feature(rustc_private)]
#![allow(rustc::diagnostic_outside_of_impl)]
#![allow(rustc::untranslatable_diagnostic)]

extern crate rustc_ast;
extern crate rustc_driver;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_middle;
extern crate rustc_session;
extern crate rustc_span;

pub mod callbacks;
pub mod discover;
pub mod plugin;
pub mod walker;

pub use plugin::Rust2SeqPlugin;
