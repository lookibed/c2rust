//! Flat `label`/`goto` back end: a CFG rendered as daScript statements.
//!
//! daslang has first-class numeric labels and jumps — `label 3:` marks a point in
//! a function body and `goto label 3` transfers control to it (daslang reference,
//! `language/statements.rst`, section "label and goto").  Jumps out of, into and
//! across `while` bodies and past local `var` declarations are all legal, which
//! makes this the one lowering that is *total*: every C control-flow graph,
//! reducible or not, has an exact rendering.
//!
//! Rendering has three steps:
//!
//! 1. **Layout** — a depth-first walk from the entry block that prefers the
//!    successor which can fall through (the `false` arm of a branch, a `switch`'s
//!    default arm), so most edges cost no `goto` at all.
//! 2. **Terminator planning** — each block's terminator is turned into a [`Tail`],
//!    which records exactly which edges still need an explicit jump.
//! 3. **Emission** — labels are numbered in layout order and only assigned to
//!    blocks that a planned jump actually targets, then the statements are
//!    concatenated.
//!
//! Everything a block declares lands in the function's own scope, so no
//! declaration hoisting is required: daScript scopes a `var` to its enclosing
//! block, and here that block is the function body.

use super::*;
use das_ast::{DaBlock, DaExpr, DaStmt};

/// What has to be emitted after a block's own statements.
enum Tail {
    /// The terminator's successor is the next block in layout order.
    FallThrough,
    /// The block ends the function (its body already returned or trapped).
    End,
    Goto(Label),
    /// `if cond { goto target }`, falling through otherwise.
    IfGoto(DaExpr, Label),
    /// `if cond { goto then } else { goto else }`.
    IfElseGoto(DaExpr, Label, Label),
    /// A `switch` dispatch: compare the scrutinee against each case value in
    /// turn, and take the default arm otherwise.  `default` is `None` when the
    /// default arm is the next block in layout order.
    Dispatch {
        scrutinee: DaExpr,
        cases: Vec<(DaExpr, Label)>,
        default: Option<Label>,
    },
}

impl Tail {
    /// Every label this tail jumps to, and therefore needs a `label N:` on.
    fn targets(&self) -> Vec<&Label> {
        match self {
            Tail::FallThrough | Tail::End => vec![],
            Tail::Goto(l) | Tail::IfGoto(_, l) => vec![l],
            Tail::IfElseGoto(_, t, f) => vec![t, f],
            Tail::Dispatch { cases, default, .. } => cases
                .iter()
                .map(|(_, l)| l)
                .chain(default.iter())
                .collect(),
        }
    }
}

