//! `RustcPlugin` impl — the plugin framework's entry point.

use std::borrow::Cow;
use std::env;
use std::process::Command;

use clap::Parser;
use rustc_plugin::{CrateFilter, RustcPlugin, RustcPluginArgs, Utf8Path};
use serde::{Deserialize, Serialize};

use crate::callbacks::Rust2SeqCallbacks;

/// The plugin handle the framework hands off to the CLI / driver binaries.
pub struct Rust2SeqPlugin;

/// CLI args. Serialized + handed across the cargo-subcommand → driver
/// process boundary by `rustc_plugin`, so derives Serialize+Deserialize.
#[derive(Parser, Serialize, Deserialize, Clone, Debug)]
#[command(name = "cargo-rust2seq", about = "Generate PlantUML sequence diagrams from typed Rust")]
pub struct Rust2SeqArgs {
    /// Extra args forwarded to the underlying `cargo check` invocation
    /// (e.g. `-- --features foo`).
    #[clap(last = true)]
    pub cargo_args: Vec<String>,
}

impl RustcPlugin for Rust2SeqPlugin {
    type Args = Rust2SeqArgs;

    fn version(&self) -> Cow<'static, str> {
        env!("CARGO_PKG_VERSION").into()
    }

    fn driver_name(&self) -> Cow<'static, str> {
        "rust2seq-driver".into()
    }

    fn args(&self, _target_dir: &Utf8Path) -> RustcPluginArgs<Self::Args> {
        // Skip the leading "rust2seq" subcommand arg cargo prepends.
        let args = Rust2SeqArgs::parse_from(env::args().skip(1));
        RustcPluginArgs {
            args,
            filter: CrateFilter::OnlyWorkspace,
        }
    }

    fn modify_cargo(&self, cargo: &mut Command, args: &Self::Args) {
        cargo.args(&args.cargo_args);
    }

    fn run(
        self,
        compiler_args: Vec<String>,
        _plugin_args: Self::Args,
    ) -> rustc_interface::interface::Result<()> {
        let mut callbacks = Rust2SeqCallbacks::new();
        rustc_driver::run_compiler(&compiler_args, &mut callbacks);
        Ok(())
    }
}
