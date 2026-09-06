//! Control Flow Graph — CfgBuilder + relooper pipeline.
//! Pipeline: C statements → CfgBuilder → Cfg → relooper → structures → DaStmt

use crate::c_ast::*;
use crate::diagnostics::TranslationResult;
use crate::translator::*;
use crate::with_stmts::WithStmts;
use crate::translator::value_lowering::ValueSite;
use das_ast::{DaBlock, DaExpr, DaStmt, DaType, DaTypeKind};
use indexmap::{indexset, IndexMap, IndexSet};
use std::collections::hash_map::DefaultHasher;
use std::fmt::{self, Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

pub mod inc_cleanup;
pub mod labels;
pub mod loops;
pub mod multiples;
pub mod relooper;
pub mod structures;

use crate::cfg::inc_cleanup::IncCleanup;
use crate::cfg::loops::*;
use crate::cfg::multiples::*;

// ===== Types (shared with submodules) =====

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Label {
    FromC(CLabelId, Option<Rc<str>>),
    Synthetic(u64),
}
impl Display for Label {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::FromC(_, Some(n)) => write!(f, "_{n}"),
            Self::FromC(id, None) => write!(f, "c_{}", id.0),
            Self::Synthetic(id) => write!(f, "s_{id}"),
        }
    }
}
impl fmt::Debug for Label {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        Display::fmt(self, f)
    }
}
impl Label {
    pub fn pretty_print(&self) -> String {
        self.to_string()
    }
    fn debug_print(&self) -> String {
        self.pretty_print().trim_start_matches('\'').to_string()
    }
    fn to_num_expr(&self) -> DaExpr {
        let mut s = DefaultHasher::new();
        self.hash(&mut s);
        DaExpr::ConstUInt(s.finish())
    }
}

#[derive(Clone, Debug)]
pub enum StructureLabel<S> {
    GoTo(Label),
    ExitTo(Label),
    Nested(Vec<Structure<S>>),
}

#[derive(Clone, Debug)]
pub enum Structure<Stmt> {
    Simple {
        entries: IndexSet<Label>,
        body: Vec<Stmt>,
        terminator: GenTerminator<StructureLabel<Stmt>>,
    },
    Loop {
        entries: IndexSet<Label>,
        body: Vec<Structure<Stmt>>,
    },
    Multiple {
        entries: IndexSet<Label>,
        branches: IndexMap<Label, Vec<Structure<Stmt>>>,
    },
}

#[derive(Clone, Debug)]
pub struct BasicBlock<L, S> {
    pub body: Vec<S>,
    pub terminator: GenTerminator<L>,
    pub live: IndexSet<CDeclId>,
    pub defined: IndexSet<CDeclId>,
}

#[derive(Clone, Debug)]
pub enum GenTerminator<Lbl> {
    End,
    Jump(Lbl),
    Branch(DaExpr, Lbl, Lbl),
    Switch {
        expr: DaExpr,
        cases: Vec<(DaExpr, Lbl)>,
    },
}
use self::GenTerminator::*;

