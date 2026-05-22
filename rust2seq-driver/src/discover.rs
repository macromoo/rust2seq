//! HIR scan — discovers `seq::diagram!` invocations, `#[seq::participant]`
//! types, `#[seq::msg]` fns, `#[seq::label("...")]` overrides.

use std::collections::{BTreeMap, HashMap};

use rustc_hir::attrs::AttributeKind;
use rustc_hir::def::DefKind;
use rustc_hir::def_id::DefId;
use rustc_hir::intravisit::Visitor;
use rustc_hir::{Attribute, ConstItemRhs, Expr, ExprKind, ImplItem, Item, ItemKind};
use rustc_middle::ty::TyCtxt;

const MARKER_PREFIX: &str = "__rust2seq_marker__:";
const DIAGRAM_CONST_PREFIX: &str = "__RUST2SEQ_DIAGRAM_";

#[derive(Debug, Default)]
pub struct DiscoveryReport {
    /// `seq::diagram!` declarations indexed by `name`.
    pub flows: BTreeMap<String, FlowDecl>,
    /// DefId → display + optional color for every `#[seq::participant]`-marked
    /// type. Display defaults to the type's short name when no
    /// `display = "..."` override is given.
    pub participants: HashMap<DefId, ParticipantInfo>,
    /// DefId → optional color for every `#[seq::msg]`-marked fn / method.
    pub msgs: HashMap<DefId, Option<String>>,
    /// DefId → label override string, from `#[seq::label("...")]`.
    pub labels: HashMap<DefId, String>,
}

#[derive(Debug, Clone)]
pub struct ParticipantInfo {
    pub display: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FlowDecl {
    pub name: String,
    pub title: Option<String>,
    /// Path to the entry fn as written in source (`Browser::start`). The
    /// walker resolves it to a `DefId` once it has full type context.
    pub entry_path: String,
    /// Output path relative to the crate root. `None` ⇒ falls back to
    /// `<crate_root>/diagrams/<name>.puml`.
    pub output: Option<String>,
    /// DefId of the module containing the `seq::diagram!` invocation.
    /// `entry_path` is resolved relative to this module so that
    /// identically-named entries in sibling modules don't collide.
    pub call_site_mod: DefId,
}

pub fn scan(tcx: TyCtxt<'_>) -> DiscoveryReport {
    let mut visitor = DiscoverVisitor {
        tcx,
        report: DiscoveryReport::default(),
    };
    tcx.hir_visit_all_item_likes_in_crate(&mut visitor);
    visitor.report
}

struct DiscoverVisitor<'tcx> {
    tcx: TyCtxt<'tcx>,
    report: DiscoveryReport,
}

impl<'tcx> Visitor<'tcx> for DiscoverVisitor<'tcx> {
    fn visit_item(&mut self, item: &'tcx Item<'tcx>) -> Self::Result {
        let def_id = item.owner_id.to_def_id();

        // (1) Diagram declarations come through as hidden consts named
        //     `__RUST2SEQ_DIAGRAM_<n>: &str = "<original-body>"`.
        if let ItemKind::Const(ident, _, _, ConstItemRhs::Body(body_id)) = &item.kind {
            let name = ident.as_str();
            if name.starts_with(DIAGRAM_CONST_PREFIX) {
                if let Some(body_str) = extract_str_literal(self.tcx, *body_id) {
                    let call_site_mod = self.tcx.parent(def_id);
                    match parse_flow_decl(&body_str, call_site_mod) {
                        Ok(flow) => {
                            self.report.flows.entry(flow.name.clone()).or_insert(flow);
                        }
                        Err(e) => {
                            eprintln!("rust2seq: bad diagram! body — {e} (at {:?})", item.span);
                        }
                    }
                }
            }
        }

        // (2) Marker doc attrs on participants / msgs / labels.
        let attrs = self.tcx.hir_attrs(item.hir_id());
        for marker in attrs.iter().filter_map(extract_marker) {
            self.record_marker(def_id, &marker);
        }

        rustc_hir::intravisit::walk_item(self, item)
    }

    fn visit_impl_item(&mut self, item: &'tcx ImplItem<'tcx>) -> Self::Result {
        let def_id = item.owner_id.to_def_id();
        let attrs = self.tcx.hir_attrs(item.hir_id());
        for marker in attrs.iter().filter_map(extract_marker) {
            self.record_marker(def_id, &marker);
        }
        rustc_hir::intravisit::walk_impl_item(self, item)
    }
}

