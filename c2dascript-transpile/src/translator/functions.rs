//! Function translation — порт c2rust functions.rs + CFG pipeline
use super::runtime::{canonical_runtime_function, CanonicalRuntimeFunction, RuntimeArgKind};
use super::*;
use crate::c_ast::iterators::{DFExpr, SomeId};
use crate::format_translation_err;

impl<'c> Translation<'c> {
    pub fn convert_variable(
        &self,
        ctx: ExprContext,
        decl_id: CDeclId,
        name: &str,
        typ: CQualTypeId,
        init: Option<CExprId>,
        is_static: bool,
    ) -> TranslationResult<DaDecl> {
        if self.is_va_decl(decl_id)
            || (self.function_context.borrow().va_list_arg_name.is_some()
                && self.ast_context.is_va_list(typ.ctype))
        {
            return self.convert_va_list_variable(decl_id, name, init);
        }
        let das_type = self.convert_type(typ)?;
        let init = init
            .map(|e| self.convert_expr(ctx, e, Some(typ)))
            .transpose()?
            .map(|ws| {
                let init_val = normalize_array_initializer_for_type(ws.val, &das_type);
                let val = if matches!(das_type.kind, DaTypeKind::Pointer(_))
                    && !matches!(init_val, DaExpr::ConstNull)
                    && Self::infer_type(&init_val).map_or(true, |inferred| inferred != das_type)
                {
                    self.abi_pointer_cast(init_val, das_type.clone())
                } else if das_type.is_numeric()
                    && Self::infer_type(&init_val).map_or(true, |inferred| inferred != das_type)
                {
                    let mut to = das_type.clone();
                    to.is_const = false;
                    to.is_ref = false;
                    to.is_temporary = false;
                    DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(init_val),
                        to,
                    }
                } else {
                    init_val
                };
                if is_static && ws.is_unsafe {
                    DaExpr::Unsafe(Box::new(val))
                } else {
                    val
                }
            });
        // A daScript fixed array is zero-initialized by its declaration, at
        // any extent, so an uninitialized C array global needs no initializer
        // expression at all.
        let name = self.declare_value_name(decl_id, name);
        Ok(DaDecl::Variable(DaVariable {
            name,
            var_type: das_type,
            init,
            annotations: vec![],
        }))
    }

    pub fn convert_function(
        &self,
        ctx: ExprContext,
        decl_id: CDeclId,
        name: &str,
        typ: CTypeId,
        parameters: &[CDeclId],
        body: Option<CStmtId>,
        _attrs: &indexmap::IndexSet<crate::c_ast::Attribute>,
    ) -> TranslationResult<DaDecl> {
        self.function_context.borrow_mut().enter_new(name);

        let (ret_ctype, is_variadic): (Option<CQualTypeId>, bool) =
            match self.ast_context.resolve_type(typ).kind {
                CTypeKind::Function(ret, _, is_var, is_noreturn, _) => {
                    (if is_noreturn { None } else { Some(ret) }, is_var)
                }
                _ => return Err(TranslationError::generic("not a function type")),
            };
        self.function_context
            .borrow_mut()
            .set_return_type(ret_ctype);
        let variadic_arg_name = is_variadic
            .then(|| body.map(|body_id| self.register_va_decls(body_id)))
            .flatten();

        // Convert return type for function signature
        let ret_type = ret_ctype
            .map(|q| self.convert_type(q))
            .transpose()?
            .unwrap_or(DaType::void());

        let mut params = vec![];
        let mut param_bindings = vec![];
        let mut unnamed_idx = 0u32;
        for param_id in parameters {
            if let CDeclKind::Variable { ref ident, typ, .. } = self.ast_context[*param_id].kind {
                let das_ty = self.convert_type(typ.clone())?;
                let is_const = typ.qualifiers.is_const;
                let is_ptr = self.is_pointer_type(typ.ctype);
                let pname = if ident.is_empty() || ident == "__" {
                    unnamed_idx += 1;
                    self.declare_value_name(*param_id, &format!("c2da_arg{}", unnamed_idx))
                } else {
                    self.declare_value_name(*param_id, ident)
                };
                self.function_context
                    .borrow_mut()
                    .add_param_alias(ident, &pname);
                param_bindings.push((*param_id, ident.clone(), typ, pname.clone()));
                if is_ptr || !is_const {
                    params.push(mk().param_mut(pname, das_ty, None));
                } else {
                    params.push(mk().param(pname, das_ty, None));
                }
            }
        }
        if let Some(arg_name) = variadic_arg_name {
            params.push(mk().param_mut(arg_name, DaType::array(self.va_arg_type()), None));
        }
        if let Some(body_id) = body {
            self.add_definition_param_aliases(body_id, &param_bindings);
        }

        let body_das = if let Some(body_id) = body {
            // Determine implicit return type
            let is_main = name == "main";
            let is_void = ret_ctype
                .map(|qty| self.ast_context[qty.ctype].kind == CTypeKind::Void)
                .unwrap_or(true);
            let imp_ret = if is_void {
                crate::cfg::ImplicitReturnType::Void
            } else if is_main {
                crate::cfg::ImplicitReturnType::Main
            } else {
                crate::cfg::ImplicitReturnType::NoImplicitReturnType
            };

            // Extract compound statement children
            let stmt_ids = match self.ast_context[body_id].kind {
                CStmtKind::Compound(ref stmts) => stmts.clone(),
                _ => vec![body_id],
            };

            // Run through CFG pipeline
            let body_stmts =
                crate::cfg::convert_function_body(self, body_id, &stmt_ids, imp_ret, ret_ctype)?;

            Some(DaExpr::Block(DaBlock { stmts: body_stmts }))
        } else {
            None
        };

        let fn_name = self.declare_value_name(decl_id, name);
        let mut func = mk().fn_decl(fn_name.as_str(), params, ret_type, body_das);
        if let DaDecl::Function(ref mut f) = func {
            // Only the translation unit's own functions are part of its API.
            // A header's `static inline` helpers (`__bswap_32` and friends, which
            // arrive with <stdio.h>) are implementation detail that happens to be
            // visible here; exporting them published names the C program never
            // defined.
            if body.is_some() && self.is_defined_in_main_file(decl_id) {
                f.annotations.push("export".into());
            }
        }
        Ok(func)
    }

    pub fn convert_function_call(
        &self,
        ctx: ExprContext,
        func: CExprId,
        args: &[CExprId],
        call_expr_ty: CQualTypeId,
        override_ty: Option<CQualTypeId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        if let Some(part) = self.match_vapart(func, args) {
            return self.convert_vapart(part);
        }
        if self.is_variadic_function_pointer_callee(func) {
            return Err(format_translation_err!(
                self.ast_context.display_loc(&self.ast_context[func].loc),
                "unsupported variadic ABI boundary: variadic function pointer call",
            ));
        }
        if let CExprKind::ImplicitCast(_, fexp, CastKind::BuiltinFnToFnPtr, _, _) =
            &self.ast_context[func].kind
        {
            // libc allocation is represented by a compiler-owned runtime even
            // when Clang classifies the source declaration as a builtin.
            if self
                .direct_call_name(*fexp)
                .as_deref()
                .and_then(canonical_runtime_function)
                .is_none()
            {
                return self.convert_builtin_call(ctx, *fexp, args);
            }
        }
        self.reject_unknown_external_call(func)?;
        // A call through a function pointer is `invoke(f, args…)` in daScript,
        // and the callee expression is the function value itself: C's
        // decay-to-pointer and the `(*f)(…)` dereference are both identities on
        // it, so they are peeled off rather than lowered as pointer operations.
        let is_direct = self.is_direct_function_declaration(func);
        let indirect_callee = (!is_direct)
            .then(|| self.function_value_operand(func))
            .flatten();
        // A direct call names the function; the decay to a pointer that Clang
        // records around it must not become a `@@name` function *value*.
        let callee_expr_id = match (indirect_callee, is_direct) {
            (Some(value), _) => value,
            (None, true) => strip_implicit_casts(&self.ast_context, func),
            (None, false) => func,
        };
        let func_expr = self.convert_expr(ctx.used(), callee_expr_id, None)?;
        let mut is_unsafe = func_expr.is_unsafe;
        // Runtime policy is selected from the C declaration, not from the
        // already-lowered expression: an implicit function-to-pointer cast can
        // erase the direct `DaExpr::Var` shape.
        let func_name = self
            .direct_call_name(func)
            .or_else(|| match &func_expr.val {
                DaExpr::Var(n) => Some(n.clone()),
                _ => None,
            });
        let runtime = func_name.as_deref().and_then(canonical_runtime_function);
        let mut all_stmts = func_expr.stmts;
        let mut das_args = vec![];
        let mut variadic_tail = vec![];
        let arg_tys = self.call_arg_types(func);
        let is_variadic = self.is_variadic_callee(func);
        for (idx, &arg) in args.iter().enumerate() {
            let expected = arg_tys.get(idx).copied().filter(|_| {
                self.libc_memory_arg_cast(func_name.as_deref(), idx)
                    .is_none()
                    && canonical_runtime_arg_type(runtime, idx).is_none()
            });
            let a = self.convert_expr(ctx, arg, expected)?;
            let a = if let Some(expected_ty) = expected {
                self.lower_to_c_value(
                    a,
                    self.ast_context[arg].kind.get_qual_type(),
                    self.convert_type(expected_ty)?,
                    ValueSite::CallArg,
                )?
            } else {
                a
            };
            is_unsafe |= a.is_unsafe;
            all_stmts.extend(a.stmts);
            let mut arg_val = a.val;
            if let Some(to) = self.libc_memory_arg_cast(func_name.as_deref(), idx) {
                arg_val = DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(arg_val),
                    to,
                };
            } else if let Some(expected_ty) = expected {
                let expected_da = self.convert_type(expected_ty)?;
                if matches!(expected_da.kind, DaTypeKind::Pointer(_))
                    && !matches!(arg_val, DaExpr::ConstNull)
                    && Self::infer_type(&arg_val).map_or(true, |actual| actual != expected_da)
                {
                    arg_val = self.abi_pointer_cast(arg_val, expected_da);
                }
            }
            // Canonical runtime ABI is raw-address/uint64 based.  This cast is
            // constructed before the daScript AST reaches the printer.
            if let Some(runtime_arg) = canonical_runtime_arg_type(runtime, idx) {
                arg_val = self.lower_runtime_arg(arg_val, runtime_arg);
            }
            if is_variadic && idx >= arg_tys.len() {
                variadic_tail.push((arg, arg_val));
            } else if let Some((stmts, lowered_arg)) = self.bool_to_integer_cast(arg_val.clone()) {
                all_stmts.extend(stmts);
                das_args.push(lowered_arg);
            } else {
                das_args.push(arg_val);
            }
        }
        if is_variadic {
            das_args.push(DaExpr::MakeArray(
                self.pack_variadic_call_tail(0, variadic_tail)?,
            ));
        }
        let call = if let Some(function) = runtime {
            mk().call_expr(DaExpr::Var(function.target_name().to_owned()), das_args)
        } else if indirect_callee.is_some() {
            // `invoke` is daScript's call-through-a-function-value operator.
            let mut invoke_args = vec![func_expr.val];
            invoke_args.extend(das_args);
            mk().call_expr(DaExpr::Var("invoke".to_owned()), invoke_args)
        } else {
            mk().call_expr(func_expr.val, das_args)
        };
        // The raw-memory runtime returns an address, not C's declared return
        // type. Materialize it once at the outermost pointer type demanded by
        // this expression: `(int *)malloc(...)` crosses as `uint64 -> int?`.
        let runtime_pointer_result_ty = runtime.and_then(|_| {
            override_ty
                .filter(|ty| self.is_pointer_type(ty.ctype))
                .or_else(|| {
                    self.is_pointer_type(call_expr_ty.ctype)
                        .then_some(call_expr_ty)
                })
        });
        let call = if let Some(pointer_ty) = runtime_pointer_result_ty {
            self.raw_address_to_pointer(call, self.convert_type(pointer_ty)?)
        } else {
            call
        };
        let result = if let Some(expected_ty) = override_ty {
            let ret_ty = self.convert_type(expected_ty)?;
            if runtime_pointer_result_ty == Some(expected_ty) {
                call
            } else if self.is_callable_type(expected_ty.ctype)
                || crate::convert_type::is_function_value_type(&ret_ty)
            {
                // daScript function values have no conversion syntax and need
                // none: a C function pointer only ever crosses to itself.
                call
            } else if matches!(ret_ty.kind, DaTypeKind::Pointer(_)) {
                self.abi_pointer_cast(call, ret_ty)
            } else {
                DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(call),
                    to: ret_ty,
                }
            }
        } else {
            call
        };
        Ok(WithStmts::new_val(result)
            .prepend_stmts(all_stmts)
            .merge_unsafe(is_unsafe))
    }

    pub(crate) fn call_arg_types(&self, func: CExprId) -> Vec<CQualTypeId> {
        let func = match &self.ast_context[func].kind {
            CExprKind::ImplicitCast(_, inner, _, _, _) => *inner,
            _ => func,
        };
        let CExprKind::DeclRef(_, decl_id, _) = &self.ast_context[func].kind else {
            return vec![];
        };
        let CDeclKind::Function { parameters, .. } = &self.ast_context[*decl_id].kind else {
            return vec![];
        };
        parameters
            .iter()
            .filter_map(|param| match &self.ast_context[*param].kind {
                CDeclKind::Variable { typ, .. } => Some(*typ),
                _ => None,
            })
            .collect()
    }

    fn is_variadic_callee(&self, func: CExprId) -> bool {
        let mut func = func;
        while let CExprKind::ImplicitCast(_, inner, _, _, _) = &self.ast_context[func].kind {
            func = *inner;
        }
        let CExprKind::DeclRef(_, decl_id, _) = self.ast_context[func].kind else { return false; };
        let CDeclKind::Function { typ, .. } = self.ast_context[decl_id].kind else { return false; };
        matches!(
            self.ast_context.resolve_type(typ).kind,
            CTypeKind::Function(_, _, true, _, _)
        )
    }

    fn is_variadic_function_pointer_callee(&self, func: CExprId) -> bool {
        // A direct reference to a variadic C declaration is represented by
        // Clang as an implicit function-to-pointer conversion at the call
        // site.  That is still our supported direct ABI boundary.  Only an
        // actual indirect expression (variable, field, dereference, etc.)
        // is the unsupported function-pointer boundary.
        if self.is_direct_function_declaration(func) {
            return false;
        }
        let Some(ty) = self.ast_context[func].kind.get_qual_type() else { return false; };
        let CTypeKind::Pointer(pointee) = self.ast_context.resolve_type(ty.ctype).kind else { return false; };
        matches!(
            self.ast_context.resolve_type(pointee.ctype).kind,
            CTypeKind::Function(_, _, true, _, _)
        )
    }

    /// The only source-call boundary conversions for the canonical raw-memory
    /// runtime.  Keeping them here prevents type repair from leaking to the
    /// printer or into each individual libc special case.
    fn lower_runtime_arg(&self, arg: DaExpr, kind: RuntimeArgKind) -> DaExpr {
        match kind {
            RuntimeArgKind::UInt64 => self.integer_literal_for_type(arg, DaType::uint64()),
            RuntimeArgKind::RawAddress => self.pointer_to_raw_address(arg),
            RuntimeArgKind::UInt8 => self.integer_literal_for_type(arg, DaType::uint8()),
        }
    }

    pub(crate) fn direct_call_name(&self, func: CExprId) -> Option<String> {
        let mut func = func;
        while let CExprKind::ImplicitCast(_, inner, _, _, _) = &self.ast_context[func].kind {
            func = *inner;
        }
        let CExprKind::DeclRef(_, decl_id, _) = &self.ast_context[func].kind else {
            return None;
        };
        self.ast_context[*decl_id].kind.get_name().cloned()
    }

    /// True when this declaration comes from the translation unit's own source
    /// file rather than from an included header.
    ///
    /// If the main file cannot be located in the source map the answer is `true`:
    /// mislabelling every function as external would be a far worse failure than
    /// exporting a few header helpers.
    fn is_defined_in_main_file(&self, decl_id: CDeclId) -> bool {
        let Some(main_file_id) = self.ast_context.find_file_id(&self.main_file) else {
            return true;
        };
        self.ast_context
            .file_id(&self.ast_context[decl_id])
            .map_or(true, |file_id| file_id == main_file_id)
    }

    /// The daScript name of the function a value-position expression names, if
    /// it names one directly (`add`, `(add)`, `*add`, …).
    pub(crate) fn direct_function_reference(&self, expr: CExprId) -> Option<String> {
        let mut current = expr;
        loop {
            match &self.ast_context[current].kind {
                CExprKind::Paren(_, inner) => current = *inner,
                CExprKind::ImplicitCast(_, inner, CastKind::FunctionToPointerDecay, _, _)
                | CExprKind::ImplicitCast(_, inner, CastKind::LValueToRValue, _, _) => {
                    current = *inner
                }
                CExprKind::Unary(_, CUnOp::Deref, inner, _) => current = *inner,
                CExprKind::DeclRef(_, decl_id, _) => {
                    let CDeclKind::Function { ref name, .. } = self.ast_context[*decl_id].kind
                    else {
                        return None;
                    };
                    return Some(self.declare_value_name(*decl_id, name));
                }
                _ => return None,
            }
        }
    }

    /// Peel the C conversions that only exist because C has no function values.
    ///
    /// `f(…)`, `(*f)(…)` and `(**f)(…)` all call the same object; Clang spells
    /// them as a decay-to-pointer over zero or more dereferences of an lvalue.
    /// daScript's `function<…>` *is* the value, so every one of those layers is
    /// an identity.  Returns the expression that actually holds the function
    /// value, or `None` when this is not a function-pointer callee at all.
    fn function_value_operand(&self, func: CExprId) -> Option<CExprId> {
        let mut current = func;
        let mut peeled = false;
        loop {
            match &self.ast_context[current].kind {
                CExprKind::ImplicitCast(_, inner, CastKind::FunctionToPointerDecay, _, _)
                | CExprKind::ImplicitCast(_, inner, CastKind::LValueToRValue, _, _) => {
                    current = *inner;
                    peeled = true;
                }
                CExprKind::Unary(_, CUnOp::Deref, inner, _) => {
                    current = *inner;
                    peeled = true;
                }
                CExprKind::Paren(_, inner) => {
                    current = *inner;
                    peeled = true;
                }
                _ => break,
            }
        }
        // The remaining expression must still be a function pointer (or a
        // function): anything else means we peeled through a real C pointer.
        let is_callable = self.ast_context[current]
            .kind
            .get_qual_type()
            .map(|ty| self.is_callable_type(ty.ctype))
            .unwrap_or(false);
        (peeled && is_callable).then_some(current)
    }

    /// True for a C function type or a pointer to one.
    pub(crate) fn is_callable_type(&self, ctype: CTypeId) -> bool {
        match self.ast_context.resolve_type(ctype).kind {
            CTypeKind::Function(..) => true,
            CTypeKind::Pointer(inner) => matches!(
                self.ast_context.resolve_type(inner.ctype).kind,
                CTypeKind::Function(..)
            ),
            _ => false,
        }
    }

    /// Reject a direct call to a C function that this translation unit neither
    /// defines nor lowers to the compiler-owned runtime.
    ///
    /// Emitting the bare call anyway produced daScript that names a function
    /// which does not exist — a link error at best, and silently different
    /// behaviour whenever daScript happened to have a name of its own.
    fn reject_unknown_external_call(&self, func: CExprId) -> TranslationResult<()> {
        let mut callee = func;
        while let CExprKind::ImplicitCast(_, inner, _, _, _) = &self.ast_context[callee].kind {
            callee = *inner;
        }
        let CExprKind::DeclRef(_, decl_id, _) = self.ast_context[callee].kind else {
            return Ok(());
        };
        let CDeclKind::Function {
            ref name, body, ..
        } = self.ast_context[decl_id].kind
        else {
            return Ok(());
        };
        if body.is_some() || canonical_runtime_function(name).is_some() {
            return Ok(());
        }
        // A declaration without a body may still be defined elsewhere in this
        // translation unit; `body` is only set on the defining declaration.
        if self.ast_context.iter_decls().any(|(_, decl)| {
            matches!(&decl.kind, CDeclKind::Function { name: other, body: Some(_), .. }
                if other == name)
        }) {
            return Ok(());
        }
        Err(format_translation_err!(
            self.ast_context.display_loc(&self.ast_context[func].loc),
            "unsupported external call: {}",
            name,
        ))
    }

    fn is_direct_function_declaration(&self, func: CExprId) -> bool {
        let mut func = func;
        while let CExprKind::ImplicitCast(_, inner, _, _, _) = &self.ast_context[func].kind {
            func = *inner;
        }
        let CExprKind::DeclRef(_, decl_id, _) = self.ast_context[func].kind else {
            return false;
        };
        matches!(self.ast_context[decl_id].kind, CDeclKind::Function { .. })
    }

    pub(crate) fn libc_memory_arg_cast(
        &self,
        func_name: Option<&str>,
        idx: usize,
    ) -> Option<DaType> {
        if idx != 2 || !matches!(func_name, Some("memset" | "memcpy" | "memmove")) {
            return None;
        }
        Some(DaType::uint64())
    }

    pub fn convert_function_param(
        &self,
        _ctx: ExprContext,
        typ: CQualTypeId,
    ) -> TranslationResult<DaType> {
        if self.ast_context.is_va_list(typ.ctype) {
            return Ok(DaType::uint64());
        }
        self.convert_type(typ)
    }

    pub fn convert_call_args(
        &self,
        ctx: ExprContext,
        exprs: &[CExprId],
        arg_tys: Option<&[CQualTypeId]>,
        _is_variadic: bool,
    ) -> TranslationResult<WithStmts<Vec<DaExpr>>> {
        let arg_tys = arg_tys.unwrap_or(&[]);
        exprs
            .iter()
            .enumerate()
            .map(|(n, arg)| self.convert_call_arg(ctx, *arg, arg_tys.get(n).copied()))
            .collect()
    }
    fn convert_call_arg(
        &self,
        ctx: ExprContext,
        expr_id: CExprId,
        override_ty: Option<CQualTypeId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        self.convert_expr(ctx, expr_id, override_ty)
    }

    fn add_definition_param_aliases(
        &self,
        body_id: CStmtId,
        param_bindings: &[(CDeclId, String, CQualTypeId, String)],
    ) {
        if param_bindings.is_empty() {
            return;
        }

        let local_decls: IndexSet<CDeclId> = DFExpr::new(&self.ast_context, body_id.into())
            .filter_map(SomeId::stmt)
            .flat_map(|sid| match &self.ast_context[sid].kind {
                CStmtKind::Decls(decls) => decls.clone(),
                _ => vec![],
            })
            .collect();

        let mut candidates = vec![];
        let mut seen = IndexSet::new();
        for expr_id in DFExpr::new(&self.ast_context, body_id.into()).filter_map(SomeId::expr) {
            if let CExprKind::DeclRef(_, decl_id, _) = self.ast_context[expr_id].kind {
                if local_decls.contains(&decl_id) || !seen.insert(decl_id) {
                    continue;
                }
                if let CDeclKind::Variable { ref ident, typ, .. } = self.ast_context[decl_id].kind {
                    if !ident.is_empty() {
                        candidates.push((decl_id, ident.clone(), typ));
                    }
                }
            }
        }

        let mut used_candidates = IndexSet::new();
        for (_param_id, param_ident, param_ty, pname) in param_bindings {
            if !param_ident.starts_with("__") && !param_ident.is_empty() {
                continue;
            }
            if let Some((decl_id, ident, _)) = candidates.iter().find(|(decl_id, ident, typ)| {
                !used_candidates.contains(decl_id)
                    && ident != param_ident
                    && typ.ctype == param_ty.ctype
            }) {
                used_candidates.insert(*decl_id);
                self.function_context
                    .borrow_mut()
                    .add_param_alias(ident, pname);
            }
        }
    }
}

