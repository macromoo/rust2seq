//! `rustc_driver::Callbacks` impl — bridges rustc's compilation lifecycle to
//! our analysis pipeline.

use std::path::{Path, PathBuf};

use rust2seq::{emit_plantuml, Config, FlowSpec};
use rustc_driver::Compilation;
use rustc_interface::interface::Compiler;
use rustc_middle::ty::TyCtxt;

use crate::discover::{self, FlowDecl};
use crate::walker;

pub struct Rust2SeqCallbacks {
    config: Config,
}

impl Rust2SeqCallbacks {
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }
}

impl Default for Rust2SeqCallbacks {
    fn default() -> Self {
        Self::new()
    }
}

impl rustc_driver::Callbacks for Rust2SeqCallbacks {
    fn after_analysis<'tcx>(&mut self, _compiler: &Compiler, tcx: TyCtxt<'tcx>) -> Compilation {
        let report = discover::scan(tcx);
        if report.flows.is_empty() {
            // Nothing rust2seq-annotated in this crate. Stay silent (the
            // driver runs on every workspace crate; only some have flows).
            return Compilation::Continue;
        }

        let crate_root = match local_crate_root(tcx) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("rust2seq: {e}");
                return Compilation::Continue;
            }
        };

        for flow_decl in report.flows.values() {
            if let Err(e) = self.process_flow(tcx, &report, flow_decl, &crate_root) {
                eprintln!("rust2seq: diagram `{}` failed: {e}", flow_decl.name);
            }
        }
        Compilation::Continue
    }
}

impl Rust2SeqCallbacks {
    fn process_flow(
        &self,
        tcx: TyCtxt<'_>,
        report: &discover::DiscoveryReport,
        flow_decl: &FlowDecl,
        crate_root: &Path,
    ) -> std::io::Result<()> {
        let Some(entry_def_id) =
            discover::resolve_entry(tcx, &flow_decl.entry_path, flow_decl.call_site_mod)
        else {
            let mod_path = tcx.def_path_str(flow_decl.call_site_mod);
            eprintln!(
                "rust2seq: could not resolve entry `{}` for diagram `{}` \
                 (looking from module `{mod_path}`) — check the path matches \
                 a fn marked #[seq::msg] in a #[seq::participant] type, or \
                 qualify with `crate::`/`self::`/`super::`",
                flow_decl.entry_path, flow_decl.name,
            );
            return Ok(());
        };

        let outcome = walker::walk(tcx, report, entry_def_id);

        let output = resolve_output(crate_root, flow_decl);
        let flow_spec = FlowSpec {
            name: flow_decl.name.clone(),
            title: flow_decl.title.clone(),
            participants: outcome.participants,
            output: output.clone(),
            events: outcome.events,
        };

        let rendered = emit_plantuml(&flow_spec, &self.config.style);

        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let existing = std::fs::read_to_string(&output).ok();
        if existing.as_deref() == Some(rendered.as_str()) {
            eprintln!("rust2seq: {} (unchanged)", output.display());
        } else {
            std::fs::write(&output, &rendered)?;
            eprintln!("rust2seq: wrote {}", output.display());
        }

        Ok(())
    }
}

/// Resolve the diagram's output path. Absolute paths are kept as-is; relative
/// paths resolve against the crate root (where `Cargo.toml` lives). Missing
/// `output =` defaults to `<crate_root>/diagrams/<name>.puml`.
fn resolve_output(crate_root: &Path, flow: &FlowDecl) -> PathBuf {
    match &flow.output {
        Some(rel) => {
            let p = Path::new(rel);
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                crate_root.join(p)
            }
        }
        None => crate_root
            .join("diagrams")
            .join(format!("{}.puml", flow.name)),
    }
}

fn local_crate_root(tcx: TyCtxt<'_>) -> Result<PathBuf, String> {
    // Use the first source file's dir as a rough proxy. Works for cargo-driven
    // builds where each crate has its own `src/lib.rs` or `src/main.rs`.
    let local_crate = rustc_hir::def_id::LOCAL_CRATE;
    let local_crate_def_id = local_crate.as_def_id();
    let span = tcx.def_span(local_crate_def_id);
    let sm = tcx.sess.source_map();
    let loc = sm.lookup_char_pos(span.lo());
    let file = loc
        .file
        .name
        .prefer_local_unconditionally()
        .to_string_lossy()
        .into_owned();
    let p = PathBuf::from(file);
    // The file is usually `<crate_root>/src/lib.rs` — go up two dirs.
    p.parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .ok_or_else(|| format!("could not derive crate root from {}", p.display()))
}
