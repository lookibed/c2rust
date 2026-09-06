//! Address-backed C object access.
//!
//! C layout is owned by `layout.rs`; this module only turns a known raw C
//! address plus that layout into daScript lvalues.

use super::*;

#[derive(Clone)]
pub(crate) struct CObjectAddress {
    pub raw: WithStmts<DaExpr>,
    /// `raw` is already a uint64 C address (local union storage), rather
    /// than a typed daScript pointer (ordinary pointer-backed object).
    pub raw_is_address: bool,
    pub ctype: CQualTypeId,
    /// Byte displacement from `raw`, always a fact supplied by `layout.rs`.
    pub byte_offset: u64,
    /// Field storage width exported by Clang. This preserves the layout
    /// contract through typedef wrappers which do not own a CTypeId layout.
    pub storage_size_bytes: Option<u64>,
}

impl<'c> Translation<'c> {
    /// Recover an address-backed aggregate place from a member expression.
    ///
    /// A nested C access such as `outer->inner.count` must never materialize
    /// `outer->inner` as an aggregate daScript rvalue.  It is only an address
    /// carrier on the way to the scalar leaf.  Returning `None` means the
    /// expression is an ordinary local daScript value and should retain the
    /// existing high-level member lowering.
    pub(crate) fn member_place_address(
        &self,
        ctx: ExprContext,
        member_expr: CExprId,
    ) -> TranslationResult<Option<CObjectAddress>> {
        let CExprKind::Member(_, base_expr, field, member_kind, _) =
            self.ast_context[member_expr].kind.clone()
        else {
            return Ok(None);
        };

        let base_address = self.member_place_address(ctx, base_expr)?;
        match (member_kind, base_address) {
            (_, Some(base_address)) => self.field_address(base_address, field).map(Some),
            (MemberKind::Arrow, None) => {
                let base_ctype = self.ast_context[base_expr]
                    .kind
                    .get_qual_type()
                    .ok_or_else(|| TranslationError::generic("member pointer has no C type"))?;
                let base = self.convert_expr(ctx, base_expr, Some(base_ctype))?;
                self.pointer_member_address(base, base_ctype, field)
                    .map(Some)
            }
            (MemberKind::Dot, None) => Ok(None),
        }
    }

    fn raw_byte_address(&self, address: &CObjectAddress) -> WithStmts<DaExpr> {
        address.raw.clone().map(|raw| {
            let raw = if address.raw_is_address {
                raw
            } else {
                self.pointer_to_raw_address(raw)
            };
            if address.byte_offset == 0 {
                raw
            } else {
                DaExpr::Op2 {
                    op: "+",
                    left: Box::new(raw),
                    right: Box::new(self.integer_literal_for_type(
                        DaExpr::ConstInt(address.byte_offset as i64),
                        DaType::uint64(),
                    )),
                }
            }
        })
    }

    /// Expose the canonical raw address of an aggregate place for array decay
    /// and aggregate-copy owners.  Callers must still choose the destination
    /// pointer type through the ABI layer; this method never invents one.
    pub(crate) fn raw_address_of_place(&self, address: &CObjectAddress) -> WithStmts<DaExpr> {
        self.raw_byte_address(address)
    }

    fn raw_storage_size(&self, address: &CObjectAddress) -> TranslationResult<u64> {
        match address.storage_size_bytes {
            Some(size) => Ok(size),
            None => Ok(self.layout_of(address.ctype.ctype)?.size_bytes),
        }
    }

