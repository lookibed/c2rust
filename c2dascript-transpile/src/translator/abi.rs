//! Canonical daScript ABI conversions shared by translator lowering paths.
//!
//! C pointers remain typed `T?` in translated program expressions.  Raw
//! `uint64` addresses are an implementation detail used only at explicit ABI
//! boundaries such as the raw-memory runtime and pointer comparisons.

use super::*;

/// `null` is the zero value of every nullable daScript type, which includes
/// the named function-pointer types as well as `T?`.  This deliberately does
/// not assert on the kind: a debug-only panic here made the debug build reject
/// programs the release build translated.
pub(crate) fn null_pointer(_pointer: &DaType) -> DaExpr {
    DaExpr::ConstNull
}

/// The single C numeric conversion table.
///
/// C6.3.1.1 integer promotion followed by C6.3.1.8 usual arithmetic
/// conversions.  Every arithmetic operand in the translator is first raised to
/// its *promoted* daScript type through [`promoted_arith_type`], the two
/// promoted types are combined by [`usual_arithmetic_type`], and the result is
/// narrowed back to the C storage type only when it is stored.
///
/// daScript defines `+ - * / % & | ^ << >>` for `int/uint/int64/uint64/
/// float/double` only, so this table doubles as the operand-legality rule:
/// nothing narrower than 32 bits ever reaches an operator.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CArith {
    Int,
    UInt,
    Int64,
    UInt64,
    Float,
    Double,
}

impl CArith {
    pub(crate) fn da_type(self) -> DaType {
        match self {
            CArith::Int => DaType::int(),
            CArith::UInt => DaType::uint(),
            CArith::Int64 => DaType::int64(),
            CArith::UInt64 => DaType::uint64(),
            CArith::Float => DaType::float(),
            CArith::Double => DaType::double(),
        }
    }

    fn rank(self) -> u8 {
        match self {
            CArith::Int | CArith::UInt => 1,
            CArith::Int64 | CArith::UInt64 => 2,
            CArith::Float => 3,
            CArith::Double => 4,
        }
    }

    fn is_unsigned(self) -> bool {
        matches!(self, CArith::UInt | CArith::UInt64)
    }

    fn is_float(self) -> bool {
        matches!(self, CArith::Float | CArith::Double)
    }
}

/// C6.3.1.1: the type an operand of this C type has after integer promotion.
///
/// Everything with a conversion rank below `int` (`_Bool`, `char`, `signed
/// char`, `unsigned char`, `short`, `unsigned short`) promotes to `int`,
/// because `int` represents every one of their values on this target.
pub(crate) fn promoted_arith_type(kind: &CTypeKind) -> Option<CArith> {
    use CTypeKind::*;
    Some(match kind {
        Bool | Char | SChar | UChar | Short | UShort | Int8 | UInt8 | Int16 | UInt16 | Int
        | Int32 | WChar => CArith::Int,
        UInt | UInt32 => CArith::UInt,
        Long | LongLong | Int64 | IntPtr | SSize | PtrDiff | IntMax => CArith::Int64,
        ULong | ULongLong | UInt64 | UIntPtr | Size | UIntMax => CArith::UInt64,
        Float | BFloat16 => CArith::Float,
        Double => CArith::Double,
        _ => return None,
    })
}

/// C6.3.1.8: the common type of two already-promoted operands.
pub(crate) fn usual_arithmetic_type(lhs: CArith, rhs: CArith) -> CArith {
    if lhs == rhs {
        return lhs;
    }
    if lhs.is_float() || rhs.is_float() {
        return if lhs.rank() >= rhs.rank() { lhs } else { rhs };
    }
    match lhs.rank().cmp(&rhs.rank()) {
        std::cmp::Ordering::Greater => lhs,
        std::cmp::Ordering::Less => rhs,
        // Equal rank, different signedness: the unsigned type wins.
        std::cmp::Ordering::Equal => {
            if lhs.is_unsigned() {
                lhs
            } else {
                rhs
            }
        }
    }
}

/// True for C types whose daScript storage is narrower than an arithmetic
/// operand, so that a value must be widened before and narrowed after an
/// operator.
pub(crate) fn is_narrow_c_type(kind: &CTypeKind) -> bool {
    use CTypeKind::*;
    matches!(
        kind,
        Bool | Char | SChar | UChar | Short | UShort | Int8 | UInt8 | Int16 | UInt16
    )
}

