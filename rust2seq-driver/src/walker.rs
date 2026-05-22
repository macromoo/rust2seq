//! HIR walker — traverses the body of an annotated entry fn, building the
//! flow's event tree (arrows and `alt`/`loop` blocks).
//!
//! We use HIR (post-expansion, post-typecheck via the typeck table) rather
//! than THIR because HIR is easier to work with and we get the resolved
//! callee DefId for both UFCS calls (`Foo::bar()`) and method calls
//! (`obj.bar()`) via `typeck_results.type_dependent_def_id(...)`.

use std::collections::HashSet;

use rust2seq::{ArrowStyle, Block, BlockKind, Branch, FlowEvent, MessageSpec, Participant};
use rustc_hir::def::Res;
use rustc_hir::def_id::DefId;
use rustc_hir::{Expr, ExprKind};
use rustc_middle::ty::{TyCtxt, TyKind};

use crate::discover::DiscoveryReport;

/// Output of `walk`: the conversation tree AND the participants discovered
/// during the walk (in first-appearance order, so the diagram columns appear
/// in the order the protocol introduces them).
pub struct WalkOutcome {
    pub events: Vec<FlowEvent>,
    pub participants: Vec<Participant>,
}

/// Walk the entry fn and recursively descend through annotated callees.
pub fn walk(
    tcx: TyCtxt<'_>,
    report: &DiscoveryReport,
    entry_def_id: DefId,
) -> WalkOutcome {
    let mut walker = Walker {
        tcx,
        report,
        active: HashSet::new(),
        events: Vec::new(),
        participants: Vec::new(),
        participant_seen: HashSet::new(),
    };

    // Seed the entry's participant first so it gets the leftmost lane on
    // the diagram regardless of whether the entry's body produces arrows.
    if let Some(p) = walker.participant_def_id_for(entry_def_id) {
        walker.record_participant(p);
    }

    // Walk the entry's body. The body's annotated calls produce all the
    // arrows in the diagram — no synthetic "entry-forward" or "entry-return"
    // arrow is emitted, because the entry fn isn't itself a message between
    // participants; it's the *root* whose body IS the protocol.
    walker.descend(entry_def_id);

    // If the body had no annotated calls at all (and so emitted no events),
    // fall back to a single self-arrow on the entry's participant so the
    // diagram has *something* — better than a blank page.
    if walker.events.is_empty() {
        walker.emit_fallback_self_arrow(entry_def_id);
    }

    WalkOutcome {
        events: walker.events,
        participants: walker.participants,
    }
}

struct Walker<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    report: &'a DiscoveryReport,
    active: HashSet<DefId>,
    /// The current event-collection target. The walker pushes to this; nested
    /// scopes (descends into callees, branch bodies) temporarily swap it out
    /// via `with_scope`.
    events: Vec<FlowEvent>,
    /// Participants discovered during the walk, in first-appearance order.
    participants: Vec<Participant>,
    /// DefIds of participants we've already added to `participants`.
    participant_seen: HashSet<DefId>,
}

impl<'a, 'tcx> Walker<'a, 'tcx> {
    fn push_msg(&mut self, msg: MessageSpec) {
        self.events.push(FlowEvent::Message(msg));
    }

    fn push_block(&mut self, block: Block) {
        // Don't emit empty blocks (zero arrows inside) — they'd just be
        // visual noise on the diagram.
        let has_any = block
            .branches
            .iter()
            .any(|b| !b.events.is_empty());
        if has_any {
            self.events.push(FlowEvent::Block(block));
        }
    }

    /// Run `f` while collecting into a separate event list. Returns whatever
    /// `f` pushed.
    fn with_scope<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> (Vec<FlowEvent>, R) {
        let prev = std::mem::take(&mut self.events);
        let r = f(self);
        let collected = std::mem::replace(&mut self.events, prev);
        (collected, r)
    }

    fn emit_fallback_self_arrow(&mut self, entry_def_id: DefId) {
        let Some(alias) = self.participant_alias_for(entry_def_id) else {
            return;
        };
        let label = self.label_for(entry_def_id);
        self.push_msg(MessageSpec {
            from: alias.clone(),
            to: alias,
            label,
            style: ArrowStyle::Solid,
            note: None,
            color: None,
        });
    }

    fn descend(&mut self, def_id: DefId) {
        if self.active.contains(&def_id) {
            return; // cycle — already handled at call site
        }
        let Some(local_def_id) = def_id.as_local() else {
            return; // out-of-crate fn — no HIR body available
        };
        self.active.insert(def_id);

        let hir = self.tcx.hir_body_owned_by(local_def_id);
        self.walk_expr(&hir.value, def_id);

        self.active.remove(&def_id);
    }