/// Render a pruned, edge-validated CFG as a flat daScript statement list.
pub(crate) fn render(
    cfg: Cfg<Label, StmtOrDecl>,
    mut store: DeclStmtStore,
) -> TranslationResult<Vec<DaStmt>> {
    let order = layout(&cfg);

    // Every local declaration is split: the `var` is hoisted to the top of the
    // function with its default value, and only the C initializer stays where
    // the declaration was.  C gives a block-scope object storage for the whole
    // block regardless of where control enters, and a `goto` may well jump over
    // a declaration and then read the object; hoisting is what makes that
    // behave.  Declarations are hoisted in the order the CFG built them, which
    // is source order.
    let declared: IndexSet<CDeclId> = cfg
        .nodes
        .values()
        .flat_map(|block| block.body.iter())
        .filter_map(|item| match item {
            StmtOrDecl::Decl(decl_id) => Some(*decl_id),
            StmtOrDecl::Stmt(_) => None,
        })
        .collect();
    let mut hoisted: Vec<DaStmt> = cfg.prelude.clone();
    for decl_id in &declared {
        hoisted.extend(store.extract_decl(*decl_id)?.into_iter().map(writable_decl));
    }

    // Step 2: plan every terminator against its fall-through successor.
    // (Hoisted declarations were made writable by `writable_decl` above.)
    let mut tails: Vec<Tail> = Vec::with_capacity(order.len());
    for (index, label) in order.iter().enumerate() {
        let next = order.get(index + 1);
        let block = cfg
            .nodes
            .get(label)
            .expect("layout only visits blocks that exist");
        tails.push(plan(&block.terminator, next));
    }

    // Step 3: number only the labels a planned jump targets.
    let jumped_to: IndexSet<&Label> = tails.iter().flat_map(Tail::targets).collect();
    let mut label_ids: IndexMap<Label, u64> = IndexMap::new();
    for label in &order {
        // Numbering follows layout order so the emitted labels read top to
        // bottom, which is what a reader of the generated file expects.
        if jumped_to.contains(label) {
            let id = label_ids.len() as u64;
            label_ids.insert(label.clone(), id);
        }
    }
    drop(jumped_to);

    let goto = |label: &Label| -> DaStmt {
        DaStmt::Expr(DaExpr::Goto(label_text(&label_ids, label)))
    };
    let goto_block = |label: &Label| -> DaExpr {
        DaExpr::Block(DaBlock {
            stmts: vec![goto(label)],
        })
    };

    let mut out: Vec<DaStmt> = hoisted;
    for (index, label) in order.iter().enumerate() {
        if label_ids.contains_key(label) {
            out.push(DaStmt::Expr(DaExpr::Label(label_text(&label_ids, label))));
        }
        let block = cfg
            .nodes
            .get(label)
            .expect("layout only visits blocks that exist");
        for item in block.body.clone() {
            out.extend(item.place_decls(&declared, &mut store));
        }
        match &tails[index] {
            Tail::FallThrough | Tail::End => {}
            Tail::Goto(target) => out.push(goto(target)),
            Tail::IfGoto(cond, target) => out.push(DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(cond.clone()),
                then: Box::new(goto_block(target)),
                elifs: vec![],
                else_: None,
            })),
            Tail::IfElseGoto(cond, then_target, else_target) => {
                out.push(DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(cond.clone()),
                    then: Box::new(goto_block(then_target)),
                    elifs: vec![],
                    else_: Some(Box::new(goto_block(else_target))),
                }))
            }
            Tail::Dispatch {
                scrutinee,
                cases,
                default,
            } => {
                let mut arms = cases.iter();
                let Some((first_value, first_target)) = arms.next() else {
                    // A `switch` with no `case` labels at all: only the default
                    // arm can ever run.
                    if let Some(default) = default {
                        out.push(goto(default));
                    }
                    continue;
                };
                let elifs = arms
                    .map(|(value, target)| (case_test(scrutinee, value), goto_block(target)))
                    .collect();
                out.push(DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(case_test(scrutinee, first_value)),
                    then: Box::new(goto_block(first_target)),
                    elifs,
                    else_: default.as_ref().map(|d| Box::new(goto_block(d))),
                }));
            }
        }
    }

    // The last laid-out block never falls through — its tail is either the end
    // of the function or an unconditional jump — but daScript checks statically
    // that a value-returning function ends on a `return` and cannot see that.
    // Close such a body with an explicitly unreachable trap.
    if !matches!(tails.last(), Some(Tail::End) | None) {
        out.push(DaStmt::Expr(unreachable_trap(
            "unreachable: fell out of a translated control-flow graph",
        )));
    }

    Ok(out)
}

/// Depth-first layout that keeps as many edges as possible implicit.
fn layout(cfg: &Cfg<Label, StmtOrDecl>) -> Vec<Label> {
    let mut order: Vec<Label> = Vec::with_capacity(cfg.nodes.len());
    let mut seen: IndexSet<Label> = IndexSet::new();
    let mut stack: Vec<Label> = vec![cfg.entries.clone()];
    while let Some(label) = stack.pop() {
        if seen.contains(&label) || !cfg.nodes.contains_key(&label) {
            continue;
        }
        seen.insert(label.clone());
        order.push(label.clone());
        // Pushed in reverse preference, so the preferred successor is popped —
        // and therefore laid out — first, and its edge needs no `goto`.
        for successor in preferred_successors(&cfg.nodes[&label].terminator)
            .into_iter()
            .rev()
        {
            if !seen.contains(&successor) {
                stack.push(successor);
            }
        }
    }
    order
}