    fn address_is_typed_aligned(&self, address: &CObjectAddress) -> TranslationResult<bool> {
        let size = self.raw_storage_size(address)?;
        Ok(size != 0 && address.byte_offset % size == 0)
    }
    pub(crate) fn field_address(
        &self,
        base: CObjectAddress,
        field: CFieldId,
    ) -> TranslationResult<CObjectAddress> {
        let offset = match self.ast_context[field].kind {
            CDeclKind::Field {
                bitfield_width: Some(_),
                platform_bit_offset,
                ..
            } => i64::try_from(platform_bit_offset / 8).map_err(|_| {
                TranslationError::generic("bitfield byte offset exceeds daScript range")
            })?,
            _ => self.field_offset(field)?,
        };
        let (field_ty, platform_type_bitwidth, bitfield_width) = match self.ast_context[field].kind
        {
            CDeclKind::Field {
                typ,
                platform_type_bitwidth,
                bitfield_width,
                ..
            } => (typ, platform_type_bitwidth, bitfield_width),
            _ => {
                return Err(TranslationError::generic(
                    "field address requested for non-field",
                ))
            }
        };
        let _ = bitfield_width;
        let offset = u64::try_from(offset)
            .map_err(|_| TranslationError::generic("negative C field offset from Clang"))?;
        let byte_offset = base
            .byte_offset
            .checked_add(offset)
            .ok_or_else(|| TranslationError::generic("C field address offset overflow"))?;
        Ok(CObjectAddress {
            raw: base.raw,
            raw_is_address: base.raw_is_address,
            ctype: field_ty,
            byte_offset,
            storage_size_bytes: (platform_type_bitwidth % 8 == 0)
                .then_some(platform_type_bitwidth / 8),
        })
    }

    /// Return an assignable daScript lvalue for an aligned scalar/pointer C
    /// field. Packed, bitfield and aggregate access deliberately fail until
    /// their respective object-memory lowerings exist.
    pub(crate) fn raw_load(&self, address: CObjectAddress) -> TranslationResult<WithStmts<DaExpr>> {
        let ty = self.ast_context.resolve_type(address.ctype.ctype);
        if address.ctype.qualifiers.is_volatile {
            return Err(TranslationError::generic(
                "volatile raw C object access is not implemented",
            ));
        }
        if matches!(
            ty.kind,
            CTypeKind::ConstantArray(..) | CTypeKind::Struct(_) | CTypeKind::Union(_)
        ) {
            return Err(TranslationError::generic(
                "aggregate C object rvalue from raw storage is not implemented",
            ));
        }
        let target = self.convert_type(address.ctype)?;
        let pointer = DaType::pointer(target.clone());
        let storage_size = self.raw_storage_size(&address)?;
        if storage_size == 0 {
            return Err(TranslationError::generic(
                "zero-sized raw C field is invalid",
            ));
        }
        if !self.address_is_typed_aligned(&address)? {
            let tmp = self.renamer.borrow_mut().fresh();
            let byte_address = self.raw_byte_address(&address);
            let tmp_address = self.pointer_to_raw_address(DaExpr::Unsafe(Box::new(DaExpr::Addr(
                Box::new(DaExpr::Var(tmp.clone())),
            ))));
            let mut stmts = byte_address.stmts;
            stmts.push(DaStmt::Var {
                name: tmp.clone(),
                var_type: target,
                init: None,
            });
            stmts.push(DaStmt::Expr(DaExpr::Call(
                Box::new(DaExpr::Var("c2da_rt_memcpy".into())),
                vec![
                    tmp_address,
                    byte_address.val,
                    self.integer_literal_for_type(
                        DaExpr::ConstInt(storage_size as i64),
                        DaType::uint64(),
                    ),
                ],
            )));
            return Ok(WithStmts::new(stmts, DaExpr::Var(tmp)).merge_unsafe(address.raw.is_unsafe));
        }
        let element_index = i64::try_from(address.byte_offset / storage_size).map_err(|_| {
            TranslationError::generic("C field index exceeds daScript integer range")
        })?;
        // daScript's raw-memory runtime writes through pointer indexing; it
        // preserves an assignable location whereas a cast/deref expression is
        // rejected by its dead-write policy.
        Ok(address.raw.map(|raw| {
            DaExpr::Unsafe(Box::new(DaExpr::Index(
                Box::new(self.raw_address_to_pointer(
                    if address.raw_is_address {
                        raw
                    } else {
                        self.pointer_to_raw_address(raw)
                    },
                    pointer,
                )),
                Box::new(DaExpr::ConstInt(element_index)),
            )))
        }))
    }