impl<'tcx> DiscoverVisitor<'tcx> {
    fn record_marker(&mut self, def_id: DefId, marker: &str) {
        match marker.split_once(':') {
            Some(("participant", payload)) => {
                let pairs = split_kv_pairs(payload);
                let display = lookup_str(&pairs, "display")
                    .map(|s| s.replace("\\n", "\n"))
                    .unwrap_or_else(|| self.tcx.item_name(def_id).as_str().to_string());
                let color = lookup_str(&pairs, "color");
                self.report
                    .participants
                    .insert(def_id, ParticipantInfo { display, color });
            }
            Some(("msg", payload)) => {
                let pairs = split_kv_pairs(payload);
                let color = lookup_str(&pairs, "color");
                self.report.msgs.insert(def_id, color);
            }
            Some(("label", payload)) => {
                if let Some(text) = parse_label_payload(payload) {
                    self.report.labels.insert(def_id, text);
                }
            }
            _ => {}
        }
    }
}

/// If `attr` is a `#[doc = "__rust2seq_marker__:...."]` doc-comment-style attr,
/// return the substring after the marker prefix.
fn extract_marker(attr: &Attribute) -> Option<String> {
    let Attribute::Parsed(AttributeKind::DocComment { comment, .. }) = attr else {
        return None;
    };
    comment
        .as_str()
        .strip_prefix(MARKER_PREFIX)
        .map(|s| s.to_string())
}

/// Pull the string-literal initializer out of a `const X: &str = "..."` body.
fn extract_str_literal(tcx: TyCtxt<'_>, body_id: rustc_hir::BodyId) -> Option<String> {
    let body = tcx.hir_body(body_id);
    let Expr {
        kind: ExprKind::Lit(lit),
        ..
    } = body.value
    else {
        return None;
    };
    let rustc_ast::LitKind::Str(sym, _) = lit.node else {
        return None;
    };
    Some(sym.as_str().to_string())
}

/// Look up a key in the kv-pair list (post-`split_kv_pairs`) and unwrap its
/// string-literal value. Returns `None` if the key is missing or its value
/// isn't a quoted string.
fn lookup_str(pairs: &[(String, String)], key: &str) -> Option<String> {
    let raw = pairs.iter().find(|(k, _)| k == key)?.1.trim();
    let after_quote = raw.strip_prefix('"')?;
    let end = after_quote.rfind('"')?;
    Some(after_quote[..end].to_string())
}

fn parse_label_payload(payload: &str) -> Option<String> {
    let trimmed = payload.trim();
    let after_quote = trimmed.strip_prefix('"')?;
    let end = after_quote.rfind('"')?;
    Some(after_quote[..end].to_string())
}

fn parse_flow_decl(body: &str, call_site_mod: DefId) -> Result<FlowDecl, String> {
    let mut name = None;
    let mut title = None;
    let mut entry = None;
    let mut output = None;

    for (key, value) in split_kv_pairs(body) {
        match key.as_str() {
            "name" => name = Some(parse_str_value(&value)?),
            "title" => title = Some(parse_str_value(&value)?),
            "entry" => entry = Some(value.trim().to_string()),
            "output" => output = Some(parse_str_value(&value)?),
            other => {
                return Err(format!(
                    "unknown diagram! key `{other}` (allowed: name, title, entry, output)"
                ))
            }
        }
    }

    Ok(FlowDecl {
        name: name.ok_or_else(|| "diagram! missing required `name`".to_string())?,
        title,
        entry_path: entry.ok_or_else(|| "diagram! missing required `entry`".to_string())?,
        output,
        call_site_mod,
    })
}