    fn walk_expr(&mut self, expr: &Expr<'tcx>, owner: DefId) {
        match &expr.kind {
            // ---- Calls ----
            ExprKind::Call(callee, args) => {
                if let Some(callee_def_id) = resolve_call_target(self.tcx, callee, owner) {
                    self.maybe_emit_call(owner, callee_def_id);
                }
                self.walk_expr(callee, owner);
                for arg in args.iter() {
                    self.walk_expr(arg, owner);
                }
            }
            ExprKind::MethodCall(_path, recv, args, _span) => {
                if let Some(callee_def_id) = resolve_method_target(self.tcx, expr, owner) {
                    self.maybe_emit_call(owner, callee_def_id);
                }
                self.walk_expr(recv, owner);
                for arg in args.iter() {
                    self.walk_expr(arg, owner);
                }
            }

            // ---- Control flow → alt/loop blocks ----
            ExprKind::If(cond, then_branch, else_branch) => {
                self.walk_expr(cond, owner);
                let cond_label = expr_label(self.tcx, cond);
                let (then_events, _) = self.with_scope(|w| w.walk_expr(then_branch, owner));
                let kind = if else_branch.is_some() {
                    BlockKind::Alt
                } else {
                    BlockKind::Opt
                };
                let mut branches = vec![Branch {
                    label: cond_label,
                    events: then_events,
                }];
                if let Some(e) = else_branch {
                    let (events, _) = self.with_scope(|w| w.walk_expr(e, owner));
                    // Empty label → emitter prints just the `else` keyword
                    // without trailing text (avoids ugly `else else`).
                    branches.push(Branch {
                        label: String::new(),
                        events,
                    });
                }
                self.push_block(Block { kind, branches });
            }
            ExprKind::Match(scrut, arms, _) => {
                self.walk_expr(scrut, owner);
                let scrut_label = expr_label(self.tcx, scrut);
                let mut branches = Vec::with_capacity(arms.len());
                for arm in arms.iter() {
                    let pat_label = pat_label(self.tcx, arm.pat);
                    let (events, _) = self.with_scope(|w| w.walk_expr(arm.body, owner));
                    branches.push(Branch {
                        label: if branches.is_empty() {
                            format!("match {} → {}", scrut_label, pat_label)
                        } else {
                            pat_label
                        },
                        events,
                    });
                }
                self.push_block(Block {
                    kind: BlockKind::Alt,
                    branches,
                });
            }
            ExprKind::Loop(body, label, source, _) => {
                let header = loop_header(*source, label);
                let (events, _) = self.with_scope(|w| {
                    for stmt in body.stmts.iter() {
                        w.walk_stmt(stmt, owner);
                    }
                    if let Some(e) = body.expr {
                        w.walk_expr(e, owner);
                    }
                });
                self.push_block(Block {
                    kind: BlockKind::Loop,
                    branches: vec![Branch {
                        label: header,
                        events,
                    }],
                });
            }

            // ---- Plumbing recursion ----
            ExprKind::Block(block, _) => {
                for stmt in block.stmts.iter() {
                    self.walk_stmt(stmt, owner);
                }
                if let Some(e) = block.expr {
                    self.walk_expr(e, owner);
                }
            }
            ExprKind::Assign(lhs, rhs, _)
            | ExprKind::AssignOp(_, lhs, rhs)
            | ExprKind::Binary(_, lhs, rhs) => {
                self.walk_expr(lhs, owner);
                self.walk_expr(rhs, owner);
            }
            ExprKind::Unary(_, inner)
            | ExprKind::AddrOf(_, _, inner)
            | ExprKind::Cast(inner, _)
            | ExprKind::Field(inner, _)
            | ExprKind::Ret(Some(inner)) => self.walk_expr(inner, owner),
            ExprKind::Tup(items) | ExprKind::Array(items) => {
                for e in items.iter() {
                    self.walk_expr(e, owner);
                }
            }
            ExprKind::Struct(_, fields, _) => {
                for f in fields.iter() {
                    self.walk_expr(f.expr, owner);
                }
            }
            _ => {
                // Leaves — Lit, Path, Closure, etc. Nothing to descend into.
            }
        }
    }

    fn walk_stmt(&mut self, stmt: &rustc_hir::Stmt<'tcx>, owner: DefId) {
        match stmt.kind {
            rustc_hir::StmtKind::Expr(e) | rustc_hir::StmtKind::Semi(e) => {
                self.walk_expr(e, owner);
            }
            rustc_hir::StmtKind::Let(let_stmt) => {
                if let Some(init) = let_stmt.init {
                    self.walk_expr(init, owner);
                }
            }
            _ => {}
        }
    }

    fn maybe_emit_call(&mut self, caller: DefId, callee: DefId) {
        let Some(from_alias) = self.participant_alias_for(caller) else {
            return;
        };
        let Some(to_alias) = self.participant_alias_for(callee) else {
            return;
        };

        if !self.report.msgs.contains_key(&callee) {
            self.descend(callee);
            return;
        }

        let label = self.label_for(callee);
        let cycle = self.active.contains(&callee);
        let note = if cycle {
            Some("↻ recursive — walk did not descend".to_string())
        } else {
            None
        };
        let color = self
            .report
            .msgs
            .get(&callee)
            .and_then(|c| c.clone());

        self.push_msg(MessageSpec {
            from: from_alias.clone(),
            to: to_alias.clone(),
            label,
            style: ArrowStyle::Solid,
            note,
            color: color.clone(),
        });

        if !cycle {
            self.descend(callee);
            if has_response(self.tcx, callee) && from_alias != to_alias {
                self.push_msg(MessageSpec {
                    from: to_alias,
                    to: from_alias,
                    label: return_type_label(self.tcx, callee),
                    style: ArrowStyle::Dashed,
                    note: None,
                    color,
                });
            }
        }
    }