    /// Store a scalar/pointer C value through an address-backed object. For a
    /// packed field this deliberately materializes a typed temporary and uses
    /// the canonical runtime memcpy boundary instead of an unaligned typed
    /// dereference.
    pub(crate) fn raw_store(
        &self,
        address: CObjectAddress,
        value: WithStmts<DaExpr>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let target = self.convert_type(address.ctype)?;
        let storage_size = self.raw_storage_size(&address)?;
        if self.address_is_typed_aligned(&address)? {
            let lvalue = self.raw_load(address)?;
            let mut stmts = lvalue.stmts;
            stmts.extend(value.stmts);
            let result = value.val.clone();
            stmts.push(DaStmt::Expr(DaExpr::Assign(
                Box::new(lvalue.val),
                Box::new(value.val),
            )));
            return Ok(
                WithStmts::new(stmts, result).merge_unsafe(lvalue.is_unsafe || value.is_unsafe)
            );
        }
        let tmp = self.renamer.borrow_mut().fresh();
        let byte_address = self.raw_byte_address(&address);
        let tmp_address = self.pointer_to_raw_address(DaExpr::Unsafe(Box::new(DaExpr::Addr(
            Box::new(DaExpr::Var(tmp.clone())),
        ))));
        let mut stmts = value.stmts;
        stmts.push(DaStmt::Var {
            name: tmp.clone(),
            var_type: target,
            init: Some(value.val),
        });
        stmts.extend(byte_address.stmts);
        stmts.push(DaStmt::Expr(DaExpr::Call(
            Box::new(DaExpr::Var("c2da_rt_memcpy".into())),
            vec![
                byte_address.val,
                tmp_address,
                self.integer_literal_for_type(
                    DaExpr::ConstInt(storage_size as i64),
                    DaType::uint64(),
                ),
            ],
        )));
        Ok(WithStmts::new(stmts, DaExpr::Var(tmp))
            .merge_unsafe(address.raw.is_unsafe || value.is_unsafe))
    }

