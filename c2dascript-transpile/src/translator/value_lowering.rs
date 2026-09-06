//! Canonical lowering from a translated daScript expression to a C value use-site.

use super::*;

#[derive(Clone, Copy, Debug)]
pub(crate) enum ValueSite {
    Return,
    Assignment,
    CallArg,
    BinaryOperand,
    BinaryResult,
}

impl<'c> Translation<'c> {
    /// Materialize semantics which C assigns to a value at a typed use-site,
    /// but daScript does not perform implicitly.
    pub(crate) fn lower_to_c_value(
        &self,
        value: WithStmts<DaExpr>,
        source: Option<CQualTypeId>,
        target: DaType,
        _site: ValueSite,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let actual = Self::infer_type(&value.val);
        if actual
            .as_ref()
            .map_or(false, |ty| matches!(ty.kind, DaTypeKind::Bool))
            && target.is_numeric()
            && !matches!(target.kind, DaTypeKind::Bool)
        {
            // C already gives the expression a type — `int` for `!x` and for
            // `a && b`. Materializing the 0/1 in `target` instead would give
            // the operand a different type from the one C assigns it, and
            // daScript has no implicit conversion to reconcile the two.
            let materialized = source
                .map(|ty| self.convert_type(ty).map(writable_type))
                .transpose()?
                .filter(|ty| ty.is_numeric() && !matches!(ty.kind, DaTypeKind::Bool))
                .unwrap_or(target);
            let cast = DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(value.val),
                to: materialized,
            };
            let is_unsafe = value.is_unsafe;
            let mut stmts = value.stmts;
            let (lowered_stmts, lowered_value) = self
                .bool_to_integer_cast(cast)
                .expect("bool-to-numeric cast must lower to statements");
            stmts.extend(lowered_stmts);
            return Ok(WithStmts::new(stmts, lowered_value).merge_unsafe(is_unsafe));
        }

        // A C value narrower than the use-site's type still has to be widened
        // explicitly: daScript performs no implicit numeric conversion.  The
        // widening is a plain cast — never a cast-stripping rewrite, because
        // an explicit narrowing cast such as `(unsigned char)300` is part of
        // the C value, not noise.
        let source_kind = source.map(|ty| self.ast_context.resolve_type(ty.ctype).kind.clone());
        // A C `_Bool` value used as a number is 0 or 1.  daScript has no
        // conversion from bool at all, so it has to be materialized through
        // control flow before any consumer sees it.
        if matches!(source_kind, Some(CTypeKind::Bool))
            && target.is_numeric()
            && !matches!(target.kind, DaTypeKind::Bool | DaTypeKind::Auto)
        {
            let is_unsafe = value.is_unsafe;
            let mut stmts = value.stmts;
            let (lowered_stmts, lowered_value) = self.materialize_bool_as_number(value.val, target);
            stmts.extend(lowered_stmts);
            return Ok(WithStmts::new(stmts, lowered_value).merge_unsafe(is_unsafe));
        }
        if let Some(kind) = source_kind {
            let storage = super::type_kind_to_datype(&kind);
            if target.is_numeric()
                && !matches!(target.kind, DaTypeKind::Bool | DaTypeKind::Auto)
                && abi::is_narrow_c_type(&kind)
                && !matches!(kind, CTypeKind::Bool)
                && storage != target
            {
                let target_ty = target.clone();
                return Ok(value.map(|expr| match Self::infer_type(&expr) {
                    Some(inferred) if inferred == target_ty => expr,
                    _ => DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(expr),
                        to: target_ty,
                    },
                }));
            }
        }

        Ok(value)
    }
}
