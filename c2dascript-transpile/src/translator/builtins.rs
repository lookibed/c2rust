//! Builtin function translation.
//!
//! Every builtin here is lowered to daScript code with the same observable
//! behaviour as the C builtin. A builtin without such a lowering is a
//! `TranslationError`: replacing one with a constant would silently change the
//! meaning of the translated program.
use super::*;
use crate::format_translation_err;
use std::cell::RefCell;
use std::collections::BTreeMap;

// Prelude helpers required by the builtins actually used in this translation
// unit. Like the string-literal pool, this lives in a thread-local because a
// `Translation` is created and consumed inside a single `translate_impl` call,
// which clears the set beforehand and drains it afterwards.
thread_local! {
    static REQUIRED_HELPERS: RefCell<BTreeMap<String, DaDecl>> = RefCell::new(BTreeMap::new());
}

/// Clears the helper set at the start of a translation unit.
pub fn reset_builtin_helpers() {
    REQUIRED_HELPERS.with(|helpers| helpers.borrow_mut().clear());
}

/// Returns the prelude declarations for every builtin helper used by this
/// translation unit, and empties the set.
pub fn take_builtin_helper_declarations() -> Vec<DaDecl> {
    REQUIRED_HELPERS.with(|helpers| {
        std::mem::take(&mut *helpers.borrow_mut())
            .into_values()
            .collect()
    })
}

/// Registers `name` as a required helper, building its declaration once, and
/// returns a call to it.
fn helper_call(name: &str, build: impl FnOnce() -> DaDecl, args: Vec<DaExpr>) -> DaExpr {
    REQUIRED_HELPERS.with(|helpers| {
        helpers
            .borrow_mut()
            .entry(name.to_owned())
            .or_insert_with(build);
    });
    DaExpr::Call(Box::new(DaExpr::Var(name.to_owned())), args)
}

fn param(name: &str, param_type: DaType, is_mutable: bool) -> DaStmt {
    DaStmt::Param {
        name: name.to_owned(),
        param_type,
        default: None,
        is_mutable,
    }
}

fn helper_fn(name: &str, params: Vec<DaStmt>, ret_type: DaType, stmts: Vec<DaStmt>) -> DaDecl {
    DaDecl::Function(DaFunction {
        name: name.to_owned(),
        params,
        ret_type,
        body: Some(DaExpr::Block(DaBlock { stmts })),
        annotations: vec![],
        is_public: false,
        is_unsafe: false,
    })
}

fn var(name: &str) -> DaExpr {
    DaExpr::Var(name.to_owned())
}

fn op2(op: &'static str, left: DaExpr, right: DaExpr) -> DaExpr {
    DaExpr::Op2 {
        op,
        left: Box::new(left),
        right: Box::new(right),
    }
}

fn cast_to(expr: DaExpr, to: DaType) -> DaExpr {
    DaExpr::Cast {
        kind: das_ast::CastKind::Cast,
        expr: Box::new(expr),
        to,
    }
}

fn ret(value: DaExpr) -> DaStmt {
    DaStmt::Expr(DaExpr::Return(Some(Box::new(value))))
}

fn block(stmts: Vec<DaStmt>) -> Box<DaExpr> {
    Box::new(DaExpr::Block(DaBlock { stmts }))
}

/// `v` with its bytes reversed, over `width` bytes of a `uint`/`uint64` value.
fn byte_swap_expr(value: DaExpr, width: u32, ty: &DaType) -> DaExpr {
    let shift_lit = |bits: u64| cast_to(DaExpr::ConstUInt(bits), DaType::uint());
    let mask = |byte: u32| {
        let mut m: u64 = 0xff;
        m <<= 8 * byte;
        cast_to(DaExpr::ConstUInt(m), ty.clone())
    };
    let mut result: Option<DaExpr> = None;
    for source in 0..width {
        let target = width - 1 - source;
        let shifted = if target > source {
            op2(
                "<<",
                value.clone(),
                shift_lit(8 * (target - source) as u64),
            )
        } else if target < source {
            op2(
                ">>",
                value.clone(),
                shift_lit(8 * (source - target) as u64),
            )
        } else {
            value.clone()
        };
        let byte = op2("&", shifted, mask(target));
        result = Some(match result {
            None => byte,
            Some(acc) => op2("|", acc, byte),
        });
    }
    result.unwrap_or_else(|| cast_to(DaExpr::ConstUInt(0), ty.clone()))
}