    /// Find the participant type that owns `def_id` (i.e. the type of the
    /// impl block it lives in, or the type itself if `def_id` IS a type).
    /// Returns the participant's DefId only if that type was annotated
    /// `#[seq::participant]`.
    fn participant_def_id_for(&self, def_id: DefId) -> Option<DefId> {
        let parent = self.tcx.parent(def_id);
        let participant_def_id = match self.tcx.def_kind(parent) {
            rustc_hir::def::DefKind::Impl { .. } => {
                let self_ty = self.tcx.type_of(parent).skip_binder();
                if let TyKind::Adt(adt_def, _) = self_ty.kind() {
                    adt_def.did()
                } else {
                    return None;
                }
            }
            // Free fn or the type itself — treat the item as its own participant.
            _ => def_id,
        };
        if self.report.participants.contains_key(&participant_def_id) {
            Some(participant_def_id)
        } else {
            None
        }
    }

    fn participant_alias_for(&mut self, def_id: DefId) -> Option<String> {
        let p = self.participant_def_id_for(def_id)?;
        let alias = self.tcx.item_name(p).as_str().to_string();
        self.record_participant(p);
        Some(alias)
    }

    /// Add a participant to the diagram's lane list if we haven't seen it
    /// yet. First-appearance order = left-to-right order on the diagram.
    fn record_participant(&mut self, participant_def_id: DefId) {
        if !self.participant_seen.insert(participant_def_id) {
            return;
        }
        let alias = self.tcx.item_name(participant_def_id).as_str().to_string();
        let info = self.report.participants.get(&participant_def_id);
        let display = info
            .map(|i| i.display.clone())
            .unwrap_or_else(|| alias.clone());
        let color = info.and_then(|i| i.color.clone());
        self.participants.push(Participant {
            alias,
            display,
            color,
        });
    }

    fn label_for(&self, def_id: DefId) -> String {
        if let Some(custom) = self.report.labels.get(&def_id) {
            return custom.clone();
        }
        let name = self.tcx.item_name(def_id);
        name.as_str().replace('_', " ")
    }

}

fn resolve_call_target(tcx: TyCtxt<'_>, callee_expr: &Expr<'_>, owner: DefId) -> Option<DefId> {
    let local_owner = owner.as_local()?;
    let typeck = tcx.typeck(local_owner);
    if let ExprKind::Path(qpath) = &callee_expr.kind {
        match typeck.qpath_res(qpath, callee_expr.hir_id) {
            Res::Def(_, def_id) => Some(def_id),
            _ => None,
        }
    } else {
        None
    }
}

fn resolve_method_target(tcx: TyCtxt<'_>, expr: &Expr<'_>, owner: DefId) -> Option<DefId> {
    let local_owner = owner.as_local()?;
    let typeck = tcx.typeck(local_owner);
    typeck.type_dependent_def_id(expr.hir_id)
}

fn has_response(tcx: TyCtxt<'_>, def_id: DefId) -> bool {
    let sig = tcx.fn_sig(def_id).skip_binder();
    let output = sig.output().skip_binder();
    !matches!(output.kind(), TyKind::Tuple(t) if t.is_empty())
}

fn return_type_label(tcx: TyCtxt<'_>, def_id: DefId) -> String {
    let sig = tcx.fn_sig(def_id).skip_binder();
    let output = sig.output().skip_binder();
    output.to_string()
}

/// Best-effort source-text reconstruction for an expression. Used to label
/// `if` conditions, `match` scrutinees, etc. Falls back to a placeholder
/// when the source can't be recovered.
fn expr_label(tcx: TyCtxt<'_>, expr: &Expr<'_>) -> String {
    let sm = tcx.sess.source_map();
    sm.span_to_snippet(expr.span)
        .unwrap_or_else(|_| "_".to_string())
}

fn pat_label(tcx: TyCtxt<'_>, pat: &rustc_hir::Pat<'_>) -> String {
    let sm = tcx.sess.source_map();
    sm.span_to_snippet(pat.span)
        .unwrap_or_else(|_| "_".to_string())
}

fn loop_header(source: rustc_hir::LoopSource, label: &Option<rustc_ast::Label>) -> String {
    let kind = match source {
        rustc_hir::LoopSource::Loop => "loop",
        rustc_hir::LoopSource::While => "while",
        rustc_hir::LoopSource::ForLoop => "for",
    };
    if let Some(l) = label {
        format!("{} '{}", kind, l.ident.name)
    } else {
        kind.to_string()
    }
}
