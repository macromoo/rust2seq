#![feature(rustc_private)]

// `cargo rust2seq` entry point. Cargo discovers this binary at
// `~/.cargo/bin/cargo-rust2seq` and dispatches to it whenever the user runs
// `cargo rust2seq` from any rust workspace.
//
// `rustc_plugin::cli_main` is responsible for:
// - Parsing our `RustcPlugin::args` impl's CLI args
// - Setting `RUSTC_WORKSPACE_WRAPPER` to point at our sibling `rust2seq-driver`
// - Running `cargo check` so cargo invokes that driver per crate
//
// Cache UX: rustc_plugin uses a separate target dir (`target/plugin-<channel>`),
// but cargo still caches typecheck results in there. Without intervention,
// re-running `cargo rust2seq` against an unchanged source tree does nothing
// — cargo says "up to date" and skips rustc, so our driver never runs.
//
// We solve this by **deleting the per-crate fingerprint dirs in the plugin
// target before delegating**. This forces cargo to re-typecheck every crate
// it would have skipped, which guarantees our driver runs. The crate's
// actual build outputs stay cached, so it's not a full rebuild — just a
// re-check pass. Fast.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    env_logger::init();
    if let Err(e) = invalidate_plugin_fingerprints() {
        // Non-fatal: if we can't find / clean the target dir, fall through
        // to cli_main anyway. Users get the cached-output gotcha but the
        // tool still runs.
        eprintln!("rust2seq: warning — couldn't invalidate cache: {e}");
    }
    rustc_plugin::cli_main(rust2seq_driver::Rust2SeqPlugin)
}

/// Locate `<workspace-target>/plugin-<channel>/debug/.fingerprint` and remove
/// it. Cargo will rebuild the fingerprints from scratch on the next check
/// invocation, which forces rustc to re-run, which lets our driver re-run.
fn invalidate_plugin_fingerprints() -> std::io::Result<()> {
    let metadata = cargo_metadata::MetadataCommand::new()
        .no_deps()
        .other_options(vec!["--offline".to_string()])
        .exec()
        .map_err(|e| std::io::Error::other(e.to_string()))?;

    let target_dir: PathBuf = metadata.target_directory.into();
    let plugin_dir = target_dir.join(format!("plugin-{}", rustc_plugin::CHANNEL));
    let fingerprint = plugin_dir.join("debug").join(".fingerprint");

    if fingerprint.exists() {
        std::fs::remove_dir_all(&fingerprint)?;
    }
    Ok(())
}