fn byte_swap_helper(name: &str, width: u32, ty: DaType) -> DaDecl {
    helper_fn(
        name,
        vec![param("v", ty.clone(), false)],
        ty.clone(),
        vec![ret(byte_swap_expr(var("v"), width, &ty))],
    )
}

/// `__builtin_ffs`: one plus the index of the least significant set bit, or
/// zero when the argument is zero.
fn find_first_set_helper(name: &str, ty: DaType, unsigned: DaType) -> DaDecl {
    helper_fn(
        name,
        vec![param("v", ty.clone(), false)],
        DaType::int(),
        vec![
            DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(op2("==", var("v"), cast_to(DaExpr::ConstInt(0), ty.clone()))),
                then: block(vec![ret(DaExpr::ConstInt(0))]),
                elifs: vec![],
                else_: None,
            }),
            ret(op2(
                "+",
                cast_to(
                    DaExpr::Call(
                        Box::new(var("ctz")),
                        vec![cast_to(var("v"), unsigned)],
                    ),
                    DaType::int(),
                ),
                DaExpr::ConstInt(1),
            )),
        ],
    )
}

/// `+Inf`, produced at run time so the value is a real infinity rather than a
/// finite approximation of one.
fn huge_value_helper(name: &str, ty: DaType) -> DaDecl {
    let zero = if matches!(ty.kind, DaTypeKind::Double) {
        DaExpr::ConstDouble(0.0)
    } else {
        DaExpr::ConstFloat(0.0)
    };
    let one = if matches!(ty.kind, DaTypeKind::Double) {
        DaExpr::ConstDouble(1.0)
    } else {
        DaExpr::ConstFloat(1.0)
    };
    helper_fn(
        name,
        vec![],
        ty.clone(),
        vec![
            // The division is performed at run time on purpose: a literal
            // `1.0 / 0.0` would be a compile-time error rather than +Inf.
            DaStmt::Var {
                name: "zero".to_owned(),
                var_type: ty.clone(),
                init: Some(zero),
            },
            DaStmt::Var {
                name: "one".to_owned(),
                var_type: ty,
                init: Some(one),
            },
            ret(op2("/", var("one"), var("zero"))),
        ],
    )
}

fn floating_zero(ty: &DaType) -> DaExpr {
    if matches!(ty.kind, DaTypeKind::Double) {
        DaExpr::ConstDouble(0.0)
    } else {
        DaExpr::ConstFloat(0.0)
    }
}

fn absolute_value_helper(name: &str, ty: DaType) -> DaDecl {
    helper_fn(
        name,
        vec![param("x", ty.clone(), false)],
        ty.clone(),
        vec![
            DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(op2("<", var("x"), floating_zero(&ty))),
                then: block(vec![ret(DaExpr::Op1 {
                    op: "-",
                    expr: Box::new(var("x")),
                })]),
                elifs: vec![],
                else_: None,
            }),
            ret(var("x")),
        ],
    )
}

/// `__builtin_signbit`: nonzero exactly when the sign bit is set, which
/// includes negative zero — hence the reciprocal test on zero.
fn sign_bit_helper(name: &str, ty: DaType) -> DaDecl {
    let one = if matches!(ty.kind, DaTypeKind::Double) {
        DaExpr::ConstDouble(1.0)
    } else {
        DaExpr::ConstFloat(1.0)
    };
    helper_fn(
        name,
        vec![param("x", ty.clone(), false)],
        DaType::int(),
        vec![
            DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(op2("<", var("x"), floating_zero(&ty))),
                then: block(vec![ret(DaExpr::ConstInt(1))]),
                elifs: vec![],
                else_: None,
            }),
            DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(op2(
                    "&&",
                    op2("==", var("x"), floating_zero(&ty)),
                    op2("<", op2("/", one, var("x")), floating_zero(&ty)),
                )),
                then: block(vec![ret(DaExpr::ConstInt(1))]),
                elifs: vec![],
                else_: None,
            }),
            ret(DaExpr::ConstInt(0)),
        ],
    )
}

