//! Pointer operation translation — полный порт c2rust pointers.rs
use super::*;

impl<'c> Translation<'c> {
    pub fn convert_address_of(
        &self,
        mut ctx: ExprContext,
        cqual_type: CQualTypeId,
        arg: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        // &*x → x
        if let CExprKind::Unary(_, CUnOp::Deref, target, _) = &self.ast_context[arg].kind {
            return self.convert_expr(ctx, *target, Some(cqual_type));
        }
        let inner = self.convert_expr(ctx.used(), arg, None)?;
        let is_unsafe = inner.is_unsafe;
        Ok(
            WithStmts::new_val(DaExpr::Unsafe(Box::new(DaExpr::Addr(Box::new(inner.val)))))
                .prepend_stmts(inner.stmts)
                .merge_unsafe(is_unsafe),
        )
    }

    pub fn convert_deref(
        &self,
        ctx: ExprContext,
        cqual_type: CQualTypeId,
        arg: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        // *&x → x
        if let CExprKind::Unary(_, CUnOp::AddressOf, target, _) = &self.ast_context[arg].kind {
            return self.convert_expr(ctx.used(), *target, Some(cqual_type));
        }
        let inner = self.convert_expr(ctx.used(), arg, None)?;
        let is_unsafe = inner.is_unsafe;
        Ok(
            WithStmts::new_val(DaExpr::Unsafe(Box::new(DaExpr::Deref(Box::new(inner.val)))))
                .prepend_stmts(inner.stmts)
                .merge_unsafe(is_unsafe),
        )
    }

    pub fn convert_array_subscript(
        &self,
        ctx: ExprContext,
        lhs: CExprId,
        rhs: CExprId,
        qual_ty: CQualTypeId,
        override_ty: Option<CQualTypeId>,
        _deref: bool,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let lhs_val = self.convert_expr(ctx, lhs, Some(qual_ty))?;
        let rhs_val = self.convert_expr(ctx, rhs, None)?;
        let is_ptr = self.is_pointer_type(qual_ty.ctype);
        let rhs_expr = self.subscript_index_operand(rhs_val.val);
        let expr = DaExpr::Index(Box::new(lhs_val.val), Box::new(rhs_expr));
        let expr = if is_ptr {
            DaExpr::Unsafe(Box::new(expr))
        } else {
            expr
        };
        let is_unsafe = lhs_val.is_unsafe || rhs_val.is_unsafe;
        let mut stmts = lhs_val.stmts;
        stmts.extend(rhs_val.stmts);
        if let Some(expected_ty) = override_ty {
            let ty = self.convert_type(expected_ty)?;
            Ok(WithStmts::new_val(DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(expr),
                to: ty,
            })
            .prepend_stmts(stmts)
            .merge_unsafe(is_unsafe))
        } else {
            Ok(WithStmts::new_val(expr)
                .prepend_stmts(stmts)
                .merge_unsafe(is_unsafe))
        }
    }

    /// Pointer arithmetik: ptr [+/-] offset → reinterpret<uint64>(ptr) +/- offset*sizeof(T)
    pub fn convert_pointer_offset(
        &self,
        ptr: DaExpr,
        offset: DaExpr,
        pointee_cty: CTypeId,
        neg: bool,
        _deref: bool,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let off = if neg {
            DaExpr::Op2 {
                op: "-",
                left: Box::new(DaExpr::ConstInt(0)),
                right: Box::new(offset),
            }
        } else {
            offset
        };
        let stride = self.sizeof_type(pointee_cty)?;
        Ok(WithStmts::new_val(DaExpr::Op2 {
            op: "+",
            left: Box::new(ptr),
            right: Box::new(DaExpr::Op2 {
                op: "*",
                left: Box::new(off),
                right: Box::new(DaExpr::ConstInt(stride)),
            }),
        })
        .set_unsafe())
    }

    pub fn null_ptr(&self, _type_id: CTypeId) -> TranslationResult<DaExpr> {
        Ok(self.null_pointer(&DaType::pointer(DaType::void())))
    }

    pub fn convert_pointer_is_null(&self, val: DaExpr, is_null: bool) -> TranslationResult<DaExpr> {
        Ok(if is_null {
            DaExpr::Op2 {
                op: "==",
                left: Box::new(val),
                right: Box::new(self.null_pointer(&DaType::pointer(DaType::void()))),
            }
        } else {
            DaExpr::Op2 {
                op: "!=",
                left: Box::new(val),
                right: Box::new(self.null_pointer(&DaType::pointer(DaType::void()))),
            }
        })
    }

    pub fn convert_pointer_to_pointer_cast(
        &self,
        _source_cty: CTypeId,
        _target_cty: CTypeId,
        val: WithStmts<DaExpr>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        Ok(val)
    }

    pub fn convert_integral_to_pointer_cast(
        &self,
        _ctx: ExprContext,
        _source_cty: CTypeId,
        _target_cty: CTypeId,
        val: WithStmts<DaExpr>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        Ok(val)
    }

    pub fn convert_pointer_to_integral_cast(
        &self,
        _ctx: ExprContext,
        _source_cty: CTypeId,
        _target_cty: CTypeId,
        val: WithStmts<DaExpr>,
        _expr: Option<CExprId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        Ok(val)
    }

    pub fn convert_array_to_pointer_decay(
        &self,
        _ctx: ExprContext,
        _source_cty: CQualTypeId,
        _target_cty: CQualTypeId,
        val: WithStmts<DaExpr>,
        _expr: Option<CExprId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        Ok(val)
    }
}
