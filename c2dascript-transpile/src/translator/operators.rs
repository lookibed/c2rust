//! Operator translation — полный порт c2rust operators.rs
use super::*;

impl<'c> Translation<'c> {
    /// Main binary expression handler.
    pub fn convert_binary_expr(
        &self,
        mut ctx: ExprContext,
        expr_type_id: CQualTypeId,
        op: CBinOp,
        lhs: CExprId,
        rhs: CExprId,
        opt_lhs_type_id: Option<CQualTypeId>,
        opt_rhs_type_id: Option<CQualTypeId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        use CBinOp::*;

        // Comma: the value of the LHS is discarded, its side effects are not.
        if matches!(op, Comma) {
            let lhs_val = self.convert_expr(ctx.unused(), lhs, None)?;
            let is_unsafe = lhs_val.is_unsafe;
            let mut stmts = lhs_val.stmts;
            stmts.extend(self.discard_value_stmt(lhs_val.val));
            return Ok(self
                .convert_expr(ctx, rhs, Some(expr_type_id))?
                .prepend_stmts(stmts)
                .merge_unsafe(is_unsafe));
        }

        // Logical ops: &&, ||
        if op.is_logical() {
            return self.convert_short_circuit(ctx, op, lhs, rhs);
        }

        // Assignment ops: =, +=, -=, etc.
        if op.is_assignment() {
            return self.convert_assignment_operator(
                ctx,
                op,
                expr_type_id,
                lhs,
                rhs,
                opt_lhs_type_id,
                opt_rhs_type_id,
            );
        }

        // Regular binary ops
        let is_ptr = self.is_pointer_type(expr_type_id.ctype);

        // The operand's own Clang type is authoritative: Clang has already
        // inserted the integer promotions C requires, so re-deriving the type
        // from the underlying declaration would undo them.
        let lhs_expr_type_id = self.ast_context[lhs].kind.get_qual_type();
        let rhs_expr_type_id = self.ast_context[rhs].kind.get_qual_type();
        let lhs_type_id = lhs_expr_type_id.or(opt_lhs_type_id);
        let rhs_type_id = rhs_expr_type_id.or(opt_rhs_type_id);
        let lhs_kind = lhs_type_id.map(|q| self.ast_context.resolve_type(q.ctype).kind.clone());
        let rhs_kind = rhs_type_id.map(|q| self.ast_context.resolve_type(q.ctype).kind.clone());

        // Canonical numeric path: both operands are C arithmetic values, so
        // the C usual arithmetic conversions in abi.rs decide the one type the
        // daScript operator runs in.
        let lhs_is_ptr_c = lhs_kind.as_ref().map_or(false, |k| k.is_pointer());
        let rhs_is_ptr_c = rhs_kind.as_ref().map_or(false, |k| k.is_pointer());
        if !is_ptr && !lhs_is_ptr_c && !rhs_is_ptr_c {
            if let (Some(lk), Some(rk)) = (lhs_kind.as_ref(), rhs_kind.as_ref()) {
                if let (Some(la), Some(ra)) =
                    (self.arith_type_of_kind(lk), self.arith_type_of_kind(rk))
                {
                    return self.convert_arithmetic_binop(
                        ctx, op, lhs, rhs, lhs_type_id, rhs_type_id, lk, rk, la, ra,
                    );
                }
            }
        }
        let lhs_is_uint = lhs_kind
            .as_ref()
            .map_or(false, |k| k.is_unsigned_integral_type());
        let rhs_is_uint = rhs_kind
            .as_ref()
            .map_or(false, |k| k.is_unsigned_integral_type());
        let lhs_is_int = lhs_kind
            .as_ref()
            .map_or(false, |k| k.is_signed_integral_type());
        let rhs_is_int = rhs_kind
            .as_ref()
            .map_or(false, |k| k.is_signed_integral_type());
        let needs_coerce = (lhs_is_uint && rhs_is_int) || (rhs_is_uint && lhs_is_int);

        // An operand's value is always used, whatever happens to the value of
        // the operator itself: `(a = b) + 1` needs the assignment hoisted into
        // a statement so the operand is the assigned variable, because
        // daScript assignment is a statement and has no value.
        let lhs_val = self.convert_expr(ctx.used(), lhs, lhs_type_id)?;
        let rhs_val = self.convert_expr(ctx.used(), rhs, rhs_type_id)?;
        let lhs_da_from_c = lhs_type_id
            .map(|q| self.convert_type(q).map(writable_type))
            .transpose()?;
        let rhs_da_from_c = rhs_type_id
            .map(|q| self.convert_type(q).map(writable_type))
            .transpose()?;
        let lhs_val = materialize_expr_type(lhs_val, lhs_da_from_c.as_ref());
        let rhs_val = materialize_expr_type(rhs_val, rhs_da_from_c.as_ref());

        // Infer daScript types from the actual converted expressions (more accurate than C AST types,
        // because C type promotion can hide type mismatches that daScript rejects).
        let lhs_da = Self::infer_type(&lhs_val.val)
            .or(lhs_da_from_c.clone())
            .or_else(|| lhs_kind.as_ref().map(|k| type_kind_to_datype(k)));
        let rhs_da = Self::infer_type(&rhs_val.val)
            .or(rhs_da_from_c.clone())
            .or_else(|| rhs_kind.as_ref().map(|k| type_kind_to_datype(k)));

        // Check if either operand is a pointer (ptr - ptr returns int64, not pointer)
        let lhs_is_ptr = lhs_kind.as_ref().map_or(false, |k| k.is_pointer());
        let rhs_is_ptr = rhs_kind.as_ref().map_or(false, |k| k.is_pointer());
        let any_ptr = lhs_is_ptr || rhs_is_ptr;

        let width_mismatch = lhs_da.is_some()
            && rhs_da.is_some()
            && lhs_da != rhs_da
            && !is_ptr
            && !any_ptr
            && !matches!(op, CBinOp::Comma);

        let coerce_target = if width_mismatch { lhs_da.clone() } else { None };
        let (lhs_val, rhs_val) = if let Some(ref target) = coerce_target {
            (
                lhs_val,
                rhs_val.map(|v| DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(v),
                    to: target.clone(),
                }),
            )
        } else if needs_coerce {
            if lhs_is_uint {
                (
                    lhs_val,
                    rhs_val.map(|v| DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(v),
                        to: DaType::uint(),
                    }),
                )
            } else {
                (
                    lhs_val.map(|v| DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(v),
                        to: DaType::uint(),
                    }),
                    rhs_val,
                )
            }
        } else {
            (lhs_val, rhs_val)
        };
        // C comparison results are integer-like, whereas daScript comparison
        // expressions are bool.  A numeric coercion above may therefore have
        // produced `int(bool)`, which daScript rejects.  Lower it here, at the
        // binary-expression owner, before the enclosing operator consumes it.
        let lhs_val = self.bool_to_integer(lhs_val);
        let rhs_val = self.bool_to_integer(rhs_val);

        // Fallback: if LHS and RHS map to different daScript types, cast RHS to LHS type
        let type_diff =
            lhs_da.is_some() && rhs_da.is_some() && lhs_da != rhs_da && !is_ptr && !any_ptr;

        match op {
            // daScript compares two `T?` values directly.  Only a mixed
            // pointer/integer comparison needs the raw-address ABI.
            EqualEqual | NotEqual if lhs_is_ptr && rhs_is_ptr => {
                let das_op = convert_binop(op).map_err(TranslationError::generic)?;
                Ok(lhs_val
                    .zip(rhs_val)
                    .map(|(l, r)| DaExpr::Unsafe(Box::new(mk().binary_op(das_op, l, r)))))
            }
            EqualEqual | NotEqual if any_ptr => {
                let das_op = convert_binop(op).map_err(TranslationError::generic)?;
                Ok(lhs_val.zip(rhs_val).map(|(l, r)| {
                    DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                        op: das_op,
                        left: Box::new(self.abi_pointer_comparison_operand(l, lhs_is_ptr)),
                        right: Box::new(self.abi_pointer_comparison_operand(r, rhs_is_ptr)),
                    }))
                }))
            }
            Less | Greater | LessEqual | GreaterEqual if any_ptr => {
                let das_op = convert_binop(op).map_err(TranslationError::generic)?;
                Ok(lhs_val.zip(rhs_val).map(|(l, r)| {
                    DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                        op: das_op,
                        left: Box::new(self.abi_pointer_comparison_operand(l, lhs_is_ptr)),
                        right: Box::new(self.abi_pointer_comparison_operand(r, rhs_is_ptr)),
                    }))
                }))
            }
            Add => {
                // C `n + p` is `p + n`; daScript only defines `T? + integer`.
                let (lhs_val, rhs_val, lhs_is_ptr) = if rhs_is_ptr && !lhs_is_ptr {
                    (rhs_val, lhs_val, true)
                } else {
                    (lhs_val, rhs_val, lhs_is_ptr)
                };
                let result = self.convert_addition(lhs_val, rhs_val, expr_type_id, lhs_is_ptr)?;
                if type_diff && !matches!(result.val, DaExpr::Unsafe(_)) {
                    let target = lhs_da.clone().unwrap();
                    Ok(result.map(|v| match v {
                        DaExpr::Op2 { op, left, right } => {
                            let right_cast = DaExpr::Cast {
                                kind: das_ast::CastKind::Cast,
                                expr: right,
                                to: target,
                            };
                            DaExpr::Op2 {
                                op,
                                left,
                                right: Box::new(right_cast),
                            }
                        }
                        v => v,
                    }))
                } else {
                    Ok(result)
                }
            }
            Subtract => {
                let sub = self.convert_subtraction(lhs_val, rhs_val, expr_type_id, lhs_is_ptr)?;
                let needs_unsafe = any_ptr && !matches!(sub.val, DaExpr::Unsafe(_));
                let sub = if needs_unsafe {
                    sub.map(|v| DaExpr::Unsafe(Box::new(v)))
                } else {
                    sub
                };
                if type_diff && !matches!(sub.val, DaExpr::Unsafe(_)) {
                    let target = lhs_da.clone().unwrap();
                    Ok(sub.map(|v| match v {
                        DaExpr::Op2 { op, left, right } => {
                            let right_cast = DaExpr::Cast {
                                kind: das_ast::CastKind::Cast,
                                expr: right,
                                to: target,
                            };
                            DaExpr::Op2 {
                                op,
                                left,
                                right: Box::new(right_cast),
                            }
                        }
                        v => v,
                    }))
                } else {
                    Ok(sub)
                }
            }
            ShiftLeft | ShiftRight => {
                // daScript << / >> требуют ОДИНАКОВЫЙ тип для обоих операндов,
                // и определены только для int/uint/int64/uint64.
                // Если LHS — меньший тип (int8, uint16...), поднимаем оба до int/uint.
                // Если типы разные (uint64 >> uint), приводим RHS к типу LHS.
                let das_op = convert_binop(op).map_err(TranslationError::generic)?;
                let target_da_type = coerce_shift_types(&lhs_kind, &rhs_kind);
                let (lhs_val, rhs_val) = if let Some(ty) = target_da_type {
                    let lhs_casted = lhs_val.map(|v| DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(v),
                        to: ty.clone(),
                    });
                    let rhs_casted = rhs_val.map(|v| DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(v),
                        to: ty,
                    });
                    (lhs_casted, rhs_casted)
                } else {
                    (lhs_val, rhs_val)
                };
                let combined = lhs_val
                    .zip(rhs_val)
                    .map(|(l, r)| mk().binary_op(das_op, l, r));
                Ok(if is_ptr {
                    combined.map(|v| DaExpr::Unsafe(Box::new(v)))
                } else {
                    combined
                })
            }
            _ => {
                let das_op = convert_binop(op).map_err(TranslationError::generic)?;
                // Late coercion: if inferred daScript types differ after all C-level
                // coercions, cast RHS to match LHS. This catches cases where C type
                // promotion hides the actual daScript type difference (e.g., uint == 0
                // where both sides are `uint` in C but `uint` vs `int` in daScript).
                let lhs_inf = Self::infer_type(&lhs_val.val)
                    .or(lhs_da_from_c.clone())
                    .or_else(|| lhs_kind.as_ref().map(|k| type_kind_to_datype(k)));
                let rhs_inf = Self::infer_type(&rhs_val.val)
                    .or(rhs_da_from_c.clone())
                    .or_else(|| rhs_kind.as_ref().map(|k| type_kind_to_datype(k)));
                let (lhs_val, rhs_val) = if let (Some(ref lt), Some(ref rt)) = (lhs_inf, rhs_inf) {
                    if lt != rt && !is_ptr && !matches!(op, CBinOp::Comma) {
                        (
                            lhs_val,
                            rhs_val.map(|v| DaExpr::Cast {
                                kind: das_ast::CastKind::Cast,
                                expr: Box::new(v),
                                to: lt.clone(),
                            }),
                        )
                    } else {
                        (lhs_val, rhs_val)
                    }
                } else {
                    (lhs_val, rhs_val)
                };
                let combined = lhs_val
                    .zip(rhs_val)
                    .map(|(l, r)| mk().binary_op(das_op, l, r));
                Ok(if is_ptr {
                    combined.map(|v| DaExpr::Unsafe(Box::new(v)))
                } else {
                    combined
                })
            }
        }
    }

    /// C `&&` / `||` evaluate the right operand only if the left one did not
    /// already decide the result.  daScript's own `&&`/`||` short-circuit, so
    /// a pure right operand needs nothing; a right operand that had to hoist
    /// statements is lowered into an `if` guarded by the left operand.
    fn convert_short_circuit(
        &self,
        ctx: ExprContext,
        op: CBinOp,
        lhs: CExprId,
        rhs: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let das_op = convert_binop(op).map_err(TranslationError::generic)?;
        let lhs_val = self.convert_condition(ctx, true, lhs)?;
        let rhs_val = self.convert_condition(ctx, true, rhs)?;
        if rhs_val.is_pure() {
            return Ok(lhs_val
                .zip(rhs_val)
                .map(|(l, r)| mk().binary_op(das_op, l, r)));
        }
        // C gives `&&` and `||` type `int` with value 0 or 1.  The temporary
        // carries that type, so no consumer has to rediscover it.
        //   a && b  ->  var t : int = 0; if (a) { <b's stmts>; if (b) t = 1 }
        //   a || b  ->  var t : int = 1; if (!a) { <b's stmts>; if (!b) t = 0 }
        let is_and = matches!(op, CBinOp::And);
        let tmp = self.renamer.borrow_mut().fresh();
        let tmp_var = DaExpr::Var(tmp.clone());
        let is_unsafe = lhs_val.is_unsafe || rhs_val.is_unsafe;
        let (seed, settled) = if is_and { (0, 1) } else { (1, 0) };
        let mut stmts = lhs_val.stmts;
        stmts.push(DaStmt::Var {
            name: tmp,
            var_type: DaType::int(),
            init: Some(DaExpr::ConstInt(seed)),
        });
        let mut guarded = rhs_val.stmts;
        let rhs_cond = if is_and {
            rhs_val.val
        } else {
            mk().unary_op("!", rhs_val.val)
        };
        guarded.push(DaStmt::Expr(DaExpr::IfThenElse {
            cond: Box::new(rhs_cond),
            then: Box::new(DaExpr::Block(DaBlock {
                stmts: vec![DaStmt::Expr(DaExpr::Assign(
                    Box::new(tmp_var.clone()),
                    Box::new(DaExpr::ConstInt(settled)),
                ))],
            })),
            elifs: vec![],
            else_: None,
        }));
        let lhs_cond = if is_and {
            lhs_val.val
        } else {
            mk().unary_op("!", lhs_val.val)
        };
        stmts.push(DaStmt::Expr(DaExpr::IfThenElse {
            cond: Box::new(lhs_cond),
            then: Box::new(DaExpr::Block(DaBlock { stmts: guarded })),
            elifs: vec![],
            else_: None,
        }));
        Ok(WithStmts::new(stmts, tmp_var).merge_unsafe(is_unsafe))
    }

    /// The one arithmetic lowering: both operands are raised to the common
    /// type the C usual arithmetic conversions select, and the daScript
    /// operator runs entirely in that type.
    #[allow(clippy::too_many_arguments)]
    fn convert_arithmetic_binop(
        &self,
        ctx: ExprContext,
        op: CBinOp,
        lhs: CExprId,
        rhs: CExprId,
        lhs_type_id: Option<CQualTypeId>,
        rhs_type_id: Option<CQualTypeId>,
        lhs_kind: &CTypeKind,
        rhs_kind: &CTypeKind,
        lhs_arith: abi::CArith,
        rhs_arith: abi::CArith,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let das_op = convert_binop(op).map_err(TranslationError::generic)?;
        // An operand's value is always consumed by the operator, even when
        // the operator's own result is discarded: `(a = b) + 1` must lower
        // the assignment as a statement whose value is then read.
        let lhs_val = self.convert_expr(ctx.used(), lhs, lhs_type_id)?;
        let rhs_val = self.convert_expr(ctx.used(), rhs, rhs_type_id)?;
        // A shift keeps the promoted type of its left operand; only the
        // operands' daScript spelling has to agree.
        let (lhs_target, rhs_target) = if matches!(op, CBinOp::ShiftLeft | CBinOp::ShiftRight) {
            (lhs_arith, lhs_arith)
        } else {
            let common = abi::usual_arithmetic_type(lhs_arith, rhs_arith);
            (common, common)
        };
        let lhs_val = self.lower_to_c_value(
            lhs_val,
            lhs_type_id,
            lhs_target.da_type(),
            ValueSite::BinaryOperand,
        )?;
        let rhs_val = self.lower_to_c_value(
            rhs_val,
            rhs_type_id,
            rhs_target.da_type(),
            ValueSite::BinaryOperand,
        )?;
        let lhs_val =
            self.bool_to_integer(lhs_val.map(|v| self.promote_operand(v, lhs_kind, lhs_target)));
        let rhs_val =
            self.bool_to_integer(rhs_val.map(|v| self.promote_operand(v, rhs_kind, rhs_target)));
        Ok(lhs_val
            .zip(rhs_val)
            .map(|(l, r)| mk().binary_op(das_op, l, r)))
    }

    #[allow(dead_code)]
    fn expr_operand_type(&self, expr_id: CExprId) -> Option<CQualTypeId> {
        match &self.ast_context[expr_id].kind {
            CExprKind::Member(_, _, field_id, _, _) => match &self.ast_context[*field_id].kind {
                CDeclKind::Field { typ, .. } => Some(*typ),
                _ => self.ast_context[expr_id].kind.get_qual_type(),
            },
            CExprKind::DeclRef(_, decl_id, _) => match &self.ast_context[*decl_id].kind {
                CDeclKind::Variable { typ, .. } | CDeclKind::Field { typ, .. } => Some(*typ),
                _ => self.ast_context[expr_id].kind.get_qual_type(),
            },
            _ => self.ast_context[expr_id].kind.get_qual_type(),
        }
    }

    #[allow(dead_code)]
    fn storage_byte_source_type(&self, expr_id: CExprId) -> Option<CQualTypeId> {
        let source = match &self.ast_context[expr_id].kind {
            CExprKind::ImplicitCast(_, inner, _, _, _) => self.storage_byte_source_type(*inner),
            _ => self.ast_context[expr_id].kind.get_qual_type(),
        }?;
        matches!(
            self.ast_context.resolve_type(source.ctype).kind,
            CTypeKind::UInt8 | CTypeKind::UChar
        )
        .then_some(source)
    }

    /// Handle assignment operator.
    /// Разворачивает chain assignment (a=b=c) и if-as-expression (x=if(c)a else b)
    fn convert_assignment_operator(
        &self,
        ctx: ExprContext,
        op: CBinOp,
        expr_type_id: CQualTypeId,
        lhs: CExprId,
        rhs: CExprId,
        compute_lhs_type_id: Option<CQualTypeId>,
        _compute_res_type_id: Option<CQualTypeId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let is_used = ctx.used;
        // The storage type of the assignment target.  Clang's "computation
        // LHS type" is the *promoted* type the arithmetic runs in, which is a
        // different thing and is consulted separately below.
        let lhs_type_id = self.ast_context[lhs]
            .kind
            .get_qual_type()
            .or(compute_lhs_type_id)
            .unwrap_or(expr_type_id);
        let lhs_kind = self
            .ast_context
            .resolve_type(lhs_type_id.ctype)
            .kind
            .clone();
        let lhs_da_type = self.convert_type(lhs_type_id)?;

        // Address-backed member writes need their own store operation: a
        // packed C field cannot be represented by a daScript lvalue at all.
        // Keep the address as a first-class object until `raw_store` selects
        // typed indexing or statement-level memcpy.
        let raw_member = match self.ast_context[lhs].kind.clone() {
            CExprKind::Member(_, base_expr, field, member_kind, _) => {
                if let Some(base) = self.member_place_address(ctx.used(), base_expr)? {
                    Some((field, self.field_address(base, field)?))
                } else if matches!(member_kind, MemberKind::Arrow) {
                    let base_ty = self.ast_context[base_expr]
                        .kind
                        .get_qual_type()
                        .ok_or_else(|| TranslationError::generic("member pointer has no C type"))?;
                    let base = self.convert_expr(ctx.used(), base_expr, Some(base_ty))?;
                    Some((field, self.pointer_member_address(base, base_ty, field)?))
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some((field, address)) = raw_member {
            let rhs_id = rhs;
            let is_bitfield = matches!(
                self.ast_context[field].kind,
                CDeclKind::Field {
                    bitfield_width: Some(_),
                    ..
                }
            );
            if op == CBinOp::Assign {
                let rhs = self.convert_expr(ctx.used(), rhs_id, Some(lhs_type_id))?;
                let rhs = self.lower_to_c_value(
                    rhs,
                    self.ast_context[rhs_id].kind.get_qual_type(),
                    lhs_da_type,
                    ValueSite::Assignment,
                )?;
                return if is_bitfield {
                    self.bitfield_store(address, field, rhs)
                } else {
                    self.raw_store(address, rhs)
                };
            }
            let inner_op = op
                .underlying_assignment()
                .ok_or_else(|| TranslationError::generic("not a compound assignment"))?;
            let das_op = convert_binop(inner_op).map_err(TranslationError::generic)?;
            let current = if is_bitfield {
                self.bitfield_load(address.clone(), field)?
            } else {
                self.raw_load(address.clone())?
            };
            let rhs = self.convert_expr(ctx.used(), rhs_id, Some(lhs_type_id))?;
            let value = current
                .zip(rhs)
                .map(|(left, right)| mk().binary_op(das_op, left, right));
            return if is_bitfield {
                self.bitfield_store(address, field, value)
            } else {
                self.raw_store(address, value)
            };
        }
        let lhs_val = self.convert_lvalue_once(ctx, lhs, lhs_type_id)?;

        if op != CBinOp::Assign {
            // C says `a op= b` is `a = (typeof a)(a op b)` with the usual
            // arithmetic conversions applied to the operation, and that `a` is
            // evaluated exactly once.
            let inner_op = op
                .underlying_assignment()
                .ok_or_else(|| TranslationError::generic("not a compound assignment"))?;
            let das_op = convert_binop(inner_op).map_err(TranslationError::generic)?;
            let is_ptr_op = lhs_kind.is_pointer() || self.is_pointer_type(lhs_type_id.ctype);
            let rhs_ty = self.ast_context[rhs].kind.get_qual_type();
            let rhs_val = self.convert_expr(ctx.used(), rhs, if is_ptr_op { None } else { rhs_ty })?;
            if is_ptr_op {
                let value = rhs_val.map(|offset| {
                    DaExpr::Unsafe(Box::new(mk().binary_op(
                        das_op,
                        lhs_val.val.clone(),
                        self.pointer_offset_operand(offset),
                    )))
                });
                let place = lhs_val.val.clone();
                let stmts = lhs_val.stmts;
                let is_unsafe = lhs_val.is_unsafe || value.is_unsafe;
                let value_stmts = value.stmts;
                let assign = DaExpr::Assign(Box::new(place.clone()), Box::new(value.val));
                return Ok(lower_assignment_expr(assign, place, is_used)
                    .prepend_stmts(value_stmts)
                    .prepend_stmts(stmts)
                    .merge_unsafe(is_unsafe));
            }
            let lhs_arith = self.arith_type_of_kind(&lhs_kind).ok_or_else(|| {
                TranslationError::generic("compound assignment to non-arithmetic C type")
            })?;
            let rhs_kind = rhs_ty
                .map(|q| self.ast_context.resolve_type(q.ctype).kind.clone())
                .ok_or_else(|| {
                    TranslationError::generic("compound assignment right operand has no C type")
                })?;
            let rhs_arith = self.arith_type_of_kind(&rhs_kind).ok_or_else(|| {
                TranslationError::generic("compound assignment from non-arithmetic C type")
            })?;
            // Clang already computed the type C performs the operation in;
            // fall back to the conversion table when it is absent.  A shift
            // keeps the promoted type of the left operand.
            let common = match compute_lhs_type_id.and_then(|q| self.arith_type_of(q)) {
                Some(computed) => computed,
                None if matches!(inner_op, CBinOp::ShiftLeft | CBinOp::ShiftRight) => lhs_arith,
                None => abi::usual_arithmetic_type(lhs_arith, rhs_arith),
            };
            let rhs_val =
                self.bool_to_integer(rhs_val.map(|v| self.promote_operand(v, &rhs_kind, common)));
            let place = lhs_val.val.clone();
            let promoted_place = self.promote_operand(place.clone(), &lhs_kind, common);
            let stmts = lhs_val.stmts;
            let is_unsafe = lhs_val.is_unsafe || rhs_val.is_unsafe;
            let rhs_stmts = rhs_val.stmts;
            let value = self.narrow_to_storage(
                mk().binary_op(das_op, promoted_place, rhs_val.val),
                &writable_type(lhs_da_type.clone()),
            );
            let assign = DaExpr::Assign(Box::new(place.clone()), Box::new(value));
            return Ok(lower_assignment_expr(assign, place, is_used)
                .prepend_stmts(rhs_stmts)
                .prepend_stmts(stmts)
                .merge_unsafe(is_unsafe));
        }

        // Chain assignment: a = b = c → b = c; a = b
        let mut rhs_val = self.convert_expr(ctx.used(), rhs, Some(lhs_type_id))?;
        if let Some(stripped_rhs) =
            self.strip_const_deref_assignment_rhs(ctx, rhs, lhs_type_id, &lhs_da_type)?
        {
            rhs_val = stripped_rhs;
        }
        rhs_val = self.lower_to_c_value(
            rhs_val,
            self.ast_context[rhs].kind.get_qual_type(),
            lhs_da_type.clone(),
            ValueSite::Assignment,
        )?;
        if let DaExpr::Assign(inner_lhs, inner_rhs) = &rhs_val.val {
            let mut stmts = lhs_val.stmts;
            stmts.extend(rhs_val.stmts.clone());
            stmts.push(DaStmt::Expr(DaExpr::Assign(
                Box::new(*inner_lhs.clone()),
                Box::new(*inner_rhs.clone()),
            )));
            let lhs_expr = lhs_val.val;
            let assign = DaExpr::Assign(Box::new(lhs_expr.clone()), Box::new(*inner_lhs.clone()));
            return Ok(lower_assignment_expr(assign, lhs_expr, is_used)
                .prepend_stmts(stmts)
                .merge_unsafe(lhs_val.is_unsafe || rhs_val.is_unsafe));
        }

        // if-as-expression в RHS: x = if (c) a else b
        // → var __tmp; if (c) __tmp = a else __tmp = b; x = __tmp
        if let DaExpr::IfThenElse {
            cond,
            then,
            elifs,
            else_,
        } = &rhs_val.val
        {
            let mut stmts = lhs_val.stmts;
            stmts.extend(rhs_val.stmts.clone());
            let tmp = "_tmp_assign";
            let tmp_var = DaExpr::Var(tmp.to_string());
            // Создаём if-else STATEMENT (не expression) с присваиванием во временную
            let then_assign = DaStmt::Expr(DaExpr::Assign(
                Box::new(tmp_var.clone()),
                Box::new(*then.clone()),
            ));
            let else_assign = else_.as_ref().map(|el| {
                DaStmt::Expr(DaExpr::Assign(
                    Box::new(tmp_var.clone()),
                    Box::new(*el.clone()),
                ))
            });

            if let Some(el_assign) = else_assign {
                stmts.push(DaStmt::Var {
                    name: tmp.to_string(),
                    var_type: DaType::int(),
                    init: Some(*then.clone()),
                });
                stmts.push(DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(*cond.clone()),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![then_assign],
                    })),
                    elifs: elifs.clone(),
                    else_: Some(Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![el_assign],
                    }))),
                }));
            } else {
                stmts.push(DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(*cond.clone()),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![then_assign],
                    })),
                    elifs: elifs.clone(),
                    else_: None,
                }));
            }

            let lhs_expr = lhs_val.val;
            let assign = DaExpr::Assign(Box::new(lhs_expr.clone()), Box::new(tmp_var));
            return Ok(lower_assignment_expr(assign, lhs_expr, is_used)
                .prepend_stmts(stmts)
                .merge_unsafe(lhs_val.is_unsafe || rhs_val.is_unsafe));
        }

        let mut stmts = lhs_val.stmts;
        stmts.extend(rhs_val.stmts);
        let lhs_expr = lhs_val.val;
        let rhs_expr = self.coerce_assignment_value(rhs_val.val, &lhs_kind, &lhs_da_type);
        let assign = DaExpr::Assign(Box::new(lhs_expr.clone()), Box::new(rhs_expr));
        Ok(lower_assignment_expr(assign, lhs_expr, is_used)
            .prepend_stmts(stmts)
            .merge_unsafe(lhs_val.is_unsafe || rhs_val.is_unsafe))
    }

    /// Strip the C nodes that do not change an lvalue's identity.
    fn strip_lvalue_wrappers(&self, mut expr: CExprId) -> CExprId {
        loop {
            match self.ast_context[expr].kind {
                CExprKind::Paren(_, inner) => expr = inner,
                CExprKind::Unary(_, CUnOp::Extension, inner, _) => expr = inner,
                _ => return expr,
            }
        }
    }

    /// Convert an assignment target so that C's "the lvalue is evaluated
    /// exactly once" rule holds.  `*get_cell() += 5` must call `get_cell`
    /// once, so the address it produced becomes a temporary the read and the
    /// write share.
    pub(crate) fn convert_lvalue_once(
        &self,
        ctx: ExprContext,
        expr_id: CExprId,
        expr_type_id: CQualTypeId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let stripped = self.strip_lvalue_wrappers(expr_id);
        if let CExprKind::Unary(_, CUnOp::Deref, ptr, _) = self.ast_context[stripped].kind {
            let ptr_ty = self.ast_context[ptr]
                .kind
                .get_qual_type()
                .ok_or_else(|| TranslationError::generic("dereferenced value has no C type"))?;
            let pointer = self.convert_expr(ctx.used(), ptr, Some(ptr_ty))?;
            let pointer = self.materialize_place_once(pointer, ptr_ty)?;
            let is_unsafe = pointer.is_unsafe;
            return Ok(pointer
                .map(|v| DaExpr::Unsafe(Box::new(DaExpr::Deref(Box::new(v)))))
                .merge_unsafe(is_unsafe));
        }
        self.convert_expr(ctx, expr_id, Some(expr_type_id))
    }

    /// Bind a place-producing expression to a temporary unless re-evaluating
    /// it is provably free of effects.
    fn materialize_place_once(
        &self,
        value: WithStmts<DaExpr>,
        ty: CQualTypeId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        fn is_stable(expr: &DaExpr) -> bool {
            match expr {
                DaExpr::Var(_) | DaExpr::ConstNull => true,
                DaExpr::Field(base, _) | DaExpr::SafeField(base, _) => is_stable(base),
                DaExpr::Unsafe(inner) => is_stable(inner),
                _ => false,
            }
        }
        if is_stable(&value.val) {
            return Ok(value);
        }
        let tmp = self.renamer.borrow_mut().fresh();
        let var_type = writable_type(self.convert_type(ty)?);
        let is_unsafe = value.is_unsafe;
        let mut stmts = value.stmts;
        stmts.push(DaStmt::Var {
            name: tmp.clone(),
            var_type,
            init: Some(value.val),
        });
        Ok(WithStmts::new(stmts, DaExpr::Var(tmp)).merge_unsafe(is_unsafe))
    }

    /// daScript scales `T? + n` by the pointee size exactly as C does; the
    /// offset only has to be a signed integer so that `p - 1` stays negative.
    pub(crate) fn pointer_offset_operand(&self, offset: DaExpr) -> DaExpr {
        match Self::infer_type(&offset) {
            Some(ty) if matches!(ty.kind, DaTypeKind::Int | DaTypeKind::Int64) => offset,
            _ => DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(offset),
                to: DaType::int64(),
            },
        }
    }

    fn strip_const_deref_assignment_rhs(
        &self,
        ctx: ExprContext,
        rhs: CExprId,
        expr_type_id: CQualTypeId,
        lhs_da_type: &DaType,
    ) -> TranslationResult<Option<WithStmts<DaExpr>>> {
        if !matches!(lhs_da_type.kind, DaTypeKind::Named(_)) {
            return Ok(None);
        }
        let Some(ptr_expr) = self.const_deref_pointer_expr(rhs) else {
            return Ok(None);
        };
        let Some(ptr_qty) = self.ast_context[ptr_expr].kind.get_qual_type() else {
            return Ok(None);
        };
        let CTypeKind::Pointer(pointee) = self.ast_context.resolve_type(ptr_qty.ctype).kind else {
            return Ok(None);
        };
        if !pointee.qualifiers.is_const {
            return Ok(None);
        }
        let target_ty = self.convert_type(expr_type_id)?;
        if writable_type(target_ty.clone()) != writable_type(lhs_da_type.clone()) {
            return Ok(None);
        }
        let ptr_val = self.convert_expr(ctx.used(), ptr_expr, None)?;
        let mutable_ptr_ty = DaType::pointer(writable_type(target_ty));
        Ok(Some(
            WithStmts::new_val(DaExpr::Unsafe(Box::new(DaExpr::Deref(Box::new(
                self.abi_pointer_cast(ptr_val.val, mutable_ptr_ty),
            )))))
            .prepend_stmts(ptr_val.stmts)
            .merge_unsafe(true),
        ))
    }

    fn const_deref_pointer_expr(&self, expr: CExprId) -> Option<CExprId> {
        match self.ast_context[expr].kind {
            CExprKind::Unary(_, CUnOp::Deref, ptr_expr, _) => Some(ptr_expr),
            CExprKind::ImplicitCast(_, inner, _, _, _)
            | CExprKind::ExplicitCast(_, inner, _, _, _)
            | CExprKind::Paren(_, inner) => self.const_deref_pointer_expr(inner),
            _ => None,
        }
    }

    /// Addition with pointer arithmetic support and type coercion.
    fn convert_addition(
        &self,
        lhs: WithStmts<DaExpr>,
        rhs: WithStmts<DaExpr>,
        expr_type_id: CQualTypeId,
        lhs_is_ptr: bool,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let is_ptr_lhs = lhs_is_ptr || self.is_pointer_type(expr_type_id.ctype);
        if is_ptr_lhs {
            Ok(lhs.zip(rhs).map(|(l, r)| {
                if let DaExpr::Unsafe(inner) = l {
                    if let DaExpr::Op2 {
                        op: "+",
                        left,
                        right,
                    } = *inner
                    {
                        return DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                            op: "+",
                            left,
                            right: Box::new(DaExpr::Op2 {
                                op: "+",
                                left: right,
                                right: Box::new(r),
                            }),
                        }));
                    }
                    return DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                        op: "+",
                        left: Box::new(DaExpr::Unsafe(inner)),
                        right: Box::new(r),
                    }));
                }
                DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                    op: "+",
                    left: Box::new(l),
                    right: Box::new(
                        self.pointer_offset_operand(normalize_numeric_binop_tree(r)),
                    ),
                }))
            }))
        } else {
            // Inline type coercion: if RHS is clearly a different type than LHS, wrap RHS in cast
            let lhs_ty = Self::infer_type(&lhs.val);
            let rhs_ty = Self::infer_type(&rhs.val);
            if let (Some(lt), Some(rt)) = (&lhs_ty, &rhs_ty) {
                if lt != rt {
                    return Ok(lhs.zip(rhs).map(|(l, r)| {
                        let r_casted = DaExpr::Cast {
                            kind: das_ast::CastKind::Cast,
                            expr: Box::new(r),
                            to: lt.clone(),
                        };
                        mk().binary_op("+", l, r_casted)
                    }));
                }
            }
            Ok(lhs.zip(rhs).map(|(l, r)| mk().binary_op("+", l, r)))
        }
    }

    pub(crate) fn infer_type(expr: &DaExpr) -> Option<DaType> {
        match expr {
            DaExpr::ConstInt(_) => Some(DaType::int()),
            DaExpr::ConstUInt(_) => Some(DaType::uint()),
            DaExpr::Cast { to, .. } => Some(to.clone()),
            DaExpr::Unsafe(inner) => Self::infer_type(inner),
            DaExpr::Op2 { op, left, .. }
                if matches!(*op, "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||") =>
            {
                Some(DaType::bool())
            }
            DaExpr::Op2 { left, .. } => Self::infer_type(left),
            DaExpr::Op1 { expr: inner, .. } => Self::infer_type(inner),
            _ => None,
        }
    }

    /// Subtraction with pointer arithmetic support.
    /// `is_ptr_op` is true if either operand is a pointer type.
    fn convert_subtraction(
        &self,
        lhs: WithStmts<DaExpr>,
        rhs: WithStmts<DaExpr>,
        expr_type_id: CQualTypeId,
        lhs_is_ptr: bool,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let is_ptr_ret = lhs_is_ptr || self.is_pointer_type(expr_type_id.ctype);
        if matches!(lhs.val, DaExpr::Unsafe(_)) {
            return Ok(lhs.zip(rhs).map(|(l, r)| match l {
                DaExpr::Unsafe(inner) => match *inner {
                    DaExpr::Op2 {
                        op: "+",
                        left,
                        right,
                    } => DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                        op: "+",
                        left,
                        right: Box::new(DaExpr::Op2 {
                            op: "-",
                            left: right,
                            right: Box::new(r),
                        }),
                    })),
                    inner => DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                        op: "-",
                        left: Box::new(inner),
                        right: Box::new(r),
                    })),
                },
                l => DaExpr::Op2 {
                    op: "-",
                    left: Box::new(l),
                    right: Box::new(r),
                },
            }));
        }
        if is_ptr_ret {
            Ok(lhs.zip(rhs).map(|(l, r)| {
                if let DaExpr::Unsafe(inner) = l {
                    if let DaExpr::Op2 {
                        op: "+",
                        left,
                        right,
                    } = *inner
                    {
                        return DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                            op: "+",
                            left,
                            right: Box::new(DaExpr::Op2 {
                                op: "-",
                                left: right,
                                right: Box::new(r),
                            }),
                        }));
                    }
                    return DaExpr::Unsafe(inner);
                }
                DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                    op: "-",
                    left: Box::new(l),
                    right: Box::new(r),
                }))
            }))
        } else {
            let lhs_ty = Self::infer_type(&lhs.val);
            let rhs_ty = Self::infer_type(&rhs.val);
            if let (Some(lt), Some(rt)) = (&lhs_ty, &rhs_ty) {
                if lt != rt {
                    return Ok(lhs.zip(rhs).map(|(l, r)| {
                        let r_casted = DaExpr::Cast {
                            kind: das_ast::CastKind::Cast,
                            expr: Box::new(r),
                            to: lt.clone(),
                        };
                        mk().binary_op("-", l, r_casted)
                    }));
                }
            }
            Ok(lhs.zip(rhs).map(|(l, r)| mk().binary_op("-", l, r)))
        }
    }

    pub fn convert_unary_operator(
        &self,
        ctx: ExprContext,
        name: CUnOp,
        cqual_type: CQualTypeId,
        arg: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        use CUnOp::*;
        match name {
            AddressOf => {
                // `&x` has pointer type, but `x` does not: passing the result
                // type down would make the operand cast itself to `T?`.
                let inner = self.convert_expr(ctx, arg, None)?;
                // Simplify addr(*ptr) → ptr: cancel the Deref+Addr pair.
                // This avoids broken unsafe() scoping where daslang can't see
                // that a pointer index inside Deref(Addr(...)) is actually
                // inside unsafe(). Without this, 31023 fires on nested patterns
                // like unsafe(addr(*blockDc[0])).
                let val = match inner.val {
                    DaExpr::Deref(ptr) => *ptr,
                    _ => DaExpr::Addr(Box::new(inner.val)),
                };
                Ok(WithStmts::new_val(DaExpr::Unsafe(Box::new(val))).prepend_stmts(inner.stmts))
            }
            Deref => {
                // `*p` has the pointee type; the operand keeps the pointer type.
                let inner = self.convert_expr(ctx, arg, None)?;
                let is_unsafe = inner.is_unsafe;
                Ok(WithStmts::new_val(DaExpr::Deref(Box::new(inner.val)))
                    .prepend_stmts(inner.stmts)
                    .merge_unsafe(is_unsafe))
            }
            Negate => self.convert_negate_operator(ctx, cqual_type, arg),
            Plus => self.convert_expr(ctx.used(), arg, Some(cqual_type)),
            Not => {
                // daScript `!` works only on bool. For non-bool, generate `expr == 0` / `expr == null`.
                let arg_ty_opt = self.ast_context[arg].kind.get_qual_type();
                let val = self.convert_expr(ctx.used(), arg, arg_ty_opt)?;
                if let Some(qty) = arg_ty_opt {
                    if self.is_pointer_type(qty.ctype) {
                        let null = self.null_for_type(qty)?;
                        return Ok(val.map(|v| DaExpr::Op2 {
                            op: "==",
                            left: Box::new(v),
                            right: Box::new(null),
                        }));
                    }
                    let resolved_kind = self.ast_context.resolve_type(qty.ctype).kind.clone();
                    if resolved_kind.is_integral_type() {
                        return Ok(val.map(|v| DaExpr::Op2 {
                            op: "==",
                            left: Box::new(v),
                            right: Box::new(DaExpr::ConstInt(0)),
                        }));
                    }
                    if resolved_kind.is_floating_type() {
                        // `!d` is `d == 0` in the operand's own floating type.
                        // Comparing against an integer zero would make every
                        // value with magnitude below one test as false.
                        let zero = super::literals::floating_zero_for_datype(
                            &self.convert_type(qty)?,
                        );
                        return Ok(val.map(|v| DaExpr::Op2 {
                            op: "==",
                            left: Box::new(v),
                            right: Box::new(zero),
                        }));
                    }
                    if matches!(resolved_kind, CTypeKind::Enum(_)) {
                        return Ok(val.map(|v| DaExpr::Op2 {
                            op: "==",
                            left: Box::new(DaExpr::Cast {
                                kind: das_ast::CastKind::Cast,
                                expr: Box::new(v),
                                to: DaType::uint(),
                            }),
                            right: Box::new(DaExpr::Cast {
                                kind: das_ast::CastKind::Cast,
                                expr: Box::new(DaExpr::ConstInt(0)),
                                to: DaType::uint(),
                            }),
                        }));
                    }
                }
                // bool or unknown: apply `!` directly
                Ok(val.map(|v| mk().unary_op("!", v)))
            }
            Complement => {
                let inner = self.convert_expr(ctx, arg, Some(cqual_type))?;
                Ok(inner.map(|v| mk().unary_op("~", v)))
            }
            Extension => self.convert_expr(ctx, arg, Some(cqual_type)),
            PreIncrement => self.convert_pre_increment(ctx, cqual_type, CBinOp::AssignAdd, arg),
            PreDecrement => {
                self.convert_pre_increment(ctx, cqual_type, CBinOp::AssignSubtract, arg)
            }
            PostIncrement => self.convert_post_increment(ctx, cqual_type, CBinOp::AssignAdd, arg),
            PostDecrement => {
                self.convert_post_increment(ctx, cqual_type, CBinOp::AssignSubtract, arg)
            }
            Real | Imag | Coawait => Err(TranslationError::generic("unsupported unary operator")),
        }
    }

    /// Negation with literal optimization.
    fn convert_negate_operator(
        &self,
        ctx: ExprContext,
        expr_type_id: CQualTypeId,
        arg_id: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let val = self.convert_expr(ctx.used(), arg_id, Some(expr_type_id))?;
        Ok(val.map(|v| mk().unary_op("-", v)))
    }

    pub fn convert_pre_increment(
        &self,
        ctx: ExprContext,
        ty: CQualTypeId,
        op: CBinOp,
        arg: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        self.convert_increment(ctx, ty, op, arg, false)
    }

    pub fn convert_post_increment(
        &self,
        ctx: ExprContext,
        ty: CQualTypeId,
        op: CBinOp,
        arg: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        self.convert_increment(ctx, ty, op, arg, true)
    }

    /// `++x` / `x++` are `x += 1` with C's conversions: the read is promoted,
    /// the arithmetic runs in the promoted type, and the result is narrowed
    /// back into the storage type.  The operand is evaluated exactly once.
    fn convert_increment(
        &self,
        ctx: ExprContext,
        ty: CQualTypeId,
        op: CBinOp,
        arg: CExprId,
        is_post: bool,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let das_op = match op {
            CBinOp::AssignAdd => "+",
            CBinOp::AssignSubtract => "-",
            _ => return Err(TranslationError::generic("invalid increment op")),
        };
        let arg_ty = self.ast_context[arg].kind.get_qual_type().unwrap_or(ty);
        let place = self.convert_lvalue_once(ctx, arg, arg_ty)?;
        let storage = writable_type(self.convert_type(arg_ty)?);
        let kind = self.ast_context.resolve_type(arg_ty.ctype).kind.clone();
        let new_value = if self.is_pointer_type(arg_ty.ctype) {
            DaExpr::Unsafe(Box::new(DaExpr::Op2 {
                op: das_op,
                left: Box::new(place.val.clone()),
                right: Box::new(DaExpr::ConstInt(1)),
            }))
        } else {
            let arith = self
                .arith_type_of_kind(&kind)
                .ok_or_else(|| TranslationError::generic("increment of non-arithmetic C type"))?;
            let one = if matches!(arith, abi::CArith::Int) {
                DaExpr::ConstInt(1)
            } else {
                self.integer_literal_for_type(DaExpr::ConstInt(1), arith.da_type())
            };
            let promoted = self.promote_operand(place.val.clone(), &kind, arith);
            self.narrow_to_storage(mk().binary_op(das_op, promoted, one), &storage)
        };
        let is_unsafe = place.is_unsafe;
        let mut stmts = place.stmts;
        let result = if is_post {
            let old_name = self.renamer.borrow_mut().pick_name("c2da_postinc");
            stmts.push(DaStmt::Var {
                name: old_name.clone(),
                var_type: storage,
                init: Some(place.val.clone()),
            });
            DaExpr::Var(old_name)
        } else {
            place.val.clone()
        };
        stmts.push(DaStmt::Expr(DaExpr::Assign(
            Box::new(place.val),
            Box::new(new_value),
        )));
        Ok(WithStmts::new(stmts, result).merge_unsafe(is_unsafe))
    }
}