impl<'c> Translation<'c> {
    /// The promoted C arithmetic type of an operand, computed once from the
    /// Clang type rather than re-inferred from generated daScript.
    pub(crate) fn arith_type_of(&self, ty: CQualTypeId) -> Option<CArith> {
        let kind = self.ast_context.resolve_type(ty.ctype).kind.clone();
        self.arith_type_of_kind(&kind)
    }

    pub(crate) fn arith_type_of_kind(&self, kind: &CTypeKind) -> Option<CArith> {
        if let CTypeKind::Enum(enum_id) = kind {
            let underlying = match self.ast_context[*enum_id].kind {
                CDeclKind::Enum { integral_type, .. } => integral_type,
                _ => None,
            };
            return match underlying {
                Some(qty) => self.arith_type_of(qty),
                None => Some(CArith::Int),
            };
        }
        promoted_arith_type(kind)
    }

    /// Raise a translated operand to its promoted C arithmetic type.
    /// A no-op when the daScript storage type already is that type.
    pub(crate) fn promote_operand(&self, expr: DaExpr, from: &CTypeKind, to: CArith) -> DaExpr {
        let target = to.da_type();
        if Self::infer_type(&expr)
            .map(super::writable_type)
            .as_ref()
            == Some(&target)
        {
            return expr;
        }
        let storage = self.storage_type_of_kind(from);
        if storage.as_ref() == Some(&target) {
            return expr;
        }
        DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(expr),
            to: target,
        }
    }

    /// The daScript type a value of this C type is *stored* in.
    fn storage_type_of_kind(&self, kind: &CTypeKind) -> Option<DaType> {
        if matches!(kind, CTypeKind::Enum(_)) {
            return self.arith_type_of_kind(kind).map(CArith::da_type);
        }
        promoted_arith_type(kind).map(|_| type_kind_to_datype(kind))
    }

    /// Narrow an arithmetic result back into a C storage type, as C does on
    /// assignment (C6.3.1.3 modular conversion).
    pub(crate) fn narrow_to_storage(&self, expr: DaExpr, target: &DaType) -> DaExpr {
        if Self::infer_type(&expr).as_ref() == Some(target) {
            return expr;
        }
        DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(expr),
            to: target.clone(),
        }
    }

    /// Canonical typed integer literal boundary for runtime parameters.
    pub(crate) fn integer_literal_for_type(&self, expr: DaExpr, target: DaType) -> DaExpr {
        DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(strip_numeric_literal_casts(expr)),
            to: target,
        }
    }

    /// daScript has no scalar bool-to-number conversion. Materialize C's 0/1
    /// value in statements before another expression consumes it.
    pub(crate) fn bool_to_integer_cast(&self, expr: DaExpr) -> Option<(Vec<DaStmt>, DaExpr)> {
        let DaExpr::Cast { kind, expr, to } = expr else { return None; };
        let bool_expr = unwrap_numeric_casts(expr);
        if kind != das_ast::CastKind::Cast
            || !to.is_numeric()
            || matches!(to.kind, DaTypeKind::Bool)
            || !Self::infer_type(&bool_expr).map_or(false, |ty| matches!(ty.kind, DaTypeKind::Bool))
        {
            return None;
        }
        let tmp = self.renamer.borrow_mut().fresh();
        let one = self.integer_literal_for_type(DaExpr::ConstInt(1), to.clone());
        let zero = self.integer_literal_for_type(DaExpr::ConstInt(0), to.clone());
        Some((
            vec![
                DaStmt::Var {
                    name: tmp.clone(),
                    var_type: to,
                    init: Some(zero),
                },
                mk().expr_stmt(DaExpr::IfThenElse {
                    cond: Box::new(bool_expr),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![DaStmt::Expr(DaExpr::Assign(
                            Box::new(DaExpr::Var(tmp.clone())),
                            Box::new(one),
                        ))],
                    })),
                    elifs: vec![],
                    else_: None,
                }),
            ],
            DaExpr::Var(tmp),
        ))
    }

    /// `var t : T = 0; if (b) { t = 1 }` — C's 0/1 value of a boolean.
    pub(crate) fn materialize_bool_as_number(
        &self,
        value: DaExpr,
        target: DaType,
    ) -> (Vec<DaStmt>, DaExpr) {
        let tmp = self.renamer.borrow_mut().fresh();
        let one = self.integer_literal_for_type(DaExpr::ConstInt(1), target.clone());
        let zero = self.integer_literal_for_type(DaExpr::ConstInt(0), target.clone());
        (
            vec![
                DaStmt::Var {
                    name: tmp.clone(),
                    var_type: target,
                    init: Some(zero),
                },
                mk().expr_stmt(DaExpr::IfThenElse {
                    cond: Box::new(self.as_bool_condition(value)),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![DaStmt::Expr(DaExpr::Assign(
                            Box::new(DaExpr::Var(tmp.clone())),
                            Box::new(one),
                        ))],
                    })),
                    elifs: vec![],
                    else_: None,
                }),
            ],
            DaExpr::Var(tmp),
        )
    }

    pub(crate) fn bool_to_integer(&self, value: WithStmts<DaExpr>) -> WithStmts<DaExpr> {
        let is_unsafe = value.is_unsafe;
        let mut stmts = value.stmts;
        let expr = value.val;
        if let Some((lowered_stmts, lowered_val)) = self.bool_to_integer_cast(expr.clone()) {
            stmts.extend(lowered_stmts);
            WithStmts::new(stmts, lowered_val).merge_unsafe(is_unsafe)
        } else {
            WithStmts::new(stmts, expr).merge_unsafe(is_unsafe)
        }
    }
    /// The target may be any nullable daScript type — `T?` for a C object
    /// pointer, a named function type for a C function pointer.
    pub(crate) fn raw_address_to_pointer(&self, raw_address: DaExpr, pointer: DaType) -> DaExpr {
        DaExpr::Unsafe(Box::new(DaExpr::Cast {
            kind: das_ast::CastKind::Reinterpret,
            expr: Box::new(raw_address),
            to: pointer,
        }))
    }

    pub(crate) fn pointer_to_raw_address(&self, pointer: DaExpr) -> DaExpr {
        DaExpr::Unsafe(Box::new(DaExpr::Cast {
            kind: das_ast::CastKind::Reinterpret,
            expr: Box::new(pointer),
            to: DaType::uint64(),
        }))
    }

    /// Reinterpret a value already represented as a daScript pointer (or an
    /// array-decay value) to another typed C pointer.  Null stays null rather
    /// than becoming an invalid numeric pointer cast.
    pub(crate) fn abi_pointer_cast(&self, pointer: DaExpr, target: DaType) -> DaExpr {
        if matches!(pointer, DaExpr::ConstNull) {
            return self.null_pointer(&target);
        }
        DaExpr::Unsafe(Box::new(DaExpr::Cast {
            kind: das_ast::CastKind::Reinterpret,
            expr: Box::new(pointer),
            to: target,
        }))
    }

    pub(crate) fn null_pointer(&self, pointer: &DaType) -> DaExpr {
        null_pointer(pointer)
    }

    pub(crate) fn abi_pointer_comparison_operand(&self, expr: DaExpr, is_pointer: bool) -> DaExpr {
        if matches!(expr, DaExpr::ConstNull) {
            return DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(DaExpr::ConstInt(0)),
                to: DaType::uint64(),
            };
        }
        if is_pointer {
            self.pointer_to_raw_address(expr)
        } else {
            DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(expr),
                to: DaType::uint64(),
            }
        }
    }
}

fn strip_numeric_literal_casts(expr: DaExpr) -> DaExpr {
    match expr {
        DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr,
            to,
        } if to.is_numeric() => {
            let inner = strip_numeric_literal_casts(*expr);
            if matches!(inner, DaExpr::ConstInt(_) | DaExpr::ConstUInt(_)) {
                inner
            } else {
                DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(inner),
                    to,
                }
            }
        }
        expr => expr,
    }
}

fn unwrap_numeric_casts(mut expr: Box<DaExpr>) -> DaExpr {
    loop {
        match *expr {
            DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: inner,
                to,
            } if to.is_numeric() && !matches!(to.kind, DaTypeKind::Bool) => expr = inner,
            other => return other,
        }
    }
}