/// Successors in the order we would most like to place them, best first.
fn preferred_successors(terminator: &GenTerminator<Label>) -> Vec<Label> {
    match terminator {
        End => vec![],
        Jump(target) => vec![target.clone()],
        // The `false` arm falls through, matching `if cond { goto then }`.
        Branch(_, then_target, else_target) => vec![else_target.clone(), then_target.clone()],
        Switch { cases, .. } => {
            // The last pair is the default arm (see `CfgBuilder`'s `Switch`
            // construction) and it is the only one that can fall through.
            let mut preferred: Vec<Label> = Vec::with_capacity(cases.len());
            if let Some((_, default)) = cases.last() {
                preferred.push(default.clone());
            }
            for (_, target) in cases.iter().take(cases.len().saturating_sub(1)) {
                preferred.push(target.clone());
            }
            preferred
        }
    }
}

/// Turn one terminator into the statements it still has to emit.
fn plan(terminator: &GenTerminator<Label>, next: Option<&Label>) -> Tail {
    match terminator {
        End => Tail::End,
        Jump(target) => {
            if Some(target) == next {
                Tail::FallThrough
            } else {
                Tail::Goto(target.clone())
            }
        }
        Branch(cond, then_target, else_target) => {
            let then_falls = Some(then_target) == next;
            let else_falls = Some(else_target) == next;
            match (then_falls, else_falls) {
                // Both arms continue at the same place: the condition was built
                // by `convert_condition`, whose side effects are already in this
                // block's statements, so nothing is lost by dropping the test.
                (true, true) => Tail::FallThrough,
                (_, true) => Tail::IfGoto(cond.clone(), then_target.clone()),
                (true, _) => Tail::IfGoto(negate(cond), else_target.clone()),
                (false, false) => Tail::IfElseGoto(
                    cond.clone(),
                    then_target.clone(),
                    else_target.clone(),
                ),
            }
        }
        Switch { expr, cases } => {
            let Some((_, default)) = cases.last() else {
                return Tail::End;
            };
            let default = if Some(default) == next {
                None
            } else {
                Some(default.clone())
            };
            Tail::Dispatch {
                scrutinee: expr.clone(),
                cases: cases
                    .iter()
                    .take(cases.len() - 1)
                    .map(|(value, target)| (value.clone(), target.clone()))
                    .collect(),
                default,
            }
        }
    }
}

fn case_test(scrutinee: &DaExpr, value: &DaExpr) -> DaExpr {
    DaExpr::Op2 {
        op: "==",
        left: Box::new(scrutinee.clone()),
        right: Box::new(value.clone()),
    }
}

fn negate(cond: &DaExpr) -> DaExpr {
    // `!(a == b)` is `a != b`; keeping the comparison flat reads better and
    // avoids a redundant parenthesised negation in the printed source.
    if let DaExpr::Op2 { op, left, right } = cond {
        if let Some(inverse) = inverse_comparison(op) {
            return DaExpr::Op2 {
                op: inverse,
                left: left.clone(),
                right: right.clone(),
            };
        }
    }
    if let DaExpr::Op1 { op: "!", expr } = cond {
        return (**expr).clone();
    }
    DaExpr::Op1 {
        op: "!",
        expr: Box::new(cond.clone()),
    }
}

fn inverse_comparison(op: &str) -> Option<&'static str> {
    match op {
        "==" => Some("!="),
        "!=" => Some("=="),
        "<" => Some(">="),
        "<=" => Some(">"),
        ">" => Some("<="),
        ">=" => Some("<"),
        _ => None,
    }
}

/// A hoisted declaration is assigned later, at the point where the C
/// declaration stood, so it cannot keep a `const` qualifier: `const int x = 42;`
/// becomes `var x : int` at the top of the function and `x = 42` in place.
fn writable_decl(stmt: DaStmt) -> DaStmt {
    match stmt {
        DaStmt::Var {
            name,
            mut var_type,
            init,
        } => {
            var_type.is_const = false;
            DaStmt::Var {
                name,
                var_type,
                init,
            }
        }
        other => other,
    }
}

fn label_text(label_ids: &IndexMap<Label, u64>, label: &Label) -> String {
    let id = label_ids
        .get(label)
        .expect("every jump target is numbered before emission");
    format!("label {id}")
}