fn normalize_numeric_binop_tree(expr: DaExpr) -> DaExpr {
    match expr {
        DaExpr::Op2 { op, left, right }
            if matches!(op, "+" | "-" | "*" | "/" | "%" | "<<" | ">>") =>
        {
            let left = normalize_numeric_binop_tree(*left);
            let right = normalize_numeric_binop_tree(*right);
            let right = match (
                Translation::infer_type(&left),
                Translation::infer_type(&right),
            ) {
                (Some(lt), Some(rt)) if lt.is_numeric() && rt.is_numeric() && lt != rt => {
                    DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(right),
                        to: lt,
                    }
                }
                _ => right,
            };
            DaExpr::Op2 {
                op,
                left: Box::new(left),
                right: Box::new(right),
            }
        }
        DaExpr::Unsafe(inner) => DaExpr::Unsafe(Box::new(normalize_numeric_binop_tree(*inner))),
        DaExpr::Cast { kind, expr, to } => DaExpr::Cast {
            kind,
            expr: Box::new(normalize_numeric_binop_tree(*expr)),
            to,
        },
        other => other,
    }
}

fn lower_assignment_expr(assign: DaExpr, result: DaExpr, is_used: bool) -> WithStmts<DaExpr> {
    if is_used {
        WithStmts::new(vec![DaStmt::Expr(assign)], result)
    } else {
        WithStmts::new_val(assign)
    }
}