/// `__builtin_{add,sub,mul}_overflow` for a narrow integer type: the operation
/// is performed in a wider type, the truncated result is stored through the
/// out-pointer, and the wide result is compared against its own truncation to
/// detect the overflow.
fn overflow_helper(name: &str, op: &'static str, narrow: DaType, wide: DaType) -> DaDecl {
    helper_fn(
        name,
        vec![
            param("a", narrow.clone(), false),
            param("b", narrow.clone(), false),
            param("res", DaType::pointer(narrow.clone()), true),
        ],
        // The C builtin has type `_Bool`; the translator lowers a `_Bool`
        // value through the boolean path, so the helper must return `bool`.
        DaType::bool(),
        vec![
            DaStmt::Var {
                name: "wide".to_owned(),
                var_type: wide.clone(),
                init: Some(op2(
                    op,
                    cast_to(var("a"), wide.clone()),
                    cast_to(var("b"), wide.clone()),
                )),
            },
            DaStmt::Expr(DaExpr::Unsafe(block(vec![DaStmt::Expr(DaExpr::Assign(
                Box::new(DaExpr::Deref(Box::new(var("res")))),
                Box::new(cast_to(var("wide"), narrow.clone())),
            ))]))),
            ret(op2(
                "!=",
                cast_to(cast_to(var("wide"), narrow), wide),
                var("wide"),
            )),
        ],
    )
}

/// The wider type an overflow check for `narrow` is evaluated in. A 64-bit
/// operand has no wider daScript type, so it has no lowering here.
fn overflow_wide_type(narrow: &DaType) -> Option<DaType> {
    match narrow.kind {
        DaTypeKind::Int | DaTypeKind::Int8 | DaTypeKind::Int16 => Some(DaType::int64()),
        DaTypeKind::UInt | DaTypeKind::UInt8 | DaTypeKind::UInt16 => Some(DaType::uint64()),
        _ => None,
    }
}

