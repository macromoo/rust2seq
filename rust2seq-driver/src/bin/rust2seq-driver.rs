#![feature(rustc_private)]

// The rustc-replacement binary. cargo invokes this once per crate it would
// have compiled, via the `RUSTC_WRAPPER` env var set by `cargo-rust2seq`.
//
// `rustc_plugin::driver_main` runs `rustc_driver::run_compiler` under the
// hood, with our plugin's `run` method getting first crack at the typed AST.
fn main() -> std::process::ExitCode {
    env_logger::init();
    rustc_plugin::driver_main(rust2seq_driver::Rust2SeqPlugin)
}