/// Drop the implicit conversions Clang records around an expression.
fn strip_implicit_casts(ast_context: &TypedAstContext, expr: CExprId) -> CExprId {
    let mut current = expr;
    while let CExprKind::ImplicitCast(_, inner, _, _, _) = &ast_context[current].kind {
        current = *inner;
    }
    current
}

fn canonical_runtime_arg_type(
    runtime: Option<CanonicalRuntimeFunction>,
    idx: usize,
) -> Option<RuntimeArgKind> {
    runtime.and_then(|function| function.arg_kind(idx))
}

pub(crate) fn normalize_array_initializer_for_type(expr: DaExpr, ty: &DaType) -> DaExpr {
    let DaTypeKind::Array(elem_ty) = &ty.kind else {
        return expr;
    };
    let DaExpr::MakeArray(items) = expr else {
        return expr;
    };
    DaExpr::MakeArray(
        items
            .into_iter()
            .map(|item| {
                if is_zero_initializer_expr(&item) {
                    default_initializer_for_datype(elem_ty.as_ref())
                } else {
                    item
                }
            })
            .collect(),
    )
}

fn default_initializer_for_datype(ty: &DaType) -> DaExpr {
    match &ty.kind {
        DaTypeKind::Named(name) => DaExpr::Call(Box::new(DaExpr::Var(name.clone())), vec![]),
        _ => zero_for_datype(ty),
    }
}