impl<'c> Translation<'c> {
    pub fn convert_builtin_call(
        &self,
        ctx: ExprContext,
        fexp: CExprId,
        args: &[CExprId],
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let builtin_name = match &self.ast_context[fexp].kind {
            CExprKind::DeclRef(_, decl_id, _) => self.ast_context[*decl_id]
                .kind
                .get_name()
                .cloned()
                .unwrap_or_default(),
            _ => return self.convert_expr(ctx, fexp, None),
        };
        let loc = self.ast_context.display_loc(&self.ast_context[fexp].loc);

        // `__builtin_constant_p` never evaluates its argument, and answering
        // "not a compile-time constant" is a conforming implementation.
        if builtin_name == "__builtin_constant_p" {
            return Ok(WithStmts::new_val(DaExpr::ConstInt(0)));
        }

        let mut das_args = vec![];
        let mut arg_stmts = vec![];
        let mut is_unsafe = false;
        for &arg in args {
            let a = self.convert_expr(ctx.used(), arg, None)?;
            is_unsafe |= a.is_unsafe();
            let (stmts, val) = a.into_stmts_and_val();
            arg_stmts.extend(stmts);
            das_args.push(val);
        }
        let finish = |value: DaExpr| {
            Ok(WithStmts::new_val(value)
                .prepend_stmts(arg_stmts.clone())
                .merge_unsafe(is_unsafe))
        };

        // __builtin_expect is a branch-prediction hint; its C value is exactly
        // its first argument, including any statements that argument hoisted.
        if builtin_name == "__builtin_expect" {
            let Some(value) = das_args.into_iter().next() else {
                return Err(format_translation_err!(
                    loc,
                    "__builtin_expect requires an argument"
                ));
            };
            return finish(value);
        }

        // `__builtin_assume_aligned(p, ...)` evaluates to its pointer operand.
        if builtin_name == "__builtin_assume_aligned" {
            let Some(value) = das_args.into_iter().next() else {
                return Err(format_translation_err!(
                    loc,
                    "__builtin_assume_aligned requires an argument"
                ));
            };
            return finish(value);
        }

        // A prefetch has no effect on the observable behaviour of the program;
        // its operands are still evaluated.
        if builtin_name == "__builtin_prefetch" {
            return finish(DaExpr::ConstNull);
        }

        // Clang represents ordinary malloc calls through its builtin path on
        // some targets.  That classification must not bypass the canonical
        // raw-memory ABI used by normal direct calls.
        if builtin_name == "malloc" || builtin_name.ends_with("_malloc") {
            let size = das_args.into_iter().next().unwrap_or(DaExpr::ConstUInt(0));
            let raw_call = DaExpr::Call(
                Box::new(DaExpr::Var("c2da_rt_malloc".to_owned())),
                vec![cast_to(size, DaType::uint64())],
            );
            return finish(self.raw_address_to_pointer(raw_call, DaType::pointer(DaType::void())));
        }

        // Rotate builtins. daScript spells rotate-left `<<<` and
        // rotate-right `>>>`; they are not interchangeable.
        if builtin_name.starts_with("__builtin_rotateleft") {
            return self.convert_builtin_rotate(&builtin_name, &loc, das_args, arg_stmts, is_unsafe, "<<<");
        }
        if builtin_name.starts_with("__builtin_rotateright") {
            return self.convert_builtin_rotate(&builtin_name, &loc, das_args, arg_stmts, is_unsafe, ">>>");
        }

        if builtin_name.ends_with("_overflow") {
            let op = if builtin_name.contains("add") {
                "+"
            } else if builtin_name.contains("sub") {
                "-"
            } else if builtin_name.contains("mul") {
                "*"
            } else {
                return Err(format_translation_err!(
                    loc,
                    "unsupported builtin {}",
                    builtin_name
                ));
            };
            return self.convert_overflow_arith(
                &builtin_name,
                &loc,
                op,
                args,
                das_args,
                arg_stmts,
                is_unsafe,
            );
        }

        let result = match builtin_name.as_str() {
            "__builtin_popcount" | "__builtin_popcountl" | "__builtin_popcountll" => {
                let width = if builtin_name == "__builtin_popcount" {
                    DaType::uint()
                } else {
                    DaType::uint64()
                };
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                cast_to(
                    DaExpr::Call(Box::new(var("popcnt")), vec![cast_to(arg, width)]),
                    DaType::int(),
                )
            }
            "__builtin_clz" | "__builtin_clzl" | "__builtin_clzll" => {
                let width = if builtin_name == "__builtin_clz" {
                    DaType::uint()
                } else {
                    DaType::uint64()
                };
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                cast_to(
                    DaExpr::Call(Box::new(var("clz")), vec![cast_to(arg, width)]),
                    DaType::int(),
                )
            }
            "__builtin_ctz" | "__builtin_ctzl" | "__builtin_ctzll" => {
                let width = if builtin_name == "__builtin_ctz" {
                    DaType::uint()
                } else {
                    DaType::uint64()
                };
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                cast_to(
                    DaExpr::Call(Box::new(var("ctz")), vec![cast_to(arg, width)]),
                    DaType::int(),
                )
            }
            "__builtin_ffs" | "__builtin_ffsl" | "__builtin_ffsll" => {
                let (name, narrow, unsigned) = if builtin_name == "__builtin_ffs" {
                    ("c2da_ffs_int", DaType::int(), DaType::uint())
                } else {
                    ("c2da_ffs_int64", DaType::int64(), DaType::uint64())
                };
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                helper_call(
                    name,
                    || find_first_set_helper(name, narrow.clone(), unsigned.clone()),
                    vec![cast_to(arg, narrow.clone())],
                )
            }
            "__builtin_bswap16" | "__builtin_bswap32" | "__builtin_bswap64" => {
                let (name, width, ty, result_ty) = match builtin_name.as_str() {
                    "__builtin_bswap16" => {
                        ("c2da_bswap16", 2u32, DaType::uint(), DaType::uint16())
                    }
                    "__builtin_bswap32" => ("c2da_bswap32", 4, DaType::uint(), DaType::uint()),
                    _ => ("c2da_bswap64", 8, DaType::uint64(), DaType::uint64()),
                };
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                let swapped = helper_call(
                    name,
                    || byte_swap_helper(name, width, ty.clone()),
                    vec![cast_to(arg, ty.clone())],
                );
                if result_ty == ty {
                    swapped
                } else {
                    cast_to(swapped, result_ty)
                }
            }
            "__builtin_huge_valf" | "__builtin_inff" => helper_call(
                "c2da_huge_val_float",
                || huge_value_helper("c2da_huge_val_float", DaType::float()),
                vec![],
            ),
            "__builtin_huge_val" | "__builtin_huge_vall" | "__builtin_inf" | "__builtin_infl" => {
                helper_call(
                    "c2da_huge_val_double",
                    || huge_value_helper("c2da_huge_val_double", DaType::double()),
                    vec![],
                )
            }
            "__builtin_fabsf" => {
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                helper_call(
                    "c2da_fabs_float",
                    || absolute_value_helper("c2da_fabs_float", DaType::float()),
                    vec![arg],
                )
            }
            "__builtin_fabs" | "__builtin_fabsl" => {
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                helper_call(
                    "c2da_fabs_double",
                    || absolute_value_helper("c2da_fabs_double", DaType::double()),
                    vec![arg],
                )
            }
            "__builtin_signbit" | "__builtin_signbitl" => {
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                helper_call(
                    "c2da_signbit_double",
                    || sign_bit_helper("c2da_signbit_double", DaType::double()),
                    vec![cast_to(arg, DaType::double())],
                )
            }
            "__builtin_signbitf" => {
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                helper_call(
                    "c2da_signbit_float",
                    || sign_bit_helper("c2da_signbit_float", DaType::float()),
                    vec![cast_to(arg, DaType::float())],
                )
            }
            // A NaN is the only value that compares unequal to itself.
            "__builtin_isnan" => {
                let arg = self.builtin_arg(&builtin_name, &loc, &das_args, 0)?;
                cast_to(op2("!=", arg.clone(), arg), DaType::int())
            }
            "__builtin_unreachable" => {
                DaExpr::Call(Box::new(var("panic")), vec![DaExpr::ConstString(
                    "__builtin_unreachable".to_owned(),
                )])
            }
            _ => {
                return Err(format_translation_err!(
                    loc,
                    "unsupported builtin {}",
                    builtin_name
                ));
            }
        };
        finish(result)
    }