    pub(crate) fn bitfield_load(
        &self,
        address: CObjectAddress,
        field: CFieldId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let (width, bit_offset) = match self.ast_context[field].kind {
            CDeclKind::Field {
                bitfield_width: Some(width),
                platform_bit_offset,
                ..
            } => (width, platform_bit_offset % 8),
            _ => {
                return Err(TranslationError::generic(
                    "bitfield load requested for non-bitfield",
                ))
            }
        };
        if width == 0 || width > 63 {
            return Err(TranslationError::generic("unsupported C bitfield width"));
        }
        let storage = self.raw_load(address)?;
        let target = self.convert_type(match self.ast_context[field].kind {
            CDeclKind::Field { typ, .. } => typ,
            _ => unreachable!(),
        })?;
        let mask = (1u64 << width) - 1;
        Ok(storage.map(|storage| DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(DaExpr::Op2 {
                op: "&",
                left: Box::new(DaExpr::Op2 {
                    op: ">>",
                    left: Box::new(storage),
                    right: Box::new(DaExpr::ConstInt(bit_offset as i64)),
                }),
                right: Box::new(DaExpr::ConstUInt(mask)),
            }),
            to: target,
        }))
    }

    pub(crate) fn bitfield_store(
        &self,
        address: CObjectAddress,
        field: CFieldId,
        value: WithStmts<DaExpr>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let (width, bit_offset) = match self.ast_context[field].kind {
            CDeclKind::Field {
                bitfield_width: Some(width),
                platform_bit_offset,
                ..
            } => (width, platform_bit_offset % 8),
            _ => {
                return Err(TranslationError::generic(
                    "bitfield store requested for non-bitfield",
                ))
            }
        };
        if width == 0 || width > 63 {
            return Err(TranslationError::generic("unsupported C bitfield width"));
        }
        let storage = self.raw_load(address.clone())?;
        // The read-modify-write is performed in the field's own storage type.
        // Every constant is built in that type too: daScript has no implicit
        // numeric conversion, so a 64-bit mask against a 32-bit storage word
        // is a type error rather than a wider computation.
        let storage_type = writable_type(self.convert_type(address.ctype)?);
        let storage_bits = self.raw_storage_size(&address)? * 8;
        let storage_mask = if storage_bits >= 64 {
            u64::MAX
        } else {
            (1u64 << storage_bits) - 1
        };
        let field_mask = (1u64 << width) - 1;
        let shifted_mask = field_mask << bit_offset;
        let in_storage_type = |expr: DaExpr| {
            if Self::infer_type(&expr).as_ref() == Some(&storage_type) {
                return expr;
            }
            DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(expr),
                to: storage_type.clone(),
            }
        };
        // Masks are bit patterns, the shift distance is a count; each keeps
        // its natural literal spelling and is always given the storage type
        // explicitly, because a bare literal's type comes from its spelling
        // rather than from the word it is applied to.
        let typed_const = |literal: DaExpr| DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(literal),
            to: storage_type.clone(),
        };
        let storage_mask_const =
            |bits: u64| typed_const(DaExpr::ConstUInt(bits & storage_mask));
        let storage_count_const = |count: u64| typed_const(DaExpr::ConstInt(count as i64));
        let value_expr = value.val.clone();
        let new_storage = storage.zip(value).map(|(old, value)| DaExpr::Op2 {
            op: "|",
            left: Box::new(DaExpr::Op2 {
                op: "&",
                left: Box::new(old),
                right: Box::new(storage_mask_const(!shifted_mask)),
            }),
            right: Box::new(DaExpr::Op2 {
                op: "<<",
                left: Box::new(DaExpr::Op2 {
                    op: "&",
                    left: Box::new(in_storage_type(value)),
                    right: Box::new(storage_mask_const(field_mask)),
                }),
                right: Box::new(storage_count_const(bit_offset as u64)),
            }),
        });
        self.raw_store(address, new_storage)
            .map(|stored| stored.map(|_| value_expr))
    }

    pub(crate) fn pointer_member_address(
        &self,
        base: WithStmts<DaExpr>,
        base_ctype: CQualTypeId,
        field: CFieldId,
    ) -> TranslationResult<CObjectAddress> {
        match self.ast_context.resolve_type(base_ctype.ctype).kind {
            CTypeKind::Pointer(inner) => match self.ast_context.resolve_type(inner.ctype).kind {
                CTypeKind::Struct(_) | CTypeKind::Union(_) => {}
                _ => {
                    return Err(TranslationError::generic(
                        "member pointer does not point to a C record",
                    ))
                }
            },
            _ => {
                return Err(TranslationError::generic(
                    "address-backed member requires C record pointer",
                ))
            }
        };
        self.field_address(
            CObjectAddress {
                raw: base,
                raw_is_address: false,
                ctype: base_ctype,
                byte_offset: 0,
                storage_size_bytes: None,
            },
            field,
        )
    }

    pub(crate) fn pointer_member_lvalue(
        &self,
        base: WithStmts<DaExpr>,
        base_ctype: CQualTypeId,
        field: CFieldId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let address = self.pointer_member_address(base, base_ctype, field)?;
        if matches!(
            self.ast_context[field].kind,
            CDeclKind::Field {
                bitfield_width: Some(_),
                ..
            }
        ) {
            self.bitfield_load(address, field)
        } else {
            self.raw_load(address)
        }
    }

    /// Load a field below an address-backed aggregate place.  The field itself
    /// may be scalar/pointer (supported) or aggregate (a precise diagnostic
    /// until the aggregate-copy layer owns it).
    pub(crate) fn member_place_lvalue(
        &self,
        base: CObjectAddress,
        field: CFieldId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let address = self.field_address(base, field)?;
        if matches!(
            self.ast_context[field].kind,
            CDeclKind::Field {
                bitfield_width: Some(_),
                ..
            }
        ) {
            self.bitfield_load(address, field)
        } else {
            self.raw_load(address)
        }
    }
}