impl<L> GenTerminator<L> {
    pub fn map_labels<F: Fn(&L) -> N, N>(&self, f: F) -> GenTerminator<N> {
        match self {
            End => End,
            Jump(l) => Jump(f(l)),
            Branch(e, l1, l2) => Branch(e.clone(), f(l1), f(l2)),
            Switch { expr, cases } => Switch {
                expr: expr.clone(),
                cases: cases.iter().map(|(e, l)| (e.clone(), f(l))).collect(),
            },
        }
    }
    pub fn get_labels(&self) -> Vec<&L> {
        match self {
            End => vec![],
            Jump(l) => vec![l],
            Branch(_, l1, l2) => vec![l1, l2],
            Switch { cases, .. } => cases.iter().map(|(_, l)| l).collect(),
        }
    }
    pub fn get_labels_mut(&mut self) -> Vec<&mut L> {
        match self {
            End => vec![],
            Jump(l) => vec![l],
            Branch(_, l1, l2) => vec![l1, l2],
            Switch { cases, .. } => cases.iter_mut().map(|(_, l)| l).collect(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum StmtOrDecl {
    Stmt(DaStmt),
    Decl(CDeclId),
}
impl StmtOrDecl {
    pub fn place_decls(self, lf: &IndexSet<CDeclId>, st: &mut DeclStmtStore) -> Vec<DaStmt> {
        match self {
            StmtOrDecl::Stmt(s) => vec![s],
            StmtOrDecl::Decl(d) if lf.contains(&d) => {
                st.extract_assign(d).unwrap().into_iter().collect()
            }
            StmtOrDecl::Decl(d) => st.extract_decl_and_assign(d).unwrap().into_iter().collect(),
        }
    }
}
impl Structure<StmtOrDecl> {
    pub fn place_decls(self, lf: &IndexSet<CDeclId>, st: &mut DeclStmtStore) -> Structure<DaStmt> {
        match self {
            Structure::Simple {
                entries,
                body,
                terminator,
            } => Structure::Simple {
                entries,
                body: body
                    .into_iter()
                    .flat_map(|s| s.place_decls(lf, st))
                    .collect(),
                terminator: terminator.place_decls(lf, st),
            },
            Structure::Loop { entries, body } => Structure::Loop {
                entries,
                body: body.into_iter().map(|s| s.place_decls(lf, st)).collect(),
            },
            Structure::Multiple { entries, branches } => Structure::Multiple {
                entries,
                branches: branches
                    .into_iter()
                    .map(|(l, vs)| (l, vs.into_iter().map(|s| s.place_decls(lf, st)).collect()))
                    .collect(),
            },
        }
    }
}
impl GenTerminator<StructureLabel<StmtOrDecl>> {
    pub fn place_decls(
        self,
        lf: &IndexSet<CDeclId>,
        st: &mut DeclStmtStore,
    ) -> GenTerminator<StructureLabel<DaStmt>> {
        match self {
            End => End,
            Jump(l) => Jump(l.place_decls(lf, st)),
            Branch(e, l1, l2) => Branch(e, l1.place_decls(lf, st), l2.place_decls(lf, st)),
            Switch { expr, cases } => Switch {
                expr,
                cases: cases
                    .into_iter()
                    .map(|(e, l)| (e, l.place_decls(lf, st)))
                    .collect(),
            },
        }
    }
}
impl StructureLabel<StmtOrDecl> {
    pub fn place_decls(
        self,
        lf: &IndexSet<CDeclId>,
        st: &mut DeclStmtStore,
    ) -> StructureLabel<DaStmt> {
        match self {
            StructureLabel::GoTo(l) => StructureLabel::GoTo(l),
            StructureLabel::ExitTo(l) => StructureLabel::ExitTo(l),
            StructureLabel::Nested(vs) => {
                StructureLabel::Nested(vs.into_iter().map(|s| s.place_decls(lf, st)).collect())
            }
        }
    }
}
impl<S1, S2> BasicBlock<StructureLabel<S1>, S2> {
    pub fn successors(&self) -> IndexSet<Label> {
        self.terminator
            .get_labels()
            .iter()
            .filter_map(|sl| match sl {
                StructureLabel::GoTo(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct Cfg<Lbl: Ord + Hash, Stmt> {
    pub entries: Lbl,
    pub nodes: IndexMap<Lbl, BasicBlock<Lbl, Stmt>>,
    pub loops: LoopInfo<Lbl>,
    pub multiples: MultipleInfo<Lbl>,
    /// Declarations that must precede the whole graph because a block may be
    /// reached before the block that introduces them (see the `switch`
    /// scrutinee temporary in `CfgBuilder`).
    pub prelude: Vec<DaStmt>,
}

#[derive(Clone, Debug)]
pub enum ImplicitReturnType {
    Main,
    Void,
    NoImplicitReturnType,
    StmtExpr(ExprContext, CExprId, Label),
    StmtExprVoid,
}
/// Case labels collected while the body of one C `switch` is being converted.
///
/// `cases` keeps every `case` value in source order together with the block that
/// the value dispatches to; `default` is the block of the (at most one) `default`
/// label, wherever in the body it appeared.  A `switch` terminator is only built
/// once the whole body has been walked, because C allows `case` labels to appear
/// arbitrarily deep inside the body (Duff's device).
#[derive(Clone, Debug, Default)]
pub struct SwitchCases {
    cases: Vec<(DaExpr, Label)>,
    default: Option<Label>,
}
#[derive(Clone, Debug, Default)]
pub struct DeclStmtStore {
    store: IndexMap<CDeclId, DeclStmtInfo>,
}
#[derive(Clone, Debug)]
pub struct DeclStmtInfo {
    pub decl: Option<Vec<DaStmt>>,
    pub assign: Option<Vec<DaStmt>>,
    pub decl_and_assign: Option<Vec<DaStmt>>,
}
impl DeclStmtInfo {
    pub fn new(d: Vec<DaStmt>, a: Vec<DaStmt>, da: Vec<DaStmt>) -> Self {
        Self {
            decl: Some(d),
            assign: Some(a),
            decl_and_assign: Some(da),
        }
    }
    pub fn empty() -> Self {
        Self {
            decl: Some(vec![]),
            assign: Some(vec![]),
            decl_and_assign: Some(vec![]),
        }
    }
}
impl DeclStmtStore {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn absorb(&mut self, o: Self) {
        self.store.extend(o.store);
    }
    pub fn extract_decl(&mut self, id: CDeclId) -> TranslationResult<Vec<DaStmt>> {
        let DeclStmtInfo {
            decl,
            assign,
            ..
        } = self
            .store
            .swap_remove(&id)
            .ok_or_else(|| TranslationError::generic("decl info not found"))?;

        let decl = decl.ok_or_else(|| TranslationError::generic("decl already extracted"))?;

        self.store.insert(
            id,
            DeclStmtInfo {
                decl: None,
                assign,
                decl_and_assign: None,
            },
        );
        Ok(decl)
    }
    pub fn extract_assign(&mut self, id: CDeclId) -> TranslationResult<Vec<DaStmt>> {
        let DeclStmtInfo {
            decl,
            assign,
            ..
        } = self
            .store
            .swap_remove(&id)
            .ok_or_else(|| TranslationError::generic("assign info not found"))?;

        let assign =
            assign.ok_or_else(|| TranslationError::generic("assign already extracted"))?;

        self.store.insert(
            id,
            DeclStmtInfo {
                decl,
                assign: None,
                decl_and_assign: None,
            },
        );
        Ok(assign)
    }
    pub fn extract_decl_and_assign(&mut self, id: CDeclId) -> TranslationResult<Vec<DaStmt>> {
        let DeclStmtInfo {
            decl_and_assign, ..
        } = self
            .store
            .swap_remove(&id)
            .ok_or_else(|| TranslationError::generic("decl+assign info not found"))?;

        let decl_and_assign = decl_and_assign
            .ok_or_else(|| TranslationError::generic("decl+assign already extracted"))?;

        self.store.insert(
            id,
            DeclStmtInfo {
                decl: None,
                assign: None,
                decl_and_assign: None,
            },
        );
        Ok(decl_and_assign)
    }
}

// ===== CfgBuilder =====

/// Builds a CFG from C statements. Each label/goto boundary splits into a BasicBlock.
struct CfgBuilder {
    nodes: IndexMap<Label, BasicBlock<Label, StmtOrDecl>>,
    decls_seen: DeclStmtStore,
    loops: LoopInfo<Label>,
    multiples: MultipleInfo<Label>,
    break_labels: Vec<Label>,
    continue_labels: Vec<Label>,
    /// One entry per `switch` whose body is currently being converted.
    /// `CStmtKind::Case`/`CStmtKind::Default` register into the innermost one.
    switch_cases: Vec<SwitchCases>,
    /// See [`Cfg::prelude`].
    prelude: Vec<DaStmt>,
    currently_live: Vec<IndexSet<CDeclId>>,
    next: u64,
}

/// A WIP block under construction.
struct WipBlock {
    label: Label,
    body: Vec<StmtOrDecl>,
    defined: IndexSet<CDeclId>,
    live: IndexSet<CDeclId>,
}

impl CfgBuilder {
    fn new(entry: Label) -> Self {
        CfgBuilder {
            nodes: IndexMap::new(),
            decls_seen: DeclStmtStore::new(),
            loops: LoopInfo::new(),
            multiples: MultipleInfo::new(),
            break_labels: vec![],
            continue_labels: vec![],
            switch_cases: vec![],
            prelude: vec![],
            currently_live: vec![IndexSet::new()],
            next: 1,
        }
    }

    fn fresh_label(&mut self) -> Label {
        let lbl = Label::Synthetic(self.next);
        self.next += 1;
        lbl
    }

    fn new_wip(&mut self, lbl: Label) -> WipBlock {
        WipBlock {
            label: lbl,
            body: vec![],
            defined: IndexSet::new(),
            live: self.currently_live.last().cloned().unwrap_or_default(),
        }
    }

    fn with_scope<T, F>(&mut self, f: F) -> TranslationResult<T>
    where
        F: FnOnce(&mut Self) -> TranslationResult<T>,
    {
        self.currently_live.push(IndexSet::new());
        let result = f(self);
        self.currently_live.pop();
        result
    }

    fn add_decl_to_scope(&mut self, decl: CDeclId) {
        for live in &mut self.currently_live {
            live.insert(decl);
        }
    }

    fn add_block(&mut self, wip: WipBlock, term: GenTerminator<Label>) {
        self.nodes.insert(
            wip.label,
            BasicBlock {
                body: wip.body,
                terminator: term,
                live: wip.live,
                defined: wip.defined,
            },
        );
    }

    fn add_condition_branch(
        &mut self,
        entry: Label,
        cond_ws: WithStmts<DaExpr>,
        true_entry: Label,
        false_entry: Label,
    ) {
        if cond_ws.stmts.is_empty() {
            let wip = self.new_wip(entry);
            self.add_block(wip, Branch(cond_ws.val, true_entry, false_entry));
            return;
        }

        let cond_entry = self.fresh_label();
        let mut prelude = self.new_wip(entry);
        prelude
            .body
            .extend(cond_ws.stmts.into_iter().map(StmtOrDecl::Stmt));
        self.add_block(prelude, Jump(cond_entry.clone()));

        let wip = self.new_wip(cond_entry);
        self.add_block(wip, Branch(cond_ws.val, true_entry, false_entry));
    }

    /// Lower a C expression that appears in *statement* position.
    ///
    /// C's comma operator is a sequence point, not a value, when it is used as a
    /// statement (`i = 0, j = 10;`) or as a `for` clause (`i++, j--`).  Value
    /// lowering keeps only the right operand, so a statement-position comma is
    /// flattened here into one daScript statement per operand.
    fn convert_expr_in_stmt_position(
        &mut self,
        tr: &Translation,
        ctx: ExprContext,
        eid: CExprId,
        out: &mut Vec<StmtOrDecl>,
    ) -> TranslationResult<()> {
        if let CExprKind::Binary(_, CBinOp::Comma, lhs, rhs, _, _) = tr.ast_context[eid].kind {
            self.convert_expr_in_stmt_position(tr, ctx, lhs, out)?;
            return self.convert_expr_in_stmt_position(tr, ctx, rhs, out);
        }
        let ws = tr.convert_expr(ctx.unused(), eid, None)?;
        out.extend(ws.stmts.into_iter().map(StmtOrDecl::Stmt));
        out.push(StmtOrDecl::Stmt(DaStmt::Expr(ws.val)));
        Ok(())
    }

    /// Process a sequence of C statements and build the CFG.
    /// Returns the label after the last statement (for fallthrough).
    fn convert_stmts(
        &mut self,
        tr: &Translation,
        ctx: ExprContext,
        stmts: &[CStmtId],
        in_tail: Option<&ImplicitReturnType>,
        entry: Label,
        ret_ty: Option<CQualTypeId>,
    ) -> TranslationResult<Option<Label>> {
        self.with_scope(|slf| {
            let mut lbl = Some(entry);
            let last = stmts.last().copied();
            for &sid in stmts {
                let new_entry = lbl.unwrap_or_else(|| slf.fresh_label());
                let tail = in_tail.filter(|_| Some(sid) == last);
                lbl = slf.convert_stmt(tr, ctx, sid, tail, new_entry, ret_ty)?;
            }
            Ok(lbl)
        })
    }

    /// Process a single C statement. Returns Some(label) for fallthrough, None if terminated.
    fn convert_stmt(
        &mut self,
        tr: &Translation,
        ctx: ExprContext,
        sid: CStmtId,
        in_tail: Option<&ImplicitReturnType>,
        entry: Label,
        ret_ty: Option<CQualTypeId>,
    ) -> TranslationResult<Option<Label>> {
        let sk = &tr.ast_context[sid].kind;
        match sk {
            CStmtKind::Empty => Ok(Some(entry)),

            CStmtKind::Expr(eid) => {
                let mut wip = self.new_wip(entry);
                let stmt_ctx = ExprContext {
                    used: false,
                    is_const: false,
                    ..Default::default()
                };
                let eid = *eid;
                let mut body = std::mem::take(&mut wip.body);
                self.convert_expr_in_stmt_position(tr, stmt_ctx, eid, &mut body)?;
                wip.body = body;
                let next = self.fresh_label();
                self.add_block(wip, Jump(next.clone()));
                Ok(Some(next))
            }

            CStmtKind::Return(ref expr_opt) => {
                let mut wip = self.new_wip(entry);
                let val: Option<Box<DaExpr>> = match expr_opt {
                    Some(e) => {
                        let ws = tr.convert_expr(ExprContext::default().used(), *e, None)?;
                        let val = if let Some(ret_ty) = ret_ty {
                            let ret_da = tr.convert_type(ret_ty)?;
                            let ws = tr.lower_to_c_value(
                                ws,
                                tr.ast_context[*e].kind.get_qual_type(),
                                ret_da.clone(),
                                ValueSite::Return,
                            )?;
                            wip.body.extend(ws.stmts.into_iter().map(StmtOrDecl::Stmt));
                            let expr_is_ptr = tr.ast_context[*e]
                                .kind
                                .get_qual_type()
                                .map_or(false, |qty| tr.is_pointer_type(qty.ctype));
                            if matches!(ret_da.kind, DaTypeKind::UInt64) && expr_is_ptr {
                                DaExpr::Unsafe(Box::new(DaExpr::Cast {
                                    kind: das_ast::CastKind::Reinterpret,
                                    expr: Box::new(ws.val),
                                    to: DaType::uint64(),
                                }))
                            } else if matches!(ret_da.kind, DaTypeKind::Pointer(_)) {
                                DaExpr::Unsafe(Box::new(DaExpr::Cast {
                                    kind: das_ast::CastKind::Reinterpret,
                                    expr: Box::new(ws.val),
                                    to: ret_da,
                                }))
                            } else {
                                ws.val
                            }
                        } else {
                            wip.body.extend(ws.stmts.into_iter().map(StmtOrDecl::Stmt));
                            ws.val
                        };
                        Some(Box::new(val))
                    }
                    None => None,
                };
                wip.body
                    .push(StmtOrDecl::Stmt(DaStmt::Expr(DaExpr::Return(val))));
                self.add_block(wip, End);
                Ok(None)
            }

            CStmtKind::Compound(ref kids) => {
                self.convert_stmts(tr, ctx, kids, in_tail, entry, ret_ty)
            }

            CStmtKind::Decls(ref decls) => {
                let mut current_entry = entry;
                for &d in decls {
                    let info = tr.convert_decl_stmt_info(ctx, d)?;
                    self.decls_seen.store.insert(d, info);
                    let mut wip = self.new_wip(current_entry);
                    wip.body.push(StmtOrDecl::Decl(d));
                    wip.defined.insert(d);
                    self.add_decl_to_scope(d);
                    wip.live = self.currently_live.last().cloned().unwrap_or_default();
                    let next = self.fresh_label();
                    self.add_block(wip, Jump(next.clone()));
                    current_entry = next;
                }
                Ok(Some(current_entry))
            }

            CStmtKind::If {
                scrutinee,
                true_variant,
                false_variant,
            } => {
                let cond_ws = tr.convert_condition(ctx.used(), true, *scrutinee)?;

                let then_entry = self.fresh_label();
                let else_entry = self.fresh_label();
                let next_entry = self.fresh_label();

                // Condition block: Branch(cond, then_entry, else_entry)
                self.add_condition_branch(entry, cond_ws, then_entry.clone(), else_entry.clone());

                // Then block
                let then_ends =
                    self.convert_stmt(tr, ctx, *true_variant, in_tail, then_entry, ret_ty)?;
                if let Some(end) = then_ends {
                    let wip = self.new_wip(end);
                    self.add_block(wip, Jump(next_entry.clone()));
                }

                // Else block
                let else_ends = match false_variant {
                    Some(fv) => self.convert_stmt(tr, ctx, *fv, in_tail, else_entry, ret_ty)?,
                    None => Some(else_entry),
                };
                if let Some(end) = else_ends {
                    let wip = self.new_wip(end);
                    self.add_block(wip, Jump(next_entry.clone()));
                }

                Ok(Some(next_entry))
            }

            CStmtKind::While { condition, body } => {
                let cond_entry = self.fresh_label();
                let body_entry = self.fresh_label();
                let post = self.fresh_label();
                self.break_labels.push(post.clone());
                self.continue_labels.push(cond_entry.clone());

                // Entry → cond_entry
                let wip0 = self.new_wip(entry);
                self.add_block(wip0, Jump(cond_entry.clone()));

                // cond_entry: convert condition, Branch to body or post
                let cond_ws = tr.convert_condition(ctx.used(), true, *condition)?;
                self.add_condition_branch(
                    cond_entry.clone(),
                    cond_ws,
                    body_entry.clone(),
                    post.clone(),
                );

                // body
                let body_end = self.convert_stmt(tr, ctx, *body, None, body_entry, ret_ty)?;
                if let Some(end) = body_end {
                    let wip = self.new_wip(end);
                    self.add_block(wip, Jump(cond_entry.clone()));
                }

                self.break_labels.pop();
                self.continue_labels.pop();
                Ok(Some(post))
            }

            CStmtKind::DoWhile { body, condition } => {
                let body_entry = self.fresh_label();
                let cond_entry = self.fresh_label();
                let post = self.fresh_label();
                self.break_labels.push(post.clone());
                self.continue_labels.push(cond_entry.clone());

                // Entry → body_entry
                let wip0 = self.new_wip(entry);
                self.add_block(wip0, Jump(body_entry.clone()));

                // body
                let body_end =
                    self.convert_stmt(tr, ctx, *body, None, body_entry.clone(), ret_ty)?;
                if let Some(end) = body_end {
                    let wip = self.new_wip(end);
                    self.add_block(wip, Jump(cond_entry.clone()));
                }

                // cond_entry: convert condition, Branch to body or post
                let cond_ws = tr.convert_condition(ctx.used(), true, *condition)?;
                self.add_condition_branch(cond_entry, cond_ws, body_entry.clone(), post.clone());

                self.break_labels.pop();
                self.continue_labels.pop();
                Ok(Some(post))
            }

            CStmtKind::ForLoop {
                init,
                condition,
                increment,
                body,
            } => {
                // Layout:            entry -> init -> cond -> body -> incr -> cond
                //                                       \-> after       break -> after
                // `continue` targets `incr`, not `cond`: C 6.8.6.2 runs the third
                // clause of a `for` before re-testing the condition.
                let cond_entry = self.fresh_label();
                let body_entry = self.fresh_label();
                let incr_entry = self.fresh_label();
                let after = self.fresh_label();

                // The init clause runs once, starting at the statement's own entry
                // block, and falls through into the condition.  Converting it into a
                // freshly minted label instead would leave `cond_entry` without a
                // predecessor and the whole loop would be pruned as unreachable.
                let init_end = match init {
                    Some(iid) => self.convert_stmt(tr, ctx, *iid, None, entry.clone(), ret_ty)?,
                    None => Some(entry.clone()),
                };
                if let Some(end) = init_end {
                    let wip = self.new_wip(end);
                    self.add_block(wip, Jump(cond_entry.clone()));
                }

                self.break_labels.push(after.clone());
                self.continue_labels.push(incr_entry.clone());

                // cond_entry: Branch(condition, body_entry, after) or Jump(body_entry)
                match condition {
                    Some(cid) => {
                        let cond_ws = tr.convert_condition(ctx.used(), true, *cid)?;
                        self.add_condition_branch(
                            cond_entry.clone(),
                            cond_ws,
                            body_entry.clone(),
                            after.clone(),
                        );
                    }
                    None => {
                        let wip = self.new_wip(cond_entry.clone());
                        self.add_block(wip, Jump(body_entry.clone()));
                    }
                }

                // body falls through into the increment block
                let body_end = self.convert_stmt(tr, ctx, *body, None, body_entry, ret_ty)?;
                if let Some(end) = body_end {
                    let wip = self.new_wip(end);
                    self.add_block(wip, Jump(incr_entry.clone()));
                }

                // increment block — also the `continue` target
                let mut incr_wip = self.new_wip(incr_entry.clone());
                if let Some(inc_id) = *increment {
                    let mut body = std::mem::take(&mut incr_wip.body);
                    self.convert_expr_in_stmt_position(tr, ctx, inc_id, &mut body)?;
                    incr_wip.body = body;
                }
                self.add_block(incr_wip, Jump(cond_entry.clone()));

                self.break_labels.pop();
                self.continue_labels.pop();
                Ok(Some(after))
            }

            CStmtKind::Switch { scrutinee, body } => {
                let scrut_ws = tr.convert_expr(ctx.used(), *scrutinee, None)?;
                let switch_end = self.fresh_label();

                // C integer promotion: the controlling expression of a `switch`
                // is promoted, and every `case` constant is converted to the
                // promoted type.  daScript has no implicit numeric conversions,
                // so the promotion has to be explicit on both sides.
                let scrut_ty = tr
                    .ast_context[*scrutinee]
                    .kind
                    .get_qual_type()
                    .map(|q| tr.convert_type(q))
                    .transpose()?
                    .unwrap_or_else(DaType::int);
                let promote = matches!(
                    scrut_ty.kind,
                    DaTypeKind::Bool
                        | DaTypeKind::Int8
                        | DaTypeKind::Int16
                        | DaTypeKind::UInt8
                        | DaTypeKind::UInt16
                        | DaTypeKind::Named(_)
                        | DaTypeKind::Auto
                );
                let case_ty = if promote { DaType::int() } else { scrut_ty };
                let scrut_val = if promote {
                    DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(scrut_ws.val),
                        to: DaType::int(),
                    }
                } else {
                    scrut_ws.val
                };

                // The scrutinee's own side effects run before the dispatch.
                let mut wip = self.new_wip(entry);
                wip.body
                    .extend(scrut_ws.stmts.into_iter().map(StmtOrDecl::Stmt));

                // C evaluates the controlling expression exactly once, but the
                // dispatch compares it against every case value in turn, so
                // anything that is not already a plain read has to be bound to a
                // temporary first.  The binding goes in the prelude, not in this
                // block: a `goto` may enter a case ahead of the dispatch, and the
                // temporary must be in scope there too.
                let scrut_val = match scrut_val {
                    plain @ (DaExpr::Var(_)
                    | DaExpr::ConstInt(_)
                    | DaExpr::ConstUInt(_)
                    | DaExpr::ConstBool(_)) => plain,
                    computed => {
                        let name = tr.renamer.borrow_mut().fresh();
                        self.prelude.push(DaStmt::Var {
                            name: name.clone(),
                            var_type: case_ty.clone(),
                            // daScript default-initializes a `var` with no
                            // initializer; the assignment below always runs
                            // before any comparison reads it.
                            init: None,
                        });
                        wip.body.push(StmtOrDecl::Stmt(DaStmt::Expr(DaExpr::Assign(
                            Box::new(DaExpr::Var(name.clone())),
                            Box::new(computed),
                        ))));
                        DaExpr::Var(name)
                    }
                };

                // The body is walked first: `case`/`default` labels register the
                // blocks they dispatch to, and C permits them arbitrarily deep in
                // the body (Duff's device), so the dispatch table is only complete
                // once the whole body has been converted.  The body is entered by
                // dispatch alone; a fresh unreachable entry keeps statements that
                // precede the first `case` from running.
                self.break_labels.push(switch_end.clone());
                self.switch_cases.push(SwitchCases::default());
                let body_entry = self.fresh_label();
                let body_end = self.convert_stmt(tr, ctx, *body, None, body_entry, ret_ty)?;
                if let Some(end) = body_end {
                    let tail = self.new_wip(end);
                    self.add_block(tail, Jump(switch_end.clone()));
                }
                let collected = self
                    .switch_cases
                    .pop()
                    .expect("switch case frame pushed above");
                self.break_labels.pop();

                // Terminator convention: the last pair is the default arm and its
                // value expression is never compared. Without a `default` label the
                // switch falls out to its own end.
                let mut cases: Vec<(DaExpr, Label)> = collected
                    .cases
                    .into_iter()
                    .map(|(val, lbl)| {
                        (
                            DaExpr::Cast {
                                kind: das_ast::CastKind::Cast,
                                expr: Box::new(val),
                                to: case_ty.clone(),
                            },
                            lbl,
                        )
                    })
                    .collect();
                cases.push((
                    DaExpr::ConstBool(true),
                    collected.default.unwrap_or_else(|| switch_end.clone()),
                ));
                self.add_block(
                    wip,
                    Switch {
                        expr: scrut_val,
                        cases,
                    },
                );

                Ok(Some(switch_end))
            }

            CStmtKind::Case(_expr, sub_stmt, cst) => {
                let case_entry = self.fresh_label();
                // Whatever preceded this label falls through into it.
                let wip = self.new_wip(entry);
                self.add_block(wip, Jump(case_entry.clone()));
                let val = match cst {
                    ConstIntExpr::I(v) => DaExpr::ConstInt(*v),
                    ConstIntExpr::U(v) => DaExpr::ConstUInt(*v),
                };
                self.switch_cases
                    .last_mut()
                    .ok_or_else(|| TranslationError::generic("case label outside switch"))?
                    .cases
                    .push((val, case_entry.clone()));
                self.convert_stmt(tr, ctx, *sub_stmt, in_tail, case_entry, ret_ty)
            }

            CStmtKind::Default(sub_stmt) => {
                let default_entry = self.fresh_label();
                let wip = self.new_wip(entry);
                self.add_block(wip, Jump(default_entry.clone()));
                let frame = self
                    .switch_cases
                    .last_mut()
                    .ok_or_else(|| TranslationError::generic("default label outside switch"))?;
                if frame.default.is_some() {
                    return Err(TranslationError::generic(
                        "switch has more than one default label",
                    ));
                }
                frame.default = Some(default_entry.clone());
                self.convert_stmt(tr, ctx, *sub_stmt, in_tail, default_entry, ret_ty)
            }

            CStmtKind::Goto(target) => {
                // A `goto` is an edge, not a statement: the CFG carries the jump and
                // the label back end renders it.  Pushing a literal `goto` into the
                // body and terminating the block with `End` used to leave the target
                // without a predecessor, so it was pruned as unreachable.
                let target_lbl =
                    Label::FromC(*target, tr.ast_context.label_names.get(target).cloned());
                let wip = self.new_wip(entry);
                self.add_block(wip, Jump(target_lbl));
                Ok(None)
            }

            CStmtKind::Label(sub_stmt) => {
                let clbl: CLabelId = sid.into();
                let lbl = Label::FromC(clbl, tr.ast_context.label_names.get(&clbl).cloned());
                // Fall-through into the labelled statement must reach the label's own
                // block, otherwise every `goto` target becomes unreachable.
                let wip = self.new_wip(entry);
                self.add_block(wip, Jump(lbl.clone()));
                self.convert_stmt(tr, ctx, *sub_stmt, in_tail, lbl, ret_ty)
            }

            CStmtKind::Break => {
                let brk = self
                    .break_labels
                    .last()
                    .cloned()
                    .ok_or_else(|| TranslationError::generic("break outside loop or switch"))?;
                // The edge *is* the break; emitting a literal `break` as well
                // produced a second, unstructured exit.
                let wip = self.new_wip(entry);
                self.add_block(wip, Jump(brk));
                Ok(None)
            }

            CStmtKind::Continue => {
                let cont = self
                    .continue_labels
                    .last()
                    .cloned()
                    .ok_or_else(|| TranslationError::generic("continue outside loop"))?;
                let wip = self.new_wip(entry);
                self.add_block(wip, Jump(cont));
                Ok(None)
            }

            // Inline asm has no CFG-neutral scalar substitute. Route it to
            // translator/assembly.rs so the user receives its source-located
            // ABI diagnostic instead of a generic CFG failure.
            CStmtKind::Asm { asm, inputs, outputs, clobbers, is_volatile } => {
                tr.convert_inline_assembly(
                    sid,
                    asm,
                    inputs,
                    outputs,
                    clobbers,
                    *is_volatile,
                )?;
                unreachable!("inline assembly lowering always diagnoses or returns a real statement")
            }

            _ => Err(TranslationError::generic(
                "unsupported statement in CfgBuilder",
            )),
        }
    }
}

// ===== Cfg::from_stmts =====

impl Cfg<Label, StmtOrDecl> {
    /// Build a CFG from a list of C statements.
    /// Uses CfgBuilder internally.
    pub fn from_stmts(
        translator: &Translation,
        ctx: ExprContext,
        stmt_ids: &[CStmtId],
        ret: ImplicitReturnType,
        ret_ty: Option<CQualTypeId>,
    ) -> TranslationResult<(Self, DeclStmtStore)> {
        let entry = Label::Synthetic(0);
        let mut builder = CfgBuilder::new(entry.clone());

        let last_lbl =
            builder.convert_stmts(translator, ctx, stmt_ids, Some(&ret), entry.clone(), ret_ty)?;

        // Add implicit return at the end
        let exit_lbl = last_lbl.unwrap_or_else(|| builder.fresh_label());
        let tail_stmt = match &ret {
            ImplicitReturnType::Main => DaStmt::Expr(DaExpr::Return(Some(Box::new(
                DaExpr::ConstInt(0),
            )))),
            ImplicitReturnType::Void | ImplicitReturnType::StmtExprVoid => {
                DaStmt::Expr(DaExpr::Return(None))
            }
            // Falling off the end of a value-returning C function is undefined
            // behaviour.  `return` with no value would not even type-check in
            // daScript, so make the undefined path a diagnosable trap instead of a
            // silently wrong value.
            _ => DaStmt::Expr(unreachable_trap(
                "control reached the end of a non-void function",
            )),
        };
        let mut wip = builder.new_wip(exit_lbl);
        wip.body.push(StmtOrDecl::Stmt(tail_stmt));
        builder.add_block(wip, End);

        let cfg = Cfg {
            entries: entry,
            nodes: builder.nodes,
            loops: builder.loops,
            multiples: builder.multiples,
            prelude: builder.prelude,
        };

        Ok((cfg, builder.decls_seen))
    }
}

/// How much a single [`Cfg::prune_unreachable`] pass removed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PruneReport {
    /// Blocks that were not reachable from the entry at all.
    pub removed_blocks: usize,
    /// Of those, the ones that still carried statements (C dead code).
    pub removed_with_body: usize,
}

/// Prune unreachable and empty blocks from the CFG.
impl<Lbl: Clone + Ord + Hash + Debug, Stmt> Cfg<Lbl, Stmt> {
    /// Drop the blocks that the entry cannot reach and report what went.
    ///
    /// Only statements that C itself makes unreachable (code after `return`,
    /// `goto`, `break`, …) may disappear here.  Anything else is a construction
    /// bug in [`CfgBuilder`] — the audit case was a `for` loop whose init block
    /// never linked to the condition, which silently deleted the loop and the
    /// whole remainder of the function.  [`Cfg::validate_edges`] turns the
    /// observable half of that class of bug into a hard error.
    fn prune_unreachable(&mut self) -> PruneReport {
        let visited: IndexSet<Lbl> = {
            let mut v = IndexSet::new();
            let mut q = vec![self.entries.clone()];
            while let Some(l) = q.pop() {
                if !v.insert(l.clone()) {
                    continue;
                }
                if let Some(bb) = self.nodes.get(&l) {
                    for n in bb.terminator.get_labels() {
                        if !v.contains(n) {
                            q.push(n.clone());
                        }
                    }
                }
            }
            v
        };
        let mut report = PruneReport::default();
        for (lbl, bb) in self.nodes.iter() {
            if visited.contains(lbl) {
                continue;
            }
            report.removed_blocks += 1;
            if !bb.body.is_empty() {
                report.removed_with_body += 1;
            }
        }
        self.nodes.retain(|l, _| visited.contains(l));
        self.loops.filter_unreachable(&visited);
        report
    }

    /// Every edge of a surviving block must land on a surviving block.
    ///
    /// A dangling successor used to be swallowed by the relooper and rendered as
    /// a bare `break`, which is how a whole function body could turn into three
    /// statements without any diagnostic.
    fn validate_edges(&self) -> TranslationResult<()> {
        for (lbl, bb) in self.nodes.iter() {
            for target in bb.terminator.get_labels() {
                if !self.nodes.contains_key(target) {
                    return Err(crate::format_translation_err!(
                        None,
                        "internal error: control-flow graph edge {lbl:?} -> {target:?} \
                         has no target block",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// `panic(msg)` — the daScript expression used wherever C reaches a path that
/// has no defined value to produce.
pub(crate) fn unreachable_trap(msg: &str) -> DaExpr {
    DaExpr::Call(
        Box::new(DaExpr::Var("panic".into())),
        vec![DaExpr::ConstString(msg.to_string())],
    )
}

// ===== Public entry point =====

/// Convert a function body: C statements → CFG → daScript statements.
///
/// # Back-end strategy
///
/// The CFG is rendered *directly* as daScript numeric labels and `goto`
/// (daslang reference, `language/statements.rst`, "label and goto"):
/// blocks are laid out linearly, every block that some non-fall-through edge
/// targets gets a `label N:`, and each terminator becomes an unconditional,
/// conditional or dispatching `goto`.  See [`labels`] for the rendering itself.
///
/// This is the audit's option (a).  It was chosen over the c2rust
/// relooper + `current_block` dispatch because daScript has no labelled `break`
/// or `continue`: with only unlabelled ones, every multi-level exit and every
/// irreducible region (a C state machine built from `goto`, a `switch` whose
/// cases sit inside a loop) has to be flattened anyway, and a partial
/// flattening is exactly what silently produced wrong programs before.  A
/// direct label/goto rendering is total — it handles reducible and irreducible
/// graphs identically — and it is checkable: [`Cfg::validate_edges`] rejects any
/// graph whose edges do not all land on real blocks.
///
/// [`relooper`] and [`structures`] are retained (with their unit tests) for a
/// future readability pass that re-structures the reducible parts; they are not
/// on this path.
pub fn convert_function_body(
    translator: &Translation,
    _body_id: CStmtId,
    stmts: &[CStmtId],
    ret: ImplicitReturnType,
    ret_ty: Option<CQualTypeId>,
) -> TranslationResult<Vec<DaStmt>> {
    let (mut graph, store) =
        Cfg::from_stmts(translator, ExprContext::default(), stmts, ret, ret_ty)?;
    let report = graph.prune_unreachable();
    if report.removed_with_body > 0 {
        log::debug!(
            "cfg: dropped {} unreachable block(s), {} of them carrying C dead code",
            report.removed_blocks,
            report.removed_with_body,
        );
    }
    graph.validate_edges()?;

    labels::render(graph, store)
}