fn materialize_expr_type(expr: WithStmts<DaExpr>, target: Option<&DaType>) -> WithStmts<DaExpr> {
    let Some(target) = target else {
        return expr;
    };
    if !target.is_numeric() || matches!(target.kind, DaTypeKind::Auto) {
        return expr;
    }
    if Translation::infer_type(&expr.val).is_some() {
        return expr;
    }
    let should_cast = matches!(
        expr.val,
        DaExpr::Var(_)
            | DaExpr::Field(_, _)
            | DaExpr::SafeField(_, _)
            | DaExpr::Index(_, _)
            | DaExpr::SafeIndex(_, _)
            | DaExpr::Deref(_)
            | DaExpr::DerefExplicit(_)
    );
    if should_cast {
        expr.map(|v| DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(v),
            to: target.clone(),
        })
    } else {
        expr
    }
}

/// Returns the daScript type to coerce both shift operands to, if needed.
/// daScript `<<`/`>>` only works for int32/uint32/int64/uint64 with matching types.
fn coerce_shift_types(lhs: &Option<CTypeKind>, rhs: &Option<CTypeKind>) -> Option<DaType> {
    use CTypeKind::*;
    let l = lhs.as_ref()?;
    // Determine the "width" of the LHS type — what daScript type should we use?
    // Smaller types (int8/uint8/int16/uint16) need widening to int or uint.
    // If LHS and RHS are different widths, coerce both to the wider type.
    if matches!(l, Int8 | SChar | Char | UInt8 | UChar | Int16 | UInt16) {
        // Small types: widen to 32-bit, preserving signedness
        Some(if l.is_unsigned_integral_type() {
            DaType::uint()
        } else {
            DaType::int()
        })
    } else if matches!(
        l,
        Int | Short | Int32 | UInt | UInt32 | Int64 | Long | LongLong | UInt64 | ULong | ULongLong
    ) {
        // Already a supported shift type. If RHS differs, coerce RHS to LHS.
        // Check if RHS matches the same daScript type.
        if let Some(r) = rhs {
            let lhs_da = type_kind_to_datype(l);
            let rhs_da = type_kind_to_datype(r);
            if lhs_da != rhs_da {
                Some(lhs_da)
            } else {
                None // already matching
            }
        } else {
            None
        }
    } else {
        None
    }
}