    /// Positional builtin argument, or a located error when the call does not
    /// have one.
    fn builtin_arg(
        &self,
        builtin_name: &str,
        loc: &Option<crate::c_ast::DisplaySrcSpan>,
        das_args: &[DaExpr],
        index: usize,
    ) -> TranslationResult<DaExpr> {
        das_args.get(index).cloned().ok_or_else(|| {
            format_translation_err!(
                loc.clone(),
                "builtin {} is missing argument {}",
                builtin_name,
                index
            )
        })
    }

    fn convert_builtin_rotate(
        &self,
        builtin_name: &str,
        loc: &Option<crate::c_ast::DisplaySrcSpan>,
        das_args: Vec<DaExpr>,
        arg_stmts: Vec<DaStmt>,
        is_unsafe: bool,
        op: &'static str,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        if das_args.len() < 2 {
            return Err(format_translation_err!(
                loc.clone(),
                "builtin {} requires two arguments",
                builtin_name
            ));
        }
        let mut args = das_args.into_iter();
        let value = args.next().expect("checked above");
        let amount = args.next().expect("checked above");
        Ok(WithStmts::new_val(op2(op, value, amount))
            .prepend_stmts(arg_stmts)
            .merge_unsafe(is_unsafe))
    }

    fn convert_overflow_arith(
        &self,
        builtin_name: &str,
        loc: &Option<crate::c_ast::DisplaySrcSpan>,
        op: &'static str,
        args: &[CExprId],
        das_args: Vec<DaExpr>,
        arg_stmts: Vec<DaStmt>,
        is_unsafe: bool,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        if args.len() != 3 || das_args.len() != 3 {
            return Err(format_translation_err!(
                loc.clone(),
                "builtin {} requires two operands and a result pointer",
                builtin_name
            ));
        }
        // The arithmetic type is the type the result is stored into, exactly
        // as the C builtin defines it.
        let result_ty = self.ast_context[args[2]]
            .kind
            .get_qual_type()
            .and_then(|qty| match self.ast_context.resolve_type(qty.ctype).kind {
                CTypeKind::Pointer(pointee) => Some(pointee),
                _ => None,
            })
            .ok_or_else(|| {
                format_translation_err!(
                    loc.clone(),
                    "builtin {} result argument is not a pointer",
                    builtin_name
                )
            })?;
        let narrow = writable_type(self.convert_type(result_ty)?);
        let Some(wide) = overflow_wide_type(&narrow) else {
            return Err(format_translation_err!(
                loc.clone(),
                "unsupported builtin {}: no daScript type wider than {} to detect the overflow in",
                builtin_name,
                narrow
            ));
        };
        let helper_name = format!("c2da_{}_overflow_{}", overflow_op_name(op), narrow);
        let mut args = das_args.into_iter();
        let lhs = args.next().expect("checked above");
        let rhs = args.next().expect("checked above");
        let out = args.next().expect("checked above");
        let call = {
            let build_name = helper_name.clone();
            let build_narrow = narrow.clone();
            helper_call(
                &helper_name,
                move || overflow_helper(&build_name, op, build_narrow, wide),
                vec![cast_to(lhs, narrow.clone()), cast_to(rhs, narrow), out],
            )
        };
        Ok(WithStmts::new_val(call)
            .prepend_stmts(arg_stmts)
            .merge_unsafe(is_unsafe)
            .set_unsafe())
    }
}

fn overflow_op_name(op: &str) -> &'static str {
    match op {
        "+" => "add",
        "-" => "sub",
        _ => "mul",
    }
}