/// Naive comma-splitting that respects bracket depth so we don't split inside
/// `participants = [ ... ]` or tuples.
fn split_kv_pairs(body: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut in_str = false;
    let mut start = 0;
    let bytes = body.as_bytes();

    let push_pair = |chunk: &str, out: &mut Vec<(String, String)>| {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            return;
        }
        if let Some((k, v)) = chunk.split_once('=') {
            out.push((k.trim().to_string(), v.trim().to_string()));
        }
    };

    for i in 0..bytes.len() {
        let c = bytes[i] as char;
        match c {
            '"' if !is_escaped(bytes, i) => in_str = !in_str,
            '[' | '(' | '{' if !in_str => depth += 1,
            ']' | ')' | '}' if !in_str => depth -= 1,
            ',' if depth == 0 && !in_str => {
                push_pair(&body[start..i], &mut out);
                start = i + 1;
            }
            _ => {}
        }
    }
    push_pair(&body[start..], &mut out);
    out
}

fn is_escaped(bytes: &[u8], i: usize) -> bool {
    i > 0 && bytes[i - 1] == b'\\'
}

fn parse_str_value(s: &str) -> Result<String, String> {
    let trimmed = s.trim();
    let after_quote = trimmed
        .strip_prefix('"')
        .ok_or_else(|| format!("expected string literal, got `{trimmed}`"))?;
    let end = after_quote
        .rfind('"')
        .ok_or_else(|| format!("unterminated string in `{trimmed}`"))?;
    Ok(after_quote[..end].to_string())
}

/// Resolve a diagram's `entry_path` (`"Browser::start"`, `"crate::a::Foo::bar"`,
/// `"self::Foo::bar"`, `"super::Foo::bar"`) to a `DefId`. Unqualified paths
/// are resolved relative to `call_site_mod` so that identically-named entries
/// in sibling modules don't collide.
pub fn resolve_entry(tcx: TyCtxt<'_>, entry_path: &str, call_site_mod: DefId) -> Option<DefId> {
    let target = normalize_entry_path(tcx, entry_path, call_site_mod)?;
    for local_def_id in tcx.hir_crate_items(()).definitions() {
        let def_id: DefId = local_def_id.to_def_id();
        if !matches!(tcx.def_kind(def_id), DefKind::Fn | DefKind::AssocFn) {
            continue;
        }
        if tcx.def_path_str(def_id) == target {
            return Some(def_id);
        }
    }
    None
}

fn normalize_entry_path(tcx: TyCtxt<'_>, entry_path: &str, call_site_mod: DefId) -> Option<String> {
    let mut segments: Vec<&str> = entry_path.split("::").map(str::trim).collect();
    if segments.is_empty() || segments.iter().any(|s| s.is_empty()) {
        return None;
    }
    let mut base: Vec<String> = match segments[0] {
        "crate" => {
            segments.remove(0);
            Vec::new()
        }
        "self" => {
            segments.remove(0);
            module_local_segs(tcx, call_site_mod)
        }
        "super" => {
            let mut p = module_local_segs(tcx, call_site_mod);
            while segments.first() == Some(&"super") {
                segments.remove(0);
                p.pop()?;
            }
            p
        }
        _ => module_local_segs(tcx, call_site_mod),
    };
    base.extend(segments.iter().map(|s| s.to_string()));
    Some(base.join("::"))
}

fn module_local_segs(tcx: TyCtxt<'_>, mod_def_id: DefId) -> Vec<String> {
    // The crate root has no meaningful path segments. Detect it directly via
    // DefId rather than parsing def_path_str — rustc emits the crate-root mod
    // as the empty string here, but that's an internal detail we shouldn't lean
    // on. Nested local modules are emitted as `foo::bar` without a crate prefix.
    if mod_def_id == rustc_hir::def_id::LOCAL_CRATE.as_def_id() {
        return Vec::new();
    }
    let s = tcx.def_path_str(mod_def_id);
    if s.is_empty() {
        Vec::new()
    } else {
        s.split("::").map(|x| x.to_string()).collect()
    }
}