impl<'c> Translation<'c> {
    fn coerce_assignment_value(
        &self,
        expr: DaExpr,
        target_kind: &CTypeKind,
        target_da_type: &DaType,
    ) -> DaExpr {
        if matches!(target_da_type.kind, DaTypeKind::Pointer(_))
            && !matches!(expr, DaExpr::ConstNull)
        {
            return self.abi_pointer_cast(expr, target_da_type.clone());
        }
        if target_da_type.is_numeric() && !matches!(target_da_type.kind, DaTypeKind::Auto) {
            let mut target = target_da_type.clone();
            target.is_const = false;
            target.is_ref = false;
            target.is_temporary = false;
            if Translation::infer_type(&expr)
                .map(|mut inferred| {
                    inferred.is_const = false;
                    inferred.is_ref = false;
                    inferred.is_temporary = false;
                    inferred != target
                })
                .unwrap_or(false)
            {
                return DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(expr),
                    to: target,
                };
            }
        }
        // Only cast when target type differs from default int32 (the type of integer literals).
        // This avoids redundant `cast<int>(10)` while still catching `uint = 10` → `uint(10)`.
        let needs_cast = target_kind.is_integral_type()
            && !matches!(
                target_kind,
                CTypeKind::Bool
                    | CTypeKind::Int
                    | CTypeKind::SChar
                    | CTypeKind::Char
                    | CTypeKind::Short
                    | CTypeKind::Int32
                    | CTypeKind::Int8
                    | CTypeKind::Int16
            );
        if needs_cast {
            DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(expr),
                to: target_da_type.clone(),
            }
        } else {
            expr
        }
    }
}
