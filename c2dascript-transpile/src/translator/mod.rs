use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Index;
use std::path::Path;
use std::path::PathBuf;

use c2dascript_ast_builder::mk;

use indexmap::IndexSet;
use log::warn;

use crate::c_ast::iterators::{DFExpr, SomeId};
use crate::c_ast::*;
use crate::convert_type::TypeConverter;
use crate::diagnostics::TranslationResult;
use crate::format_translation_err;
use crate::renamer::Renamer;
use crate::with_stmts::WithStmts;
use crate::ExternCrate;
use crate::TranspilerConfig;

use das_ast::{
    DaAlias, DaBlock, DaDecl, DaEnumVariant, DaEnumeration, DaExpr, DaField, DaFunction, DaModule,
    DaStmt, DaStructure, DaType, DaTypeKind, DaVariable,
};

mod abi;
mod assembly;
mod atomics;
mod builtins;
mod comments;
mod enums;
mod functions;
mod layout;
mod literals;
mod macros;
mod named_references;
mod object_memory;
mod operators;
mod pointers;
mod runtime;
mod simd;
mod structs_unions;
pub(crate) mod value_lowering;
mod variadic;

use self::value_lowering::ValueSite;

pub use crate::diagnostics::{TranslationError, TranslationErrorKind};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct Import {
    decl_id: CDeclId,
    ident_name: String,
}
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum DecayRef {
    Yes,
    #[default]
    Default,
    No,
}

impl DecayRef {
    pub fn is_yes(&self) -> bool {
        match self {
            DecayRef::Yes => true,
            DecayRef::Default => true,
            DecayRef::No => false,
        }
    }

    pub fn is_no(&self) -> bool {
        !self.is_yes()
    }

    pub fn set_default_to_no(&mut self) {
        if *self == DecayRef::Default {
            *self = DecayRef::No;
        }
    }
}

impl From<bool> for DecayRef {
    fn from(b: bool) -> Self {
        match b {
            true => DecayRef::Yes,
            false => DecayRef::No,
        }
    }
}

pub(crate) fn anonymous_struct_signature(s: &DaStructure) -> (String, Vec<String>) {
    let fields = s
        .fields
        .iter()
        .map(|f| format!("{}:{}", f.name, f.field_type))
        .collect();
    (s.name.clone(), fields)
}

#[derive(Clone, Debug, Default)]
pub struct FuncContext {
    name: Option<String>,
    /// Name of the va_list argument for variadic functions
    va_list_arg_name: Option<String>,
    /// Local va_list declarations that belong to the canonical variadic ABI.
    va_list_decl_ids: Option<IndexSet<CDeclId>>,
    param_aliases: HashMap<String, String>,
    return_type: Option<CQualTypeId>,
}

impl FuncContext {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn enter_new(&mut self, fn_name: &str) {
        *self = Self {
            name: Some(fn_name.to_string()),
            ..Default::default()
        };
    }
    pub fn set_return_type(&mut self, ret_ty: Option<CQualTypeId>) {
        self.return_type = ret_ty;
    }
    pub fn get_return_type(&self) -> Option<CQualTypeId> {
        self.return_type
    }
    pub fn get_name(&self) -> &str {
        self.name.as_deref().unwrap_or("<unknown>")
    }
    pub fn get_va_list_arg_name(&self) -> &str {
        self.va_list_arg_name
            .as_deref()
            .expect("va_list_arg_name not set")
    }
    pub fn set_va_list_context(&mut self, arg_name: String, decls: IndexSet<CDeclId>) {
        self.va_list_arg_name = Some(arg_name);
        self.va_list_decl_ids = Some(decls);
    }
    pub fn add_param_alias(&mut self, c_name: &str, das_name: &str) {
        if !c_name.is_empty() {
            self.param_aliases
                .insert(c_name.to_string(), das_name.to_string());
        }
    }
    pub fn get_param_alias(&self, c_name: &str) -> Option<String> {
        self.param_aliases.get(c_name).cloned()
    }
}

/// Options that impact an expression and all of its subexpressions.
#[derive(Copy, Clone, Debug, Default)]
pub struct ExprContext {
    pub used: bool,
    pub is_const: bool,
    pub is_static: bool,
    pub decay_ref: DecayRef,
    pub is_bitfield_write: bool,
    pub needs_address: bool,
    pub ternary_needs_parens: bool,
    pub expanding_macro: Option<CDeclId>,
}

impl ExprContext {
    pub fn used(self) -> Self {
        ExprContext { used: true, ..self }
    }
    pub fn unused(self) -> Self {
        ExprContext {
            used: false,
            ..self
        }
    }
    pub fn is_used(&self) -> bool {
        self.used
    }
    pub fn is_unused(&self) -> bool {
        !self.used
    }
    pub fn decay_ref(self) -> Self {
        ExprContext {
            decay_ref: DecayRef::Yes,
            ..self
        }
    }
    pub fn const_(self) -> Self {
        ExprContext {
            is_const: true,
            ..self
        }
    }
    pub fn not_const(self) -> Self {
        ExprContext {
            is_const: false,
            ..self
        }
    }
    pub fn not_static(self) -> Self {
        ExprContext {
            is_static: false,
            ..self
        }
    }
    pub fn static_(self) -> Self {
        ExprContext {
            is_static: true,
            ..self
        }
    }
    pub fn is_bitfield_write(&self) -> bool {
        self.is_bitfield_write
    }
    pub fn set_bitfield_write(self, is_bitfield_write: bool) -> Self {
        ExprContext {
            is_bitfield_write,
            ..self
        }
    }
    pub fn needs_address(&self) -> bool {
        self.needs_address
    }
    pub fn set_needs_address(self, needs_address: bool) -> Self {
        ExprContext {
            needs_address,
            ..self
        }
    }
    pub fn expanding_macro(&self, mac: &CDeclId) -> bool {
        match self.expanding_macro {
            Some(expanding) => expanding == *mac,
            None => false,
        }
    }
    pub fn set_expanding_macro(self, mac: CDeclId) -> Self {
        ExprContext {
            expanding_macro: Some(mac),
            ..self
        }
    }
}

pub struct Translation<'c> {
    pub ast_context: TypedAstContext,
    pub tcfg: &'c TranspilerConfig,
    pub function_context: RefCell<FuncContext>,
    pub type_converter: RefCell<TypeConverter>,
    pub renamer: RefCell<Renamer<CDeclId>>,
    pub emitted_structs: std::cell::RefCell<std::collections::HashSet<String>>,
    pub emitted_anon_structs: std::cell::RefCell<std::collections::HashSet<(String, Vec<String>)>>,
    pub(crate) layout_cache: RefCell<HashMap<CTypeId, self::layout::CLayout>>,
    /// Module-level variables synthesised while lowering function bodies.
    /// Currently only C function-scope `static` storage, which has to outlive
    /// the call that declares it. Drained once, by `translate_impl`.
    pub(crate) hoisted_statics: RefCell<Vec<DaDecl>>,
    pub main_file: PathBuf,
}

impl<'c> Translation<'c> {
    pub fn new(ast_context: TypedAstContext, tcfg: &'c TranspilerConfig, main_file: &Path) -> Self {
        Translation {
            type_converter: RefCell::new(TypeConverter::new(tcfg)),
            renamer: RefCell::new(Renamer::global_value_namespace()),
            function_context: RefCell::new(FuncContext::new()),
            emitted_structs: std::cell::RefCell::new(std::collections::HashSet::new()),
            emitted_anon_structs: std::cell::RefCell::new(std::collections::HashSet::new()),
            layout_cache: RefCell::new(HashMap::new()),
            hoisted_statics: RefCell::new(vec![]),
            ast_context,
            tcfg,
            main_file: main_file.to_path_buf(),
        }
    }

    /// Take the module-level variables synthesised for function-scope `static`s.
    pub(crate) fn take_hoisted_statics(&self) -> Vec<DaDecl> {
        std::mem::take(&mut *self.hoisted_statics.borrow_mut())
    }

    pub fn declare_value_name(&self, decl_id: CDeclId, name: &str) -> String {
        {
            let renamer = self.renamer.borrow();
            if let Some(existing) = renamer.get(&decl_id) {
                return existing;
            }
        }
        self.renamer
            .borrow_mut()
            .insert(decl_id, name)
            .expect("Value name already assigned")
    }

    pub fn convert_decl(&self, ctx: ExprContext, decl_id: CDeclId) -> TranslationResult<DaDecl> {
        let decl = &self.ast_context[decl_id];
        use CDeclKind::*;
        match &decl.kind {
            Function {
                name,
                parameters,
                body,
                typ,
                is_global,
                is_inline,
                is_extern,
                attrs,
                ..
            } => self.convert_function(ctx, decl_id, name, *typ, parameters, *body, attrs),
            Variable {
                ident,
                typ,
                initializer,
                has_static_duration,
                ..
            } => self.convert_variable(
                ctx,
                decl_id,
                ident,
                *typ,
                *initializer,
                *has_static_duration,
            ),
            Typedef {
                name,
                typ,
                is_implicit,
                ..
            } => {
                // Skip __-prefixed builtin typedefs (__int128_t, __builtin_va_list, etc.)
                if name.starts_with("__") {
                    return Err(TranslationError::generic("skipping implicit typedef"));
                }
                // Check if this typedef is for a struct/union/enum (named or anonymous).
                // If the inner decl has no name (anonymous), use the typedef name.
                // If it has a name (e.g., Clang-generated), still emit the struct definition.
                let resolved = self.ast_context.resolve_type(typ.ctype);
                if *is_implicit
                    && !matches!(
                        resolved.kind,
                        CTypeKind::Struct(_) | CTypeKind::Union(_) | CTypeKind::Enum(_)
                    )
                {
                    return Err(TranslationError::generic("skipping implicit typedef"));
                }
                match &resolved.kind {
                    CTypeKind::Struct(rec_id)
                    | CTypeKind::Union(rec_id)
                    | CTypeKind::Enum(rec_id) => {
                        let inner_decl = &self.ast_context[*rec_id];
                        let typedef_target = inner_decl
                            .kind
                            .get_name()
                            .and_then(|n| {
                                let s = n.trim();
                                if s.is_empty() || s.starts_with("(") {
                                    None
                                } else {
                                    Some(s.to_string())
                                }
                            })
                            .unwrap_or_else(|| name.clone());
                        // Emit named struct/enum with typedef name (or inner name for non-anonymous)
                        match &resolved.kind {
                            CTypeKind::Struct(_) | CTypeKind::Union(_) => {
                                let fields = match &inner_decl.kind {
                                    CDeclKind::Struct { fields, .. }
                                    | CDeclKind::Union { fields, .. } => fields,
                                    _ => &None,
                                };
                                if let Some(fids) = fields {
                                    for &fid in fids {
                                        if let CDeclKind::Field { ref name, .. } =
                                            self.ast_context[fid].kind
                                        {
                                            self.type_converter
                                                .borrow_mut()
                                                .declare_field_name(*rec_id, fid, name);
                                        }
                                    }
                                }
                                let das_fields = fields
                                    .as_ref()
                                    .map(|fids| {
                                        fids.iter()
                                            .filter_map(|fid| {
                                                if let CDeclKind::Field { ref name, typ, .. } =
                                                    self.ast_context[*fid].kind
                                                {
                                                    let ft = self.convert_type(typ.clone()).ok()?;
                                                    Some(DaField {
                                                        name: self
                                                            .type_converter
                                                            .borrow()
                                                            .resolve_field_name(Some(*rec_id), *fid)
                                                            .unwrap_or_else(|| name.clone()),
                                                        field_type: ft,
                                                        default: None,
                                                    })
                                                } else {
                                                    None
                                                }
                                            })
                                            .collect::<Vec<_>>()
                                    })
                                    .unwrap_or_default();
                                return Ok(DaDecl::Structure(DaStructure {
                                    name: self
                                        .type_converter
                                        .borrow_mut()
                                        .ensure_decl_name(decl_id, &typedef_target),
                                    fields: das_fields,
                                    annotations: vec![],
                                }));
                            }
                            CTypeKind::Enum(_) => {
                                let variants = match &inner_decl.kind {
                                    CDeclKind::Enum { variants, .. } => variants.clone(),
                                    _ => vec![],
                                };
                                let mut das_variants = vec![];
                                for &vid in &variants {
                                    if let CDeclKind::EnumConstant { ref name, value } =
                                        self.ast_context[vid].kind
                                    {
                                        let das_val = match value {
                                            crate::c_ast::ConstIntExpr::U(v) => {
                                                Some(DaExpr::ConstUInt(v))
                                            }
                                            crate::c_ast::ConstIntExpr::I(v) => {
                                                Some(DaExpr::ConstInt(v))
                                            }
                                        };
                                        das_variants.push(DaEnumVariant {
                                            name: name.clone(),
                                            value: das_val,
                                        });
                                    }
                                }
                                return Ok(DaDecl::Enumeration(DaEnumeration {
                                    name: self
                                        .type_converter
                                        .borrow_mut()
                                        .ensure_decl_name(decl_id, &typedef_target),
                                    base_type: DaType::int(),
                                    variants: das_variants,
                                }));
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
                // Skip redundant `typedef Foo = Foo` — struct уже создаёт тип
                let resolved = self.ast_context.resolve_type(typ.ctype);
                if let CTypeKind::Struct(decl_id)
                | CTypeKind::Enum(decl_id)
                | CTypeKind::Union(decl_id) = &resolved.kind
                {
                    if let Some(struct_name) = self.ast_context[*decl_id].kind.get_name() {
                        if *struct_name == *name {
                            return Err(TranslationError::generic(
                                "redundant self-typedef, skipping",
                            ));
                        }
                    }
                }
                // Resolve through typedef chain to get base type
                let resolved_id = self.ast_context.resolve_type_id(typ.ctype);
                let final_type = match self.convert_type_inner(resolved_id) {
                    Ok(t) if !matches!(t.kind, DaTypeKind::Auto) => t,
                    Err(_) => {
                        // An alias for a C type with no daScript
                        // representation gets no declaration of its own.
                        // Aliasing it to `auto` would hide the gap and let
                        // every use of the name silently lower to something
                        // else; the diagnostic belongs at each use site, where
                        // the type actually has to be laid out.
                        return Err(TranslationError::generic(
                            "typedef target has no daScript representation; reported at each use",
                        ));
                    }
                    Ok(_) => {
                        let r = self.ast_context.resolve_type(resolved_id);
                        type_kind_to_datype(&r.kind)
                    }
                };
                Ok(DaDecl::Alias(DaAlias {
                    name: self
                        .type_converter
                        .borrow_mut()
                        .ensure_decl_name(decl_id, name),
                    aliased_type: final_type,
                }))
            }
            Struct {
                name: None, fields, ..
            } => {
                // Anonymous struct — emit as Unnamed_N
                // First check if typedef already handled this via prenamed_decls
                let typedef_name = self
                    .ast_context
                    .prenamed_decls
                    .iter()
                    .find(|(_, &v)| v == decl_id)
                    .and_then(|(k, _)| {
                        if let CDeclKind::Typedef { name, .. } = &self.ast_context[*k].kind {
                            Some(name.clone())
                        } else {
                            None
                        }
                    });
                if let Some(tname) = typedef_name {
                    // Already handled by Typedef — skip
                    return Err(TranslationError::generic(
                        "anonymous struct (will be handled by typedef)",
                    ));
                }
                // No typedef — need to generate the struct body with a generated name
                self.convert_union(decl_id, &None, fields)
            }
            Struct { name, fields, .. } => self.convert_struct(decl_id, name, fields),
            Enum {
                name,
                variants,
                integral_type,
            } => self.convert_enum(decl_id, name, variants, *integral_type),
            Union {
                name: None, fields, ..
            } => {
                // Anonymous union — daScript has no union, map to struct.
                // Must NOT skip: field types (resolved by convert_inner) may
                // reference this union by its generated Unnamed_N label, and
                // the struct definition must exist in the output.
                self.convert_struct(decl_id, &None, fields)
            }
            Union { name, fields, .. } => {
                // daScript has no union; map to struct
                self.convert_union(decl_id, name, fields)
            }
            MacroObject { name } | MacroFunction { name, .. } => {
                self.convert_macro(ctx, decl_id, name)
            }
            _ => Err(TranslationError::generic("unsupported decl kind")),
        }
    }

    /// Convert a C compound statement into daScript statements
    // Label counter for daScript's integer label syntax
    fn label_name(&self, c_label_id: &CStmtId) -> String {
        let name = self
            .ast_context
            .label_names
            .get(c_label_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("_l{}", c_label_id.0));
        let mut h = 0u64;
        for b in name.bytes() {
            h = h.wrapping_mul(31).wrapping_add(b as u64);
        }
        let label_num = (h % 100000) as i64;
        format!("label {}", label_num)
    }

    pub fn convert_stmt(&self, stmt_id: CStmtId) -> TranslationResult<WithStmts<Vec<DaStmt>>> {
        let stmt = &self.ast_context[stmt_id];
        match &stmt.kind {
            CStmtKind::Compound(ref children) => {
                let mut result = vec![];
                let mut is_unsafe = false;
                for &s in children {
                    let sub = self.convert_stmt(s)?;
                    is_unsafe |= sub.is_unsafe;
                    result.extend(sub.val);
                }
                Ok(WithStmts {
                    stmts: vec![],
                    val: result,
                    is_unsafe,
                })
            }
            CStmtKind::Expr(expr_id) => {
                // Use `used: true` so assignments inside expressions are split into
                // `stmts` (side effects) + `val` (result), rather than being embedded
                // as compound expressions which daScript can't parse.
                let v = self.convert_expr(
                    ExprContext {
                        used: true,
                        is_const: false,
                        ..Default::default()
                    },
                    *expr_id,
                    None,
                )?;
                let mut result = v.stmts;
                result.push(mk().expr_stmt(v.val));
                Ok(WithStmts {
                    stmts: vec![],
                    val: result,
                    is_unsafe: v.is_unsafe,
                })
            }
            CStmtKind::Return(expr_id) => {
                let val = expr_id
                    .map(|e| {
                        self.convert_expr(
                            ExprContext {
                                used: true,
                                is_const: false,
                                ..Default::default()
                            },
                            e,
                            None,
                        )
                    })
                    .transpose()?;
                let ret_ty = self.function_context.borrow().get_return_type();
                let val = match (val, ret_ty) {
                    (Some(ws), Some(ret_ty)) => Some(self.lower_to_c_value(
                        ws,
                        expr_id.and_then(|e| self.ast_context[e].kind.get_qual_type()),
                        self.convert_type(ret_ty)?,
                        ValueSite::Return,
                    )?),
                    (value, _) => value,
                };
                let is_unsafe = val.as_ref().map(|v| v.is_unsafe).unwrap_or(false);
                fn is_if_then_else(expr: &DaExpr) -> bool {
                    match expr {
                        DaExpr::IfThenElse { .. } => true,
                        DaExpr::Cast { expr: inner, .. } => {
                            matches!(inner.as_ref(), DaExpr::IfThenElse { .. })
                        }
                        _ => false,
                    }
                }
                let stmts = match &val {
                    Some(ws) if is_if_then_else(&ws.val) => {
                        // return if(c) a else b → if(c) return a else return b
                        let ife = &ws.val;
                        let mut out = vec![];
                        convert_ifexpr_to_return(ife, &mut out);
                        out
                    }
                    _ => {
                        let mut stmts = vec![];
                        if let Some(ref ws) = val {
                            stmts.extend(ws.stmts.clone());
                        }
                        let ret_val = val
                            .map(|ws| {
                                let val = if let Some(ret_ty) =
                                    self.function_context.borrow().get_return_type()
                                {
                                    let ret_da = self.convert_type(ret_ty)?;
                                    let expr_is_ptr = expr_id
                                        .and_then(|e| self.ast_context[e].kind.get_qual_type())
                                        .map_or(false, |qty| self.is_pointer_type(qty.ctype));
                                    if matches!(ret_da.kind, DaTypeKind::UInt64) && expr_is_ptr {
                                        self.pointer_to_raw_address(ws.val)
                                    } else if matches!(ret_da.kind, DaTypeKind::Pointer(_)) {
                                        if expr_is_ptr {
                                            self.abi_pointer_cast(ws.val, ret_da)
                                        } else {
                                            self.raw_address_to_pointer(ws.val, ret_da)
                                        }
                                    } else if matches!(ret_da.kind, DaTypeKind::Named(_))
                                        && Self::infer_type(&ws.val)
                                            .map_or(true, |inferred| inferred != ret_da)
                                    {
                                        DaExpr::Unsafe(Box::new(DaExpr::Cast {
                                            kind: das_ast::CastKind::Reinterpret,
                                            expr: Box::new(ws.val),
                                            to: ret_da,
                                        }))
                                    } else {
                                        ws.val
                                    }
                                } else {
                                    ws.val
                                };
                                Ok::<Box<DaExpr>, TranslationError>(Box::new(val))
                            })
                            .transpose()?;
                        stmts.push(mk().expr_stmt(DaExpr::Return(ret_val)));
                        stmts
                    }
                };
                Ok(WithStmts {
                    stmts: vec![],
                    val: stmts,
                    is_unsafe,
                })
            }
            CStmtKind::Decls(ref decls) => {
                let mut result = vec![];
                for &d in decls {
                    if let Ok(das_decl) = self.convert_decl(
                        ExprContext {
                            used: true,
                            is_const: false,
                            ..Default::default()
                        },
                        d,
                    ) {
                        // Skip declarations already emitted in Pass 1.
                        let already = match &das_decl {
                            DaDecl::Structure(s) => {
                                if s.name.starts_with("Unnamed_") {
                                    self.emitted_anon_structs
                                        .borrow()
                                        .contains(&anonymous_struct_signature(s))
                                } else {
                                    self.emitted_structs.borrow().contains(&s.name)
                                }
                            }
                            DaDecl::Enumeration(e) => {
                                self.emitted_structs.borrow().contains(&e.name)
                            }
                            _ => false,
                        };
                        if already {
                            continue;
                        }
                        result.push(DaStmt::Decl(das_decl));
                    }
                }
                Ok(WithStmts {
                    stmts: vec![],
                    val: result,
                    is_unsafe: false,
                })
            }
            CStmtKind::If {
                scrutinee,
                true_variant,
                false_variant,
            } => {
                let ctx_used = ExprContext {
                    used: true,
                    is_const: false,
                    ..Default::default()
                };
                let cond = self.convert_condition(ctx_used, true, *scrutinee)?;
                let then_ws = self.convert_stmt(*true_variant)?;
                let then_expr = DaExpr::Block(DaBlock { stmts: then_ws.val });
                let elifs = vec![];
                let (else_expr, else_unsafe) = match false_variant {
                    Some(fv) => {
                        let else_ws = self.convert_stmt(*fv)?;
                        (
                            Some(Box::new(DaExpr::Block(DaBlock { stmts: else_ws.val }))),
                            else_ws.is_unsafe,
                        )
                    }
                    None => (None, false),
                };
                let mut stmts = cond.stmts;
                stmts.push(mk().expr_stmt(DaExpr::IfThenElse {
                    cond: Box::new(cond.val),
                    then: Box::new(then_expr),
                    elifs,
                    else_: else_expr,
                }));
                Ok(WithStmts {
                    stmts: vec![],
                    val: stmts,
                    is_unsafe: cond.is_unsafe || then_ws.is_unsafe || else_unsafe,
                })
            }
            CStmtKind::While { condition, body } => {
                let ctx_used = ExprContext {
                    used: true,
                    is_const: false,
                    ..Default::default()
                };
                let cond = self.convert_condition(ctx_used, true, *condition)?;
                let body_ws = self.convert_stmt(*body)?;
                let body_expr = DaExpr::Block(DaBlock { stmts: body_ws.val });
                Ok(WithStmts {
                    stmts: vec![],
                    val: vec![
                        mk().expr_stmt(DaExpr::While(Box::new(cond.val), Box::new(body_expr)))
                    ],
                    is_unsafe: cond.is_unsafe || body_ws.is_unsafe,
                })
            }
            CStmtKind::DoWhile { body, condition } => {
                let first_var = format!("_dw_{}", stmt_id.0);
                let ctx_used = ExprContext {
                    used: true,
                    is_const: false,
                    ..Default::default()
                };
                let body_ws = self.convert_stmt(*body)?;
                let cond = self.convert_expr(ctx_used, *condition, None)?;

                let mut loop_stmts = vec![];
                loop_stmts.push(DaStmt::Expr(DaExpr::Assign(
                    Box::new(DaExpr::Var(first_var.clone())),
                    Box::new(DaExpr::ConstBool(false)),
                )));
                loop_stmts.extend(body_ws.val);

                let set_first = DaStmt::Var {
                    name: first_var.clone(),
                    var_type: DaType::bool(),
                    init: Some(DaExpr::ConstBool(true)),
                };

                let cond_val = match &cond.val {
                    DaExpr::ConstInt(0) => DaExpr::ConstBool(false),
                    _ => cond.val,
                };
                let cond_or_first = DaExpr::Op2 {
                    op: "||",
                    left: Box::new(DaExpr::Var(first_var)),
                    right: Box::new(cond_val),
                };
                Ok(WithStmts {
                    stmts: vec![],
                    val: vec![
                        set_first,
                        mk().expr_stmt(DaExpr::While(
                            Box::new(cond_or_first),
                            Box::new(DaExpr::Block(DaBlock { stmts: loop_stmts })),
                        )),
                    ],
                    is_unsafe: body_ws.is_unsafe || cond.is_unsafe,
                })
            }
            CStmtKind::ForLoop {
                init,
                condition,
                increment,
                body,
            } => {
                let ctx_used = ExprContext {
                    used: true,
                    is_const: false,
                    ..Default::default()
                };
                let mut result = vec![];
                let mut is_unsafe = false;

                if let Some(init_id) = init {
                    let init_ws = self.convert_stmt(*init_id)?;
                    is_unsafe |= init_ws.is_unsafe;
                    result.extend(init_ws.val);
                }

                let body_ws = self.convert_stmt(*body)?;
                is_unsafe |= body_ws.is_unsafe;
                let mut loop_body = body_ws.val;

                if let Some(inc_id) = increment {
                    let inc = self.convert_expr(ctx_used, *inc_id, None)?;
                    is_unsafe |= inc.is_unsafe;
                    if inc.stmts.is_empty() {
                        loop_body.push(DaStmt::Expr(inc.val));
                    } else {
                        loop_body.extend(inc.stmts);
                    }
                }

                let cond_expr = match condition {
                    Some(cond_id) => {
                        let cond = self.convert_condition(ctx_used, true, *cond_id)?;
                        is_unsafe |= cond.is_unsafe;
                        cond.val
                    }
                    None => DaExpr::ConstBool(true),
                };

                result.push(mk().expr_stmt(DaExpr::While(
                    Box::new(cond_expr),
                    Box::new(DaExpr::Block(DaBlock { stmts: loop_body })),
                )));
                Ok(WithStmts {
                    stmts: vec![],
                    val: result,
                    is_unsafe,
                })
            }
            CStmtKind::Switch { scrutinee, body } => {
                let ctx_u = ExprContext {
                    used: true,
                    is_const: false,
                    ..Default::default()
                };
                let cond = self.convert_expr(ctx_u, *scrutinee, None)?;
                let (cases, cases_unsafe) = self.collect_switch_cases(*body)?;
                let if_chain = self.build_switch_chain(&cond.val, &cases);
                Ok(WithStmts {
                    stmts: vec![],
                    val: vec![mk().expr_stmt(if_chain)],
                    is_unsafe: cond.is_unsafe || cases_unsafe,
                })
            }
            CStmtKind::Case(_, _, _) | CStmtKind::Default(_) => Ok(WithStmts {
                stmts: vec![],
                val: vec![],
                is_unsafe: false,
            }),
            CStmtKind::Goto(label_id) => {
                let ln = self.label_name(label_id);
                Ok(WithStmts {
                    stmts: vec![],
                    val: vec![mk().expr_stmt(DaExpr::Goto(ln))],
                    is_unsafe: false,
                })
            }
            CStmtKind::Label(sub_stmt) => {
                let ln = self.label_name(&stmt_id);
                let sub = self.convert_stmt(*sub_stmt)?;
                let mut stmts = vec![mk().expr_stmt(DaExpr::Label(ln))];
                stmts.extend(sub.val);
                Ok(WithStmts {
                    stmts: vec![],
                    val: stmts,
                    is_unsafe: sub.is_unsafe,
                })
            }
            CStmtKind::Break => Ok(WithStmts {
                stmts: vec![],
                val: vec![mk().expr_stmt(DaExpr::Break)],
                is_unsafe: false,
            }),
            CStmtKind::Continue => Ok(WithStmts {
                stmts: vec![],
                val: vec![mk().expr_stmt(DaExpr::Continue)],
                is_unsafe: false,
            }),
            CStmtKind::Empty => Ok(WithStmts {
                stmts: vec![],
                val: vec![],
                is_unsafe: false,
            }),
            CStmtKind::Asm {
                asm,
                inputs,
                outputs,
                clobbers,
                is_volatile,
            } => {
                self.convert_inline_assembly(stmt_id, asm, inputs, outputs, clobbers, *is_volatile)
            }
            CStmtKind::BadStmt => Err(TranslationError::generic("bad statement")),
            _ => Err(TranslationError::generic("unsupported statement kind")),
        }
    }

    /// Convert a C expression into a daScript expression
    pub fn convert_expr(
        &self,
        ctx: ExprContext,
        expr_id: CExprId,
        override_ty: Option<CQualTypeId>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let Located {
            loc: src_loc,
            kind: expr_kind,
        } = &self.ast_context[expr_id];

        // Macro expansion is represented by ordinary Clang expression nodes.
        // Give provenance ownership to macros.rs before normal AST lowering;
        // it deliberately returns None because re-parsing macro text would
        // duplicate side effects and violate C evaluation order.
        if self.expr_is_expanded_macro(ctx, expr_id, override_ty) {
            if let Some(lowered) = self.convert_const_macro_expansion(ctx, expr_id, override_ty)? {
                return Ok(lowered);
            }
        }

        use CExprKind::*;
        match expr_kind {
            Literal(ty, lit) => {
                // `char s[] = "abc"` initializes array storage, not a string
                // handle: C copies the bytes plus the terminating NUL.
                let literal_ty = override_ty.unwrap_or(*ty);
                if let CLiteral::String(bytes, width) = lit {
                    if let CTypeKind::ConstantArray(elem, size) =
                        self.ast_context.resolve_type(literal_ty.ctype).kind
                    {
                        return self
                            .string_literal_array(bytes, *width, elem, size)
                            .map(WithStmts::new_val);
                    }
                }
                self.convert_literal(literal_ty, lit).map(WithStmts::new_val)
            }

            Binary(ty, op, lhs, rhs, lty, rty) => {
                let value = self.convert_binary_expr(ctx, *ty, *op, *lhs, *rhs, *lty, *rty)?;
                self.lower_to_c_value(
                    value,
                    Some(*ty),
                    self.convert_type(*ty)?,
                    ValueSite::BinaryResult,
                )
            }

            ArraySubscript(ty, arr, idx, _lrvalue) => {
                let arr_val = self.convert_expr(ctx, *arr, None)?;
                let idx_val = self.convert_expr(ctx, *idx, None)?;
                // ArraySubscript — daScript requires Index on pointer/array to be
                // inside `unsafe()`. The C AST type check (is_pointer_type) sometimes
                // fails for nullable arrays; always wrapping is safe since
                // redundant unsafe(unsafe(...)) is harmless in daScript.
                let needs_unsafe = true;
                // A C subscript index is signed: `p[-1]` is a legal read of
                // the element before `p`.  Coercing it to `uint` would turn
                // that into a four-billion-element offset.
                let idx_expr = self.subscript_index_operand(idx_val.val);
                let arr_expr = if let Some(arr_ty) = self.ast_context[*arr].kind.get_qual_type() {
                    let target_type = self.convert_type(arr_ty)?;
                    if matches!(target_type.kind, DaTypeKind::Pointer(_))
                        && !matches!(arr_val.val, DaExpr::Unsafe(_))
                        && Self::infer_type(&arr_val.val)
                            .map_or(true, |inferred| writable_type(inferred) != target_type)
                    {
                        self.abi_pointer_cast(arr_val.val, target_type)
                    } else {
                        arr_val.val
                    }
                } else {
                    arr_val.val
                };
                let expr = DaExpr::Index(Box::new(arr_expr), Box::new(idx_expr));
                let expr = if needs_unsafe {
                    DaExpr::Unsafe(Box::new(expr))
                } else {
                    expr
                };
                let mut stmts = arr_val.stmts;
                stmts.extend(idx_val.stmts);
                Ok(WithStmts::new_val(expr)
                    .prepend_stmts(stmts)
                    .merge_unsafe(arr_val.is_unsafe || idx_val.is_unsafe))
            }

            Member(ty, expr, field_id, member_kind, _lrvalue) => {
                self.convert_member_expr(ctx, *ty, *expr, *field_id, *member_kind, override_ty)
            }

            DeclRef(_ty, decl_id, _lrvalue) => {
                let decl = &self.ast_context[*decl_id];
                let name = decl
                    .kind
                    .get_name()
                    .ok_or_else(|| TranslationError::generic("unnamed DeclRef"))?;
                let name = {
                    let existing = self.renamer.borrow().get(decl_id);
                    if let Some(existing) = existing {
                        existing
                    } else if let Some(alias) = self.function_context.borrow().get_param_alias(name)
                    {
                        alias
                    } else {
                        self.declare_value_name(*decl_id, name)
                    }
                };
                Ok(WithStmts::new_val(mk().ident(name)))
            }

            Call(ty, func_expr, args) => {
                // All call semantics, including libc ABI, live in the single
                // owning lowering path in translator/functions.rs. Preserve
                // the outer expected C type: runtime pointer results must be
                // materialized directly as that type at their raw ABI boundary.
                self.convert_function_call(ctx, *func_expr, args, *ty, override_ty)
            }

            ImplicitCast(ty, expr, cast_kind, _, _) => {
                if matches!(cast_kind, CastKind::NullToPointer) {
                    return Ok(WithStmts::new_val(self.null_for_type(*ty)?));
                }
                // C `_Bool b = x` is `x != 0`, never a numeric reinterpretation.
                // daScript has no conversion to bool at all, so the comparison
                // is the only correct lowering.
                if matches!(
                    cast_kind,
                    CastKind::IntegralToBoolean
                        | CastKind::FloatingToBoolean
                        | CastKind::PointerToBoolean
                ) {
                    return self.convert_to_boolean(ctx, *expr);
                }
                // C decays a function designator to a pointer wherever a value
                // is wanted. daScript has function values instead of function
                // pointers, and `@@name` is how one is taken.
                if matches!(cast_kind, CastKind::FunctionToPointerDecay) {
                    if let Some(name) = self.direct_function_reference(*expr) {
                        return Ok(WithStmts::new_val(DaExpr::FuncRef(name)));
                    }
                }
                if matches!(cast_kind, CastKind::BooleanToSignedIntegral) {
                    let target_type = self.convert_type(ty.clone())?;
                    let inner = self.convert_expr(ctx, *expr, None)?;
                    let tmp = self.renamer.borrow_mut().fresh();
                    let one = DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(DaExpr::ConstInt(1)),
                        to: target_type.clone(),
                    };
                    let mut stmts = inner.stmts;
                    stmts.extend([
                        DaStmt::Var {
                            name: tmp.clone(),
                            var_type: target_type.clone(),
                            init: Some(zero_for_datype(&target_type)),
                        },
                        DaStmt::Expr(DaExpr::IfThenElse {
                            cond: Box::new(inner.val),
                            then: Box::new(DaExpr::Block(DaBlock {
                                stmts: vec![DaStmt::Expr(DaExpr::Assign(
                                    Box::new(DaExpr::Var(tmp.clone())),
                                    Box::new(one),
                                ))],
                            })),
                            elifs: vec![],
                            else_: None,
                        }),
                    ]);
                    return Ok(
                        WithStmts::new(stmts, DaExpr::Var(tmp)).merge_unsafe(inner.is_unsafe)
                    );
                }
                // ToVoid, ConstCast, NoOp — transparent in C, but daScript may need
                // an explicit cast if the inferred types differ (e.g., int→uint for 0).
                if matches!(
                    cast_kind,
                    CastKind::ToVoid | CastKind::ConstCast | CastKind::NoOp | CastKind::Dependent
                ) {
                    let inner = self.convert_expr(ctx, *expr, Some(*ty))?;
                    let target_type = self.convert_type(ty.clone())?;
                    if self.ast_context[*expr]
                        .kind
                        .get_qual_type()
                        .map_or(false, |qty| {
                            type_kind_to_datype(&self.ast_context.resolve_type(qty.ctype).kind)
                                == target_type
                        })
                    {
                        return Ok(WithStmts::new_val(inner.val)
                            .prepend_stmts(inner.stmts)
                            .merge_unsafe(inner.is_unsafe));
                    }
                    let inner_ty = Translation::infer_type(&inner.val);
                    if inner_ty.map_or(false, |it| it != target_type) {
                        return Ok(WithStmts::new_val(DaExpr::Cast {
                            kind: das_ast::CastKind::Cast,
                            expr: Box::new(inner.val),
                            to: target_type,
                        })
                        .prepend_stmts(inner.stmts)
                        .merge_unsafe(inner.is_unsafe));
                    }
                    return Ok(WithStmts::new_val(inner.val)
                        .prepend_stmts(inner.stmts)
                        .merge_unsafe(inner.is_unsafe));
                }
                // IntegralCast (C integer promotion, e.g. uint16→int):
                // daScript не делает неявное продвижение — вставляем явный cast.
                if matches!(cast_kind, CastKind::IntegralCast) {
                    let inner = self.convert_expr(ctx, *expr, None)?;
                    let target_type = self.convert_type(ty.clone())?;
                    // Clang spells `_Bool` → integer as an ordinary integral
                    // cast; daScript has no conversion from bool, so it goes
                    // through the 0/1 materialization instead.
                    let source_ty = self.ast_context[*expr].kind.get_qual_type();
                    if source_ty.map_or(false, |q| {
                        matches!(self.ast_context.resolve_type(q.ctype).kind, CTypeKind::Bool)
                    }) {
                        return self.lower_to_c_value(
                            inner,
                            source_ty,
                            writable_type(target_type),
                            ValueSite::BinaryOperand,
                        );
                    }
                    let inner_unsafe = inner.is_unsafe;
                    let mut stmts = inner.stmts;
                    let inner_val = inner.val;
                    let cast = DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(inner_val.clone()),
                        to: target_type.clone(),
                    };
                    if let Some((lowered_stmts, lowered_val)) =
                        self.bool_to_integer_cast(cast.clone())
                    {
                        stmts.extend(lowered_stmts);
                        return Ok(WithStmts::new(stmts, lowered_val).merge_unsafe(inner_unsafe));
                    }
                    // Check if inner expr already has the target type (e.g., int→int identity)
                    // Check identity cast: if source and target daScript types match, skip.
                    let src_raw = self.ast_context[*expr].kind.get_qual_type();
                    let is_identity = src_raw.map_or(false, |qty| {
                        type_kind_to_datype(&self.ast_context.resolve_type(qty.ctype).kind)
                            == target_type
                    });
                    if is_identity {
                        return Ok(WithStmts::new(stmts, inner_val).merge_unsafe(inner_unsafe));
                    }
                    return Ok(WithStmts::new_val(cast)
                        .prepend_stmts(stmts)
                        .merge_unsafe(inner_unsafe));
                }
                if matches!(cast_kind, CastKind::ArrayToPointerDecay) {
                    // `p->array` is an aggregate C place, not a daScript
                    // array value.  C decay crosses directly from its Clang
                    // field address to the requested pointer type.
                    if let Some(place) = self.member_place_address(ctx, *expr)? {
                        let raw = self.raw_address_of_place(&place);
                        let target = self.convert_type(*ty)?;
                        // daScript refuses to write through a pointer built
                        // inline from address arithmetic (its dead-write
                        // policy), so the decayed pointer becomes a named
                        // value that both reads and writes can address.
                        let is_unsafe = raw.is_unsafe;
                        let pointer = raw.map(|raw| self.raw_address_to_pointer(raw, target.clone()));
                        let tmp = self.renamer.borrow_mut().fresh();
                        let mut stmts = pointer.stmts;
                        stmts.push(DaStmt::Var {
                            name: tmp.clone(),
                            var_type: writable_type(target),
                            init: Some(pointer.val),
                        });
                        return Ok(WithStmts::new(stmts, DaExpr::Var(tmp))
                            .merge_unsafe(is_unsafe || pointer.is_unsafe));
                    }
                    let inner = self.convert_expr(ctx, *expr, Some(*ty))?;
                    let idx = mk().int_lit(0);
                    return Ok(WithStmts::new_val(DaExpr::Unsafe(Box::new(DaExpr::Addr(
                        Box::new(DaExpr::Index(Box::new(inner.val), Box::new(idx))),
                    ))))
                    .prepend_stmts(inner.stmts)
                    .merge_unsafe(inner.is_unsafe));
                }
                // pointer ↔ integer / bitwise casts → reinterpret
                if matches!(
                    cast_kind,
                    CastKind::PointerToIntegral | CastKind::IntegralToPointer | CastKind::BitCast
                ) {
                    let inner = self.convert_expr(ctx, *expr, None)?;
                    let target_type = self.convert_type(ty.clone())?;
                    let cast = if matches!(cast_kind, CastKind::IntegralToPointer)
                        && matches!(target_type.kind, DaTypeKind::Pointer(_))
                    {
                        self.raw_address_to_pointer(inner.val, target_type)
                    } else if matches!(cast_kind, CastKind::PointerToIntegral)
                        && matches!(target_type.kind, DaTypeKind::UInt64)
                    {
                        self.pointer_to_raw_address(inner.val)
                    } else if matches!(target_type.kind, DaTypeKind::Pointer(_)) {
                        self.abi_pointer_cast(inner.val, target_type)
                    } else {
                        DaExpr::Unsafe(Box::new(DaExpr::Cast {
                            kind: das_ast::CastKind::Reinterpret,
                            expr: Box::new(inner.val),
                            to: target_type,
                        }))
                    };
                    return Ok(WithStmts::new_val(cast)
                        .prepend_stmts(inner.stmts)
                        .merge_unsafe(inner.is_unsafe));
                }
                // int↔float casts — generate explicit cast (mirrors c2rust convert_cast)
                if matches!(
                    cast_kind,
                    CastKind::IntegralToFloating
                        | CastKind::FloatingToIntegral
                        | CastKind::FloatingCast
                ) {
                    let inner = self.convert_expr(ctx, *expr, None)?;
                    let target_type = self.convert_type(ty.clone())?;
                    return Ok(WithStmts::new_val(DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(inner.val),
                        to: target_type,
                    })
                    .prepend_stmts(inner.stmts)
                    .merge_unsafe(inner.is_unsafe));
                }
                let inner = self.convert_expr(ctx, *expr, Some(*ty))?;
                Ok(WithStmts::new_val(inner.val)
                    .prepend_stmts(inner.stmts)
                    .merge_unsafe(inner.is_unsafe))
            }

            ExplicitCast(ty, expr, cast_kind, _, _) => {
                if matches!(
                    cast_kind,
                    CastKind::IntegralToBoolean
                        | CastKind::FloatingToBoolean
                        | CastKind::PointerToBoolean
                ) {
                    return self.convert_to_boolean(ctx, *expr);
                }
                let target_type = self.convert_type(ty.clone())?;
                let source_is_bool = self.ast_context[*expr]
                    .kind
                    .get_qual_type()
                    .map(|source_ty| {
                        matches!(
                            self.ast_context.resolve_type(source_ty.ctype).kind,
                            CTypeKind::Bool
                        )
                    })
                    .unwrap_or(false);
                // daScript has no direct bool -> integer cast.  Lower this C
                // conversion as explicit control flow before the printer sees
                // it, preserving C's 0/1 result for every numeric target.
                if source_is_bool
                    && target_type.is_numeric()
                    && !matches!(target_type.kind, DaTypeKind::Bool)
                {
                    let inner = self.convert_expr(ctx, *expr, None)?;
                    let tmp = self.renamer.borrow_mut().fresh();
                    let one = DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(DaExpr::ConstInt(1)),
                        to: target_type.clone(),
                    };
                    let mut stmts = inner.stmts;
                    stmts.extend([
                        DaStmt::Var {
                            name: tmp.clone(),
                            var_type: target_type.clone(),
                            init: Some(zero_for_datype(&target_type)),
                        },
                        DaStmt::Expr(DaExpr::IfThenElse {
                            cond: Box::new(inner.val),
                            then: Box::new(DaExpr::Block(DaBlock {
                                stmts: vec![DaStmt::Expr(DaExpr::Assign(
                                    Box::new(DaExpr::Var(tmp.clone())),
                                    Box::new(one),
                                ))],
                            })),
                            elifs: vec![],
                            else_: None,
                        }),
                    ]);
                    return Ok(
                        WithStmts::new(stmts, DaExpr::Var(tmp)).merge_unsafe(inner.is_unsafe)
                    );
                }
                let inner = self.convert_expr(ctx, *expr, Some(*ty))?;
                if matches!(cast_kind, CastKind::ToUnion) {
                    let union_id = match self.ast_context.resolve_type(ty.ctype).kind {
                        CTypeKind::Union(id) => id,
                        _ => {
                            return Err(TranslationError::generic(
                                "ToUnion cast has non-union target",
                            ))
                        }
                    };
                    let field = match &self.ast_context[union_id].kind {
                        CDeclKind::Union {
                            fields: Some(fields),
                            ..
                        } => fields.first().copied(),
                        _ => None,
                    };
                    return self.convert_cast_to_union(inner, field);
                }
                // ToVoid and ConstCast are no-ops in daScript too
                if matches!(
                    cast_kind,
                    CastKind::ToVoid | CastKind::ConstCast | CastKind::Dependent
                ) {
                    return Ok(WithStmts::new_val(inner.val)
                        .prepend_stmts(inner.stmts)
                        .merge_unsafe(inner.is_unsafe));
                }
                // If source and target are the same daScript type, skip the cast.
                // This avoids `can't cast uint8?& -const to uint8?` when a C pointer
                // variable/reference is assigned to the same pointer type.  The
                // comparison uses the full converted types: two C pointers with
                // different pointees are different daScript types and the cast
                // between them is load-bearing.
                if let Some(src_qual) = self.ast_context[*expr].kind.get_qual_type() {
                    let da_src = writable_type(self.convert_type(src_qual)?);
                    let da_tgt = writable_type(target_type.clone());
                    if da_src == da_tgt {
                        return Ok(WithStmts::new_val(inner.val)
                            .prepend_stmts(inner.stmts)
                            .merge_unsafe(inner.is_unsafe));
                    }
                }
                if matches!(target_type.kind, DaTypeKind::Pointer(_)) {
                    let cast = if matches!(cast_kind, CastKind::IntegralToPointer) {
                        self.raw_address_to_pointer(inner.val, target_type)
                    } else {
                        self.abi_pointer_cast(inner.val, target_type)
                    };
                    return Ok(WithStmts::new_val(cast)
                        .prepend_stmts(inner.stmts)
                        .merge_unsafe(inner.is_unsafe));
                }
                // Pointer/integer/bitwise casts use reinterpret<T>(x) in daScript
                let kind = if matches!(
                    cast_kind,
                    CastKind::BitCast | CastKind::IntegralToPointer | CastKind::PointerToIntegral
                ) {
                    das_ast::CastKind::Reinterpret
                } else {
                    das_ast::CastKind::Cast
                };
                Ok(WithStmts::new_val(DaExpr::Cast {
                    kind,
                    expr: Box::new(inner.val),
                    to: target_type,
                })
                .prepend_stmts(inner.stmts)
                .merge_unsafe(inner.is_unsafe))
            }

            ImplicitValueInit(ty) => {
                if let CTypeKind::Union(union_id) = self.ast_context.resolve_type(ty.ctype).kind {
                    return self.convert_union_literal(ctx, union_id, &[], override_ty);
                }
                let das_type = self.convert_type(*ty)?;
                Ok(WithStmts::new_val(zero_for_datype(&das_type)))
            }
            InitList(ty, ref init_ids, union_field, _syntactic) => {
                if let CTypeKind::Union(union_id) = self.ast_context.resolve_type(ty.ctype).kind {
                    let fields: Vec<CExprId> = init_ids.clone();
                    let value = self.convert_union_literal(ctx, union_id, &fields, override_ty)?;
                    return Ok(value);
                }
                if let Some(struct_init) = self.convert_struct_init_list(ctx, *ty, init_ids)? {
                    return Ok(struct_init);
                }
                let mut is_unsafe = false;
                let mut init_stmts = vec![];
                let mut items = vec![];
                let item_ty = self.init_list_item_type(*ty);
                let item_override = item_ty.unwrap_or(*ty);
                for &eid in init_ids {
                    let mut item = self.convert_expr(ctx.used(), eid, Some(item_override))?;
                    if let Some(elem_ty) = item_ty {
                        if is_zero_initializer_expr(&item.val) {
                            item.val = self.default_initializer_for_ctype(elem_ty.ctype)?;
                        }
                    }
                    is_unsafe |= item.is_unsafe;
                    init_stmts.extend(item.stmts);
                    items.push(item.val);
                }
                // Clang omits trailing aggregate members from InitList. C
                // nevertheless zero-initializes them, so restore the declared
                // ConstantArray extent in AST before daScript sees the value.
                if let CTypeKind::ConstantArray(elem_ty, size) =
                    &self.ast_context.resolve_type(ty.ctype).kind
                {
                    let size = *size;
                    let elem_ty = *elem_ty;
                    while items.len() < size {
                        items.push(self.default_initializer_for_ctype(elem_ty)?);
                    }
                    items.truncate(size);
                    let elem_da = writable_type(self.convert_type_raw(elem_ty)?);
                    return Ok(WithStmts::new_val(DaExpr::MakeFixedArray {
                        elem_type: elem_da,
                        items,
                    })
                    .prepend_stmts(init_stmts)
                    .merge_unsafe(is_unsafe));
                }
                Ok(WithStmts::new_val(DaExpr::MakeArray(items))
                    .prepend_stmts(init_stmts)
                    .merge_unsafe(is_unsafe))
            }
            // `sizeof`/`_Alignof` have type `size_t`, not `int`. Typing them
            // here is what lets `sizeof(int) * n` with an `unsigned long` n
            // keep C's uint64 arithmetic instead of narrowing n to int.
            UnaryType(ty, kind, _opt_expr, arg_ty) => {
                let result_type = self.convert_type(*ty).unwrap_or_else(|_| DaType::uint64());
                let value = match kind {
                    CUnTypeOp::SizeOf => self.sizeof_type(arg_ty.ctype)?,
                    CUnTypeOp::AlignOf => self.alignof_type(arg_ty.ctype)?,
                    _ => return Err(TranslationError::generic("unsupported unary type op")),
                };
                Ok(WithStmts::new_val(self.integer_literal_for_type(
                    DaExpr::ConstInt(value),
                    writable_type(result_type),
                )))
            }
            CompoundLiteral(ty, expr) => self.convert_expr(ctx, *expr, Some(*ty)),
            Predefined(_ty, expr) => self.convert_predefined_expression(ctx, *expr, override_ty),
            Paren(_ty, expr) => self.convert_expr(ctx, *expr, override_ty),

            Unary(_ty, op, expr, _) => {
                // Delegate to operatores.rs для всей логики, включая ++/--
                self.convert_unary_operator(ctx, *op, *_ty, *expr)
            }

            // GNU statement expression ({ stmts; expr }) → convert as daScript block
            Statements(_ty, stmt_id) => self.convert_gnu_statement_expression(ctx, *stmt_id),
            // offsetof → return 0 (daScript has no ABI-visible layout)
            OffsetOf(ty, kind) => {
                let target = self.convert_type(ty.clone())?;
                let value = match kind {
                    OffsetOfKind::Constant(value) => DaExpr::ConstInt(*value as i64),
                    OffsetOfKind::Variable(_, field, index) => {
                        let base = self.field_offset(*field)?;
                        let field_type = match self.ast_context[*field].kind {
                            CDeclKind::Field { typ, .. } => typ.ctype,
                            _ => {
                                return Err(TranslationError::generic(
                                    "offsetof references non-field",
                                ))
                            }
                        };
                        let stride = self.sizeof_type(field_type)?;
                        let index = self.convert_expr(ctx, *index, None)?;
                        return Ok(index.map(|index| DaExpr::Cast {
                            kind: das_ast::CastKind::Cast,
                            expr: Box::new(DaExpr::Op2 {
                                op: "+",
                                left: Box::new(DaExpr::ConstInt(base)),
                                right: Box::new(DaExpr::Op2 {
                                    op: "*",
                                    left: Box::new(index),
                                    right: Box::new(DaExpr::ConstInt(stride)),
                                }),
                            }),
                            to: target,
                        }));
                    }
                };
                Ok(WithStmts::new_val(DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(value),
                    to: target,
                }))
            }
            // va_arg → not supported
            VAArg(ty, expr) => self.convert_vaarg(ctx, *ty, *expr),
            // C11 atomic expressions → not supported
            Atomic { .. } => Err(TranslationError::generic(
                "C11 atomics not supported in daScript",
            )),
            ShuffleVector(ty, operands) => self.convert_shuffle_vector(expr_id, *ty, operands),
            ConvertVector(ty, operands) => self.convert_vector_conversion(expr_id, *ty, operands),
            // GNU choose expression
            Choose(_, _, _, _, _) => Err(TranslationError::generic(
                "GNU choose expression not supported",
            )),
            // Designated initializer expression (already expanded in C AST)
            DesignatedInitExpr(_, _, _) => Err(TranslationError::generic(
                "designated init expr not supported",
            )),
            // Ternary conditional — cond ? then : else
            // daScript не поддерживает if-then-else как выражение.
            // Разворачиваем в var _tmp; if (c) _tmp = a else _tmp = b; val = _tmp
            Conditional(ty, cond, then, else_) => {
                // C evaluates exactly one arm.  Every statement an arm needed
                // to hoist therefore has to move *inside* that arm's block,
                // not in front of the `if`.  The selector is a C truth test,
                // exactly like the one in `if`, so it goes through
                // `convert_condition` rather than being lowered as a value.
                let cond_e = self.convert_condition(ctx, true, *cond)?;
                let then_e = self.convert_expr(ctx.used(), *then, Some(*ty))?;
                let else_e = self.convert_expr(ctx.used(), *else_, Some(*ty))?;
                if then_e.is_pure() && else_e.is_pure() {
                    if let Some(minmax) =
                        lower_minmax_conditional(&cond_e.val, &then_e.val, &else_e.val)
                    {
                        return Ok(WithStmts {
                            stmts: cond_e.stmts,
                            val: minmax,
                            is_unsafe: cond_e.is_unsafe || then_e.is_unsafe || else_e.is_unsafe,
                        });
                    }
                }
                let tmp_type = writable_type(self.convert_type(*ty)?);
                let is_unsafe = cond_e.is_unsafe || then_e.is_unsafe || else_e.is_unsafe;
                let mut c_stmts = cond_e.stmts;
                let (tmp_var, decl_and_if) = self.guarded_value_branches(
                    &tmp_type,
                    cond_e.val,
                    then_e.map(|v| self.coerce_branch_value(v, &tmp_type)),
                    Some(else_e.map(|v| self.coerce_branch_value(v, &tmp_type))),
                );
                c_stmts.extend(decl_and_if);
                Ok(WithStmts {
                    stmts: c_stmts,
                    val: tmp_var,
                    is_unsafe,
                })
            }
            // GNU binary conditional — a ?: b evaluates `a` exactly once.
            BinaryConditional(ty, cond, else_) => {
                let cond_e = self.convert_expr(ctx.used(), *cond, Some(*ty))?;
                let else_e = self.convert_expr(ctx.used(), *else_, Some(*ty))?;
                let tmp_type = writable_type(self.convert_type(*ty)?);
                let is_unsafe = cond_e.is_unsafe || else_e.is_unsafe;
                let tmp = self.renamer.borrow_mut().fresh();
                let tmp_var = DaExpr::Var(tmp.clone());
                let mut c_stmts = cond_e.stmts;
                c_stmts.push(DaStmt::Var {
                    name: tmp.clone(),
                    var_type: tmp_type.clone(),
                    init: Some(self.coerce_branch_value(cond_e.val, &tmp_type)),
                });
                let mut else_stmts = else_e.stmts;
                else_stmts.push(DaStmt::Expr(DaExpr::Assign(
                    Box::new(tmp_var.clone()),
                    Box::new(self.coerce_branch_value(else_e.val, &tmp_type)),
                )));
                c_stmts.push(DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(mk().unary_op(
                        "!",
                        self.value_is_truthy(tmp_var.clone(), &tmp_type),
                    )),
                    then: Box::new(DaExpr::Block(DaBlock { stmts: else_stmts })),
                    elifs: vec![],
                    else_: None,
                }));
                Ok(WithStmts {
                    stmts: c_stmts,
                    val: tmp_var,
                    is_unsafe,
                })
            }
            // Bad expression — skip
            BadExpr => Err(TranslationError::generic("bad/invalid expression")),
            ConstantExpr(ty, child, _value) => self.convert_expr(ctx, *child, Some(*ty)),
            _ => Err(TranslationError::generic(
                "expr kind not yet implemented in daScript translator (catch-all)",
            )),
        }
    }

    /// A C subscript index is a signed integer, and daScript indexes pointers
    /// and fixed arrays with `int`/`int64`.
    pub(crate) fn subscript_index_operand(&self, index: DaExpr) -> DaExpr {
        match Self::infer_type(&index) {
            Some(ty) if matches!(ty.kind, DaTypeKind::Int | DaTypeKind::Int64) => index,
            Some(ty) if matches!(ty.kind, DaTypeKind::UInt64) => DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(index),
                to: DaType::int64(),
            },
            _ => DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(index),
                to: DaType::int(),
            },
        }
    }

    /// A C string literal used as an array initializer is array storage:
    /// every code unit plus the NUL terminator, zero-padded to the declared
    /// extent.
    fn string_literal_array(
        &self,
        bytes: &[u8],
        width: u8,
        elem: CTypeId,
        size: usize,
    ) -> TranslationResult<DaExpr> {
        if width != 1 {
            return Err(TranslationError::generic(
                "wide string array initializer is not implemented",
            ));
        }
        let elem_da = writable_type(self.convert_type_raw(elem)?);
        if bytes.len() >= size + 1 {
            return Err(TranslationError::generic(
                "C string initializer is longer than its array",
            ));
        }
        let signed_elem = matches!(
            elem_da.kind,
            DaTypeKind::Int8 | DaTypeKind::Int16 | DaTypeKind::Int | DaTypeKind::Int64
        );
        let mut items = Vec::with_capacity(size);
        for index in 0..size {
            let byte = bytes.get(index).copied().unwrap_or(0);
            let value = if signed_elem {
                i64::from(byte as i8)
            } else {
                i64::from(byte)
            };
            items.push(
                self.integer_literal_for_type(DaExpr::ConstInt(value), elem_da.clone()),
            );
        }
        Ok(DaExpr::MakeFixedArray {
            elem_type: elem_da,
            items,
        })
    }

    /// Lower a C conversion to `_Bool`.  C says the result is `x != 0`
    /// (`x != null` for a pointer); daScript has no conversion to bool, so the
    /// comparison is not an approximation but the definition.
    fn convert_to_boolean(
        &self,
        ctx: ExprContext,
        expr_id: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        if let Some(qty) = self.ast_context[expr_id].kind.get_qual_type() {
            let kind = self.ast_context.resolve_type(qty.ctype).kind.clone();
            if matches!(
                kind,
                CTypeKind::Float
                    | CTypeKind::Double
                    | CTypeKind::LongDouble
                    | CTypeKind::Float128
                    | CTypeKind::BFloat16
            ) {
                let value = self.convert_expr(ctx.used(), expr_id, Some(qty))?;
                let da = writable_type(self.convert_type(qty)?);
                return Ok(value.map(|v| DaExpr::Op2 {
                    op: "!=",
                    left: Box::new(v),
                    right: Box::new(zero_for_datype(&da)),
                }));
            }
        }
        self.convert_condition(ctx, true, expr_id)
    }

    /// A daScript condition slot only accepts a boolean *expression*; a bare
    /// `bool` value has to be spelled as one.
    pub(crate) fn as_bool_condition(&self, value: DaExpr) -> DaExpr {
        fn is_condition(expr: &DaExpr) -> bool {
            match expr {
                DaExpr::ConstBool(_) | DaExpr::Op1 { op: "!", .. } => true,
                DaExpr::Op2 { op, .. } => matches!(
                    *op,
                    "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||"
                ),
                DaExpr::Unsafe(inner) => is_condition(inner),
                _ => false,
            }
        }
        if is_condition(&value) {
            value
        } else {
            DaExpr::Op2 {
                op: "==",
                left: Box::new(value),
                right: Box::new(DaExpr::ConstBool(true)),
            }
        }
    }

    /// `value != 0` for the daScript type a C value is stored in.
    pub(crate) fn value_is_truthy(&self, value: DaExpr, ty: &DaType) -> DaExpr {
        let zero = match ty.kind {
            DaTypeKind::Pointer(_) => abi::null_pointer(ty),
            DaTypeKind::Bool => return self.as_bool_condition(value),
            _ => zero_for_datype(ty),
        };
        DaExpr::Op2 {
            op: "!=",
            left: Box::new(value),
            right: Box::new(zero),
        }
    }

    /// Make an arm value assignable to a temporary of `target`.
    fn coerce_branch_value(&self, value: DaExpr, target: &DaType) -> DaExpr {
        if matches!(target.kind, DaTypeKind::Pointer(_)) {
            if matches!(value, DaExpr::ConstNull) {
                return value;
            }
            return match Self::infer_type(&value) {
                Some(ref inferred) if inferred == target => value,
                _ => value,
            };
        }
        if !target.is_numeric() || matches!(target.kind, DaTypeKind::Auto) {
            return value;
        }
        match Self::infer_type(&value) {
            Some(inferred) => {
                if writable_type(inferred) == *target {
                    value
                } else {
                    DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(value),
                        to: target.clone(),
                    }
                }
            }
            None => value,
        }
    }

    /// `var tmp : T = zero; if (cond) { then-stmts; tmp = a } else { … }`.
    ///
    /// This is the shape every C construct that evaluates one of two operands
    /// lowers to, so the statements an operand hoisted stay guarded by the
    /// condition that decides whether C evaluates it at all.
    fn guarded_value_branches(
        &self,
        tmp_type: &DaType,
        cond: DaExpr,
        then_value: WithStmts<DaExpr>,
        else_value: Option<WithStmts<DaExpr>>,
    ) -> (DaExpr, Vec<DaStmt>) {
        let tmp = self.renamer.borrow_mut().fresh();
        let tmp_var = DaExpr::Var(tmp.clone());
        let mut then_stmts = then_value.stmts;
        then_stmts.push(DaStmt::Expr(DaExpr::Assign(
            Box::new(tmp_var.clone()),
            Box::new(then_value.val),
        )));
        let else_block = else_value.map(|else_value| {
            let mut else_stmts = else_value.stmts;
            else_stmts.push(DaStmt::Expr(DaExpr::Assign(
                Box::new(tmp_var.clone()),
                Box::new(else_value.val),
            )));
            Box::new(DaExpr::Block(DaBlock { stmts: else_stmts }))
        });
        let stmts = vec![
            DaStmt::Var {
                name: tmp,
                var_type: tmp_type.clone(),
                init: Some(zero_for_datype(tmp_type)),
            },
            DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(cond),
                then: Box::new(DaExpr::Block(DaBlock { stmts: then_stmts })),
                elifs: vec![],
                else_: else_block,
            }),
        ];
        (tmp_var, stmts)
    }

    /// A C expression whose value is discarded still has to run.  Returns the
    /// statement that keeps its side effects, or `None` when the daScript
    /// expression provably has none (daScript rejects a pure expression
    /// statement).
    pub(crate) fn discard_value_stmt(&self, expr: DaExpr) -> Option<DaStmt> {
        fn has_effect(expr: &DaExpr) -> bool {
            match expr {
                DaExpr::ConstInt(_)
                | DaExpr::ConstUInt(_)
                | DaExpr::ConstFloat(_)
                | DaExpr::ConstDouble(_)
                | DaExpr::ConstBool(_)
                | DaExpr::ConstString(_)
                | DaExpr::ConstNull
                | DaExpr::Var(_) => false,
                DaExpr::Field(base, _) | DaExpr::SafeField(base, _) => has_effect(base),
                DaExpr::Index(base, idx) | DaExpr::SafeIndex(base, idx) => {
                    has_effect(base) || has_effect(idx)
                }
                DaExpr::Op1 { expr, .. } => has_effect(expr),
                DaExpr::Op2 { left, right, .. } => has_effect(left) || has_effect(right),
                DaExpr::Cast { expr, .. }
                | DaExpr::Unsafe(expr)
                | DaExpr::Addr(expr)
                | DaExpr::Deref(expr)
                | DaExpr::DerefExplicit(expr) => has_effect(expr),
                _ => true,
            }
        }
        has_effect(&expr).then(|| DaStmt::Expr(expr))
    }

    fn collect_case_values(
        &self,
        first_expr: CExprId,
        sub_stmt: CStmtId,
    ) -> TranslationResult<(Vec<CExprId>, CStmtId)> {
        match &self.ast_context[sub_stmt].kind {
            CStmtKind::Case(expr_id, nested_sub, _) => {
                // Nested fallthrough: case 1: case 2: { body } → first_expr=1, sub_stmt=Case(2, body)
                let (mut rest_vals, body) = self.collect_case_values(*expr_id, *nested_sub)?;
                let mut all = vec![first_expr];
                all.extend(rest_vals);
                Ok((all, body))
            }
            CStmtKind::Default(_) => {
                // `case 1: default: { body }` — the body is both a case arm and
                // the default arm.  The if/elif chain this path builds has one
                // `else`, so keeping only the case value would silently drop the
                // default.
                Err(TranslationError::generic(
                    "unsupported switch shape in a statement expression: \
                     a default label sharing a case label's body",
                ))
            }
            _ => {
                // Regular case with body: case 1: { body }
                Ok((vec![first_expr], sub_stmt))
            }
        }
    }

    /// True when control can run off the end of this C statement instead of
    /// leaving it through `break`, `return`, `goto` or `continue`.
    fn statement_falls_through(&self, stmt_id: CStmtId) -> bool {
        match &self.ast_context[stmt_id].kind {
            CStmtKind::Break
            | CStmtKind::Continue
            | CStmtKind::Return(_)
            | CStmtKind::Goto(_) => false,
            CStmtKind::Compound(ref stmts) => stmts
                .last()
                .map_or(true, |&last| self.statement_falls_through(last)),
            CStmtKind::If {
                true_variant,
                false_variant: Some(false_variant),
                ..
            } => {
                self.statement_falls_through(*true_variant)
                    || self.statement_falls_through(*false_variant)
            }
            CStmtKind::Label(sub) | CStmtKind::Case(_, sub, _) | CStmtKind::Default(sub) => {
                self.statement_falls_through(*sub)
            }
            _ => true,
        }
    }

    /// Walk a switch body compound statement, extracting Case/Default branches.
    /// Returns (cases, is_unsafe).
    ///
    /// This is the statement-expression path only; a `switch` in a function body
    /// goes through the CFG, which models dispatch and fall-through exactly.
    /// Here the switch becomes an if/elif chain, which cannot express C's
    /// fall-through, so anything that would need it is rejected rather than
    /// silently translated into a different program.
    fn collect_switch_cases(&self, body_id: CStmtId) -> TranslationResult<(Vec<SwitchCase>, bool)> {
        // First pass: collect raw cases with their values and body substatements
        struct RawCase {
            values: Vec<CExprId>,
            body_sub: CStmtId,
        }
        let mut raw: Vec<RawCase> = vec![];
        let body = &self.ast_context[body_id];
        match &body.kind {
            CStmtKind::Compound(ref stmts) => {
                for &sid in stmts {
                    match &self.ast_context[sid].kind {
                        CStmtKind::Case(expr_id, sub_stmt, _) => {
                            let (vals, body) = self.collect_case_values(*expr_id, *sub_stmt)?;
                            raw.push(RawCase {
                                values: vals,
                                body_sub: body,
                            });
                        }
                        CStmtKind::Default(sub_stmt) => {
                            raw.push(RawCase {
                                values: vec![],
                                body_sub: *sub_stmt,
                            });
                        }
                        // C allows statements before the first `case`; they are
                        // unreachable but a Duff's-device body puts real code
                        // here too. Neither survives an if/elif chain.
                        _ => {
                            return Err(TranslationError::generic(
                                "unsupported switch shape in a statement expression: \
                                 a statement outside every case label",
                            ))
                        }
                    }
                }
            }
            CStmtKind::Case(expr_id, sub_stmt, _) => {
                let (vals, body) = self.collect_case_values(*expr_id, *sub_stmt)?;
                raw.push(RawCase {
                    values: vals,
                    body_sub: body,
                });
            }
            CStmtKind::Default(sub_stmt) => {
                raw.push(RawCase {
                    values: vec![],
                    body_sub: *sub_stmt,
                });
            }
            _ => return Err(TranslationError::generic("switch body not case/compound")),
        }

        // Second pass: merge fallthrough cases (consecutive cases where the first has empty body)
        let mut cases: Vec<SwitchCase> = vec![];
        let mut pending_values: Vec<DaExpr> = vec![];
        let mut is_unsafe = false;

        let last_index = raw.len().saturating_sub(1);
        for (index, rc) in raw.iter().enumerate() {
            // Only the final arm may run off the end of the switch; anywhere
            // else that is C fall-through into the next case.
            if index != last_index
                && !matches!(self.ast_context[rc.body_sub].kind, CStmtKind::Empty)
                && self.statement_falls_through(rc.body_sub)
            {
                return Err(TranslationError::generic(
                    "unsupported switch shape in a statement expression: \
                     a case falls through into the next one",
                ));
            }
            // Convert values
            let mut vals: Vec<DaExpr> = vec![];
            for &ev in &rc.values {
                let val = self.convert_expr(
                    ExprContext {
                        used: true,
                        is_const: false,
                        ..Default::default()
                    },
                    ev,
                    None,
                )?;
                is_unsafe |= val.is_unsafe;
                vals.push(val.val);
            }

            // Check body
            let mut body_stmts = vec![];
            let body_unsafe = self.collect_case_body(rc.body_sub, &mut body_stmts)?;
            is_unsafe |= body_unsafe;

            // `case 1: ;` shares the next arm's body. An arm whose statements
            // merely lowered to nothing (`case 1: break;`) does not — merging on
            // emptiness used to give it the following arm's body.
            if matches!(self.ast_context[rc.body_sub].kind, CStmtKind::Empty) && !vals.is_empty() {
                pending_values.extend(vals);
            } else {
                let mut merged_vals = std::mem::take(&mut pending_values);
                merged_vals.extend(vals);
                cases.push(SwitchCase {
                    values: merged_vals,
                    stmts: body_stmts,
                });
            }
        }
        if !pending_values.is_empty() {
            cases.push(SwitchCase {
                values: pending_values,
                stmts: vec![],
            });
        }
        Ok((cases, is_unsafe))
    }

    /// Recursively collect case body statements, skipping breaks.
    /// Returns `true` if any statement in the body contains unsafe operations.
    fn collect_case_body(
        &self,
        stmt_id: CStmtId,
        stmts: &mut Vec<DaStmt>,
    ) -> TranslationResult<bool> {
        let mut is_unsafe = false;
        match &self.ast_context[stmt_id].kind {
            CStmtKind::Compound(ref children) => {
                for &sid in children {
                    is_unsafe |= self.collect_case_body(sid, stmts)?;
                }
            }
            CStmtKind::Break => { /* skip */ }
            CStmtKind::Return(expr) => {
                let val = expr
                    .map(|e| {
                        self.convert_expr(
                            ExprContext {
                                used: true,
                                is_const: false,
                                ..Default::default()
                            },
                            e,
                            None,
                        )
                    })
                    .transpose()?;
                let ret_ty = self.function_context.borrow().get_return_type();
                let val = match (val, ret_ty) {
                    (Some(ws), Some(ret_ty)) => Some(self.lower_to_c_value(
                        ws,
                        expr.and_then(|e| self.ast_context[e].kind.get_qual_type()),
                        self.convert_type(ret_ty)?,
                        ValueSite::Return,
                    )?),
                    (value, _) => value,
                };
                is_unsafe |= val.as_ref().map(|v| v.is_unsafe).unwrap_or(false);
                if let Some(ref ws) = val {
                    stmts.extend(ws.stmts.clone());
                }
                stmts.push(mk().expr_stmt(DaExpr::Return(val.map(|ws| Box::new(ws.val)))));
            }
            CStmtKind::Expr(expr_id) => {
                let v = self.convert_expr(
                    ExprContext {
                        used: true,
                        is_const: false,
                        ..Default::default()
                    },
                    *expr_id,
                    None,
                )?;
                is_unsafe |= v.is_unsafe;
                stmts.extend(v.stmts);
                stmts.push(mk().expr_stmt(v.val));
            }
            CStmtKind::If {
                scrutinee,
                true_variant,
                false_variant,
            } => {
                let cond = self.convert_condition(
                    ExprContext {
                        used: true,
                        is_const: false,
                        ..Default::default()
                    },
                    true,
                    *scrutinee,
                )?;
                let mut then_stmts = vec![];
                let then_unsafe = self.collect_case_body(*true_variant, &mut then_stmts)?;
                let then_expr = DaExpr::Block(DaBlock { stmts: then_stmts });
                let (else_expr, else_unsafe) = match false_variant {
                    Some(fv) => {
                        let mut else_stmts = vec![];
                        let eu = self.collect_case_body(*fv, &mut else_stmts)?;
                        (
                            Some(Box::new(DaExpr::Block(DaBlock { stmts: else_stmts }))),
                            eu,
                        )
                    }
                    None => (None, false),
                };
                is_unsafe |= cond.is_unsafe || then_unsafe || else_unsafe;
                stmts.push(mk().expr_stmt(DaExpr::IfThenElse {
                    cond: Box::new(cond.val),
                    then: Box::new(then_expr),
                    elifs: vec![],
                    else_: else_expr,
                }));
            }
            CStmtKind::While { condition, body } => {
                let cond = self.convert_condition(
                    ExprContext {
                        used: true,
                        is_const: false,
                        ..Default::default()
                    },
                    true,
                    *condition,
                )?;
                let mut body_stmts = vec![];
                let body_unsafe = self.collect_case_body(*body, &mut body_stmts)?;
                is_unsafe |= cond.is_unsafe || body_unsafe;
                stmts.push(mk().expr_stmt(DaExpr::While(
                    Box::new(cond.val),
                    Box::new(DaExpr::Block(DaBlock { stmts: body_stmts })),
                )));
            }
            CStmtKind::Label(_) | CStmtKind::Goto(_) => {
                let sub = self.convert_stmt(stmt_id)?;
                is_unsafe |= sub.is_unsafe;
                stmts.extend(sub.val);
            }
            _ => {
                let sub = self.convert_stmt(stmt_id)?;
                is_unsafe |= sub.is_unsafe;
                stmts.extend(sub.val);
            }
        }
        Ok(is_unsafe)
    }

    /// Build if/elif/else chain from collected switch cases.
    fn build_switch_chain(&self, scrutinee: &DaExpr, cases: &[SwitchCase]) -> DaExpr {
        if cases.is_empty() {
            return DaExpr::Block(DaBlock { stmts: vec![] });
        }
        // Collect all elifs and the final else
        let mut elifs = vec![];
        let mut final_else = None;
        for case in cases {
            let body = DaExpr::Block(DaBlock {
                stmts: case.stmts.clone(),
            });
            if case.values.is_empty() {
                final_else = Some(body); // default → else
            } else {
                let cond = self.build_switch_cond(scrutinee, &case.values);
                elifs.push((cond, body));
            }
        }
        // First case becomes the if, rest become elifs
        if elifs.is_empty() {
            return final_else.unwrap_or(DaExpr::Block(DaBlock { stmts: vec![] }));
        }
        let first = elifs.remove(0);
        DaExpr::IfThenElse {
            cond: Box::new(first.0),
            then: Box::new(first.1),
            elifs,
            else_: final_else.map(Box::new),
        }
    }

    fn build_switch_arm<'a>(
        &self,
        scrutinee: &DaExpr,
        case: &SwitchCase,
        rest: &mut impl Iterator<Item = &'a SwitchCase>,
    ) -> DaExpr {
        let body = DaExpr::Block(DaBlock {
            stmts: case.stmts.clone(),
        });
        if let Some(next) = rest.next() {
            let else_arm = self.build_switch_arm(scrutinee, next, rest);
            if case.values.is_empty() {
                DaExpr::IfThenElse {
                    cond: Box::new(DaExpr::ConstBool(true)),
                    then: Box::new(body),
                    elifs: vec![],
                    else_: Some(Box::new(else_arm)),
                }
            } else {
                let cond = self.build_switch_cond(scrutinee, &case.values);
                DaExpr::IfThenElse {
                    cond: Box::new(cond),
                    then: Box::new(body),
                    elifs: vec![],
                    else_: Some(Box::new(else_arm)),
                }
            }
        } else {
            if case.values.is_empty() {
                body
            } else {
                let cond = self.build_switch_cond(scrutinee, &case.values);
                DaExpr::IfThenElse {
                    cond: Box::new(cond),
                    then: Box::new(body),
                    elifs: vec![],
                    else_: None,
                }
            }
        }
    }

    fn build_switch_cond(&self, scrutinee: &DaExpr, values: &[DaExpr]) -> DaExpr {
        if values.is_empty() {
            return DaExpr::ConstBool(true);
        }
        let mut cond = DaExpr::Op2 {
            op: "==",
            left: Box::new(scrutinee.clone()),
            right: Box::new(values[0].clone()),
        };
        for v in &values[1..] {
            cond = DaExpr::Op2 {
                op: "||",
                left: Box::new(cond),
                right: Box::new(DaExpr::Op2 {
                    op: "==",
                    left: Box::new(scrutinee.clone()),
                    right: Box::new(v.clone()),
                }),
            };
        }
        cond
    }

    /// Convert a C condition expression to a daScript boolean expression.
    pub fn convert_condition(
        &self,
        ctx: ExprContext,
        _used: bool,
        expr_id: CExprId,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let expr_ty = self.ast_context[expr_id].kind.get_qual_type();
        let val = self.convert_expr(ctx.used(), expr_id, expr_ty)?;
        if Self::infer_type(&val.val).map_or(false, |ty| matches!(ty.kind, DaTypeKind::Bool)) {
            return self.normalize_condition_comparison(expr_id, val);
        }
        let Some(qty) = expr_ty else {
            return Ok(val);
        };
        let ty = self.ast_context.resolve_type(qty.ctype);
        if matches!(ty.kind, CTypeKind::Bool) {
            return Ok(val);
        }
        if self.is_pointer_type(qty.ctype) {
            let null = self.null_for_type(qty)?;
            return Ok(val.map(|v| DaExpr::Op2 {
                op: "!=",
                left: Box::new(v),
                right: Box::new(null),
            }));
        }
        if ty.kind.is_integral_type() {
            // If the expression is already boolean (Op2 comparison), skip adding `!= 0`.
            // Our !ptr fix generates `ptr == null` which is bool, but C type is `int`.
            if matches!(
                val.val,
                DaExpr::Op2 {
                    op: "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||",
                    ..
                }
            ) {
                return self.normalize_condition_comparison(expr_id, val);
            }
            return Ok(val.map(|v| DaExpr::Op2 {
                op: "!=",
                left: Box::new(v),
                right: Box::new(zero_for_ctype_kind(&ty.kind)),
            }));
        }
        if ty.kind.is_floating_type() {
            // A floating condition is compared against a floating zero in its
            // own type. Routing it through an integer comparison would make
            // every value with magnitude below one — `0.5`, `-0.5` — test as
            // false.
            let zero = literals::floating_zero_for_datype(&self.convert_type(qty)?);
            return Ok(val.map(|v| DaExpr::Op2 {
                op: "!=",
                left: Box::new(v),
                right: Box::new(zero),
            }));
        }
        if let Some(inferred) = Self::infer_type(&val.val) {
            if inferred.is_numeric() && !matches!(inferred.kind, DaTypeKind::Bool) {
                return Ok(val.map(|v| DaExpr::Op2 {
                    op: "!=",
                    left: Box::new(v),
                    right: Box::new(zero_for_datype(&inferred)),
                }));
            }
        }
        Ok(val)
    }

    fn normalize_condition_comparison(
        &self,
        expr_id: CExprId,
        val: WithStmts<DaExpr>,
    ) -> TranslationResult<WithStmts<DaExpr>> {
        let CExprKind::Binary(_, op, lhs_id, rhs_id, _, _) = &self.ast_context[expr_id].kind else {
            return Ok(val);
        };
        if !matches!(
            op,
            CBinOp::EqualEqual
                | CBinOp::NotEqual
                | CBinOp::Less
                | CBinOp::Greater
                | CBinOp::LessEqual
                | CBinOp::GreaterEqual
        ) {
            return Ok(val);
        }

        let Some(lhs_ty) = self.ast_context[*lhs_id].kind.get_qual_type() else {
            return Ok(val);
        };
        let Some(rhs_ty) = self.ast_context[*rhs_id].kind.get_qual_type() else {
            return Ok(val);
        };
        let lhs_da = writable_type(self.convert_type(lhs_ty)?);
        let rhs_da = writable_type(self.convert_type(rhs_ty)?);
        if lhs_da == rhs_da {
            return Ok(val);
        }

        Ok(val.map(|expr| match expr {
            DaExpr::Op2 { op, left, right } => DaExpr::Op2 {
                op,
                left,
                right: Box::new(DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: right,
                    to: lhs_da,
                }),
            },
            expr => expr,
        }))
    }

    pub fn null_for_type(&self, ty: CQualTypeId) -> TranslationResult<DaExpr> {
        let da_type = self.convert_type(ty)?;
        // A daScript `function<…>` does not accept `null`; its null value is
        // spelled `default<T>` (and compares equal to `null`).  The check uses
        // the C type because a typedef can hide the function type behind a name.
        if self.is_callable_type(ty.ctype) {
            return Ok(DaExpr::DefaultValue(da_type));
        }
        if matches!(da_type.kind, DaTypeKind::UInt64) {
            Ok(DaExpr::Cast {
                kind: das_ast::CastKind::Cast,
                expr: Box::new(DaExpr::ConstUInt(0)),
                to: DaType::uint64(),
            })
        } else {
            Ok(self.null_pointer(&da_type))
        }
    }

    fn has_decl_reference(&self, decl_id: CDeclId, expr_id: CExprId) -> bool {
        let mut iter = DFExpr::new(&self.ast_context, expr_id.into());
        while let Some(x) = iter.next() {
            match x {
                SomeId::Expr(e) => match self.ast_context[e].kind {
                    CExprKind::DeclRef(_, d, _) if d == decl_id => return true,
                    CExprKind::UnaryType(_, _, Some(_), _) => iter.prune(1),
                    _ => {}
                },
                SomeId::Type(t) => {
                    if let CTypeKind::TypeOfExpr(_) = self.ast_context[t].kind {
                        iter.prune(1);
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// Create DeclStmtInfo for a C declaration.
    pub fn convert_decl_stmt_info(
        &self,
        ctx: ExprContext,
        decl_id: CDeclId,
    ) -> TranslationResult<crate::cfg::DeclStmtInfo> {
        match self.ast_context[decl_id].kind {
            // A function-scope `static` is not a local at all: it has the
            // lifetime of the program and is initialised exactly once, before
            // `main` (C 6.2.4p3, 6.7.9p4 — its initialiser is a constant
            // expression, and it is zero-initialised without one).  Emitting it
            // as a `var` inside the body reset it on every call.  Hoist it to a
            // module-level variable under a name mangled with the owning
            // function, so two functions that both declare `static int count`
            // still get two distinct objects.
            CDeclKind::Variable {
                has_static_duration: true,
                has_thread_duration: false,
                ref ident,
                initializer,
                typ,
                ..
            } => {
                let mangled = format!("{}__{}", self.function_context.borrow().get_name(), ident);
                let name = self.declare_value_name(decl_id, &mangled);
                let var_type = self.convert_type(typ)?;
                let init = match initializer {
                    Some(expr_id) => {
                        let init_ws = self.convert_expr(
                            ExprContext {
                                used: true,
                                is_const: true,
                                ..Default::default()
                            },
                            expr_id,
                            Some(typ),
                        )?;
                        if !init_ws.stmts.is_empty() {
                            return Err(TranslationError::generic(
                                "static local initializer is not a constant expression",
                            ));
                        }
                        crate::translator::functions::normalize_array_initializer_for_type(
                            init_ws.val,
                            &var_type,
                        )
                    }
                    None => self.default_initializer_for_ctype(typ.ctype)?,
                };
                self.hoisted_statics
                    .borrow_mut()
                    .push(DaDecl::Variable(DaVariable {
                        name,
                        var_type,
                        init: Some(init),
                        annotations: vec![],
                    }));
                // Nothing is emitted in the function body: the declaration site
                // carries no runtime effect any more.
                Ok(crate::cfg::DeclStmtInfo::empty())
            }
            CDeclKind::Variable {
                has_static_duration: false,
                has_thread_duration: false,
                is_externally_visible: false,
                is_defn,
                ref ident,
                initializer,
                typ,
                ..
            } => {
                assert!(
                    is_defn,
                    "Only local variable definitions should be extracted"
                );

                let rust_name = self.declare_value_name(decl_id, ident);
                if self.function_context.borrow().va_list_arg_name.is_some()
                    && self.ast_context.is_va_list(typ.ctype)
                {
                    let decl_stmt = DaStmt::Var {
                        name: rust_name,
                        var_type: self.va_cursor_type(),
                        init: Some(self.va_cursor_initializer()),
                    };
                    return Ok(crate::cfg::DeclStmtInfo::new(
                        vec![decl_stmt.clone()],
                        vec![],
                        vec![decl_stmt],
                    ));
                }
                let var_type = self.convert_type(typ)?;
                let default_init = self.default_initializer_for_ctype(typ.ctype)?;
                let decl_stmt = DaStmt::Var {
                    name: rust_name.clone(),
                    var_type: var_type.clone(),
                    init: Some(default_init),
                };

                let has_self_reference = initializer
                    .map(|expr_id| self.has_decl_reference(decl_id, expr_id))
                    .unwrap_or(false);

                let init_ws = initializer
                    .map(|expr_id| self.convert_expr(ctx.used(), expr_id, Some(typ)))
                    .transpose()?;

                match init_ws {
                    None => Ok(crate::cfg::DeclStmtInfo::new(
                        vec![decl_stmt.clone()],
                        vec![],
                        vec![decl_stmt],
                    )),
                    Some(mut init_ws) => {
                        let init_expr =
                            crate::translator::functions::normalize_array_initializer_for_type(
                                init_ws.val,
                                &var_type,
                            );
                        let assign_expr = DaExpr::Assign(
                            Box::new(DaExpr::Var(rust_name.clone())),
                            Box::new(init_expr.clone()),
                        );

                        let mut assign_stmts = init_ws.stmts.clone();
                        assign_stmts.push(DaStmt::Expr(assign_expr.clone()));

                        if has_self_reference {
                            let mut decl_and_assign = vec![decl_stmt.clone()];
                            decl_and_assign.append(&mut init_ws.stmts);
                            decl_and_assign.push(DaStmt::Expr(assign_expr));
                            Ok(crate::cfg::DeclStmtInfo::new(
                                vec![decl_stmt],
                                assign_stmts,
                                decl_and_assign,
                            ))
                        } else {
                            let mut decl_and_assign = init_ws.stmts;
                            decl_and_assign.push(DaStmt::Var {
                                name: rust_name,
                                var_type,
                                init: Some(init_expr),
                            });
                            Ok(crate::cfg::DeclStmtInfo::new(
                                vec![decl_stmt],
                                assign_stmts,
                                decl_and_assign,
                            ))
                        }
                    }
                }
            }
            ref decl => {
                let inserted = if let Some(ident) = decl.get_name() {
                    self.renamer.borrow_mut().insert(decl_id, ident).is_some()
                } else {
                    false
                };

                use CDeclKind::*;
                let skip = match decl {
                    Variable { .. } => !inserted,
                    Struct { .. } | Union { .. } | Enum { .. } | Typedef { .. } => true,
                    _ => false,
                };

                if skip {
                    Ok(crate::cfg::DeclStmtInfo::new(vec![], vec![], vec![]))
                } else {
                    let decl_stmt = DaStmt::Decl(self.convert_decl(ctx, decl_id)?);
                    Ok(crate::cfg::DeclStmtInfo::new(
                        vec![decl_stmt.clone()],
                        vec![],
                        vec![decl_stmt],
                    ))
                }
            }
        }
    }

    /// Execute a closure with a new scope.
    pub fn with_scope<T, F: FnOnce() -> TranslationResult<T>>(&self, f: F) -> TranslationResult<T> {
        f()
    }

    /// Panic with an error message for unreachable code.
    pub fn panic(&self, msg: &str) -> Box<DaExpr> {
        Box::new(DaExpr::ConstInt(0))
    }

    fn init_list_item_type(&self, ty: CQualTypeId) -> Option<CQualTypeId> {
        match self.ast_context.resolve_type(ty.ctype).kind {
            CTypeKind::ConstantArray(inner, _)
            | CTypeKind::IncompleteArray(inner)
            | CTypeKind::VariableArray(inner, _) => Some(CQualTypeId {
                ctype: inner,
                qualifiers: ty.qualifiers,
            }),
            _ => None,
        }
    }

    fn default_initializer_for_ctype(&self, ty: CTypeId) -> TranslationResult<DaExpr> {
        match self.ast_context.resolve_type(ty).kind {
            CTypeKind::Union(union_id) => {
                let name = match self.convert_type(CQualTypeId::new(ty))?.kind {
                    DaTypeKind::Named(name) => name,
                    _ => {
                        return Err(TranslationError::generic(
                            "union wrapper has no daScript name",
                        ))
                    }
                };
                let size = self.record_layout(union_id)?.object.size_bytes;
                Ok(DaExpr::MakeStruct {
                    type_name: name,
                    fields: vec![(
                        "c2da_storage".into(),
                        DaExpr::Call(
                            Box::new(DaExpr::Var("c2da_rt_calloc".into())),
                            vec![
                                self.integer_literal_for_type(
                                    DaExpr::ConstInt(1),
                                    DaType::uint64(),
                                ),
                                self.integer_literal_for_type(
                                    DaExpr::ConstInt(i64::try_from(size).map_err(|_| {
                                        TranslationError::generic(
                                            "union size exceeds daScript integer range",
                                        )
                                    })?),
                                    DaType::uint64(),
                                ),
                            ],
                        ),
                    )],
                })
            }
            CTypeKind::Struct(_) => {
                let das_type = self.convert_type(CQualTypeId::new(ty))?;
                if let DaTypeKind::Named(name) = das_type.kind {
                    Ok(DaExpr::Call(Box::new(DaExpr::Var(name)), vec![]))
                } else {
                    Ok(zero_for_datype(&das_type))
                }
            }
            _ => {
                let das_type = self.convert_type(CQualTypeId::new(ty))?;
                Ok(zero_for_datype(&das_type))
            }
        }
    }

    fn convert_struct_init_list(
        &self,
        ctx: ExprContext,
        ty: CQualTypeId,
        init_ids: &[CExprId],
    ) -> TranslationResult<Option<WithStmts<DaExpr>>> {
        let rec_id = match self.ast_context.resolve_type(ty.ctype).kind {
            CTypeKind::Struct(rec_id) | CTypeKind::Union(rec_id) => rec_id,
            _ => return Ok(None),
        };
        let das_type = self.convert_type(ty)?;
        let DaTypeKind::Named(type_name) = das_type.kind else {
            return Ok(None);
        };
        let fields = match &self.ast_context[rec_id].kind {
            CDeclKind::Struct {
                fields: Some(fields),
                ..
            }
            | CDeclKind::Union {
                fields: Some(fields),
                ..
            } => fields,
            _ => return Ok(None),
        };
        let mut is_unsafe = false;
        let mut stmts = vec![];
        let mut values = vec![];
        for (&field_id, &init_id) in fields.iter().zip(init_ids.iter()) {
            let CDeclKind::Field { name, typ, .. } = &self.ast_context[field_id].kind else {
                continue;
            };
            let item = self.convert_expr(ctx, init_id, Some(*typ))?;
            is_unsafe |= item.is_unsafe;
            stmts.extend(item.stmts);
            let field_name = self
                .type_converter
                .borrow()
                .resolve_field_name(Some(rec_id), field_id)
                .unwrap_or_else(|| name.clone());
            values.push((field_name, item.val));
        }
        Ok(Some(
            WithStmts::new(
                stmts,
                DaExpr::MakeStruct {
                    type_name,
                    fields: values,
                },
            )
            .merge_unsafe(is_unsafe),
        ))
    }
}

/// Collected switch case branch.
struct SwitchCase {
    values: Vec<DaExpr>, // empty = default
    stmts: Vec<DaStmt>,
}

/// Map CTypeKind → the daScript *storage* type of a scalar C type.
///
/// This must agree with `Translation::convert_type_inner`; it exists only for
/// the callers that hold a resolved `CTypeKind` and no `CQualTypeId`.  It
/// deliberately answers `None` for every type whose daScript spelling needs
/// the surrounding context (pointers, records, arrays) so that no caller can
/// mistake two different pointer types for one another.
fn scalar_type_kind_to_datype(kind: &CTypeKind) -> Option<DaType> {
    use CTypeKind::*;
    Some(match kind {
        Void => DaType::void(),
        Bool => DaType::bool(),
        Int | Int32 => DaType::int(),
        SChar | Char | Int8 => DaType::int8(),
        Short | Int16 => DaType::int16(),
        Int64 | Long | LongLong => DaType::int64(),
        IntPtr | SSize | PtrDiff | IntMax => DaType::int64(),
        UChar | UInt8 => DaType::uint8(),
        UShort | UInt16 => DaType::uint16(),
        UInt | UInt32 => DaType::uint(),
        UInt64 | ULong | ULongLong | UIntPtr | Size | WChar => DaType::uint64(),
        Float | BFloat16 => DaType::float(),
        Double => DaType::double(),
        _ => return None,
    })
}

fn type_kind_to_datype(kind: &CTypeKind) -> DaType {
    scalar_type_kind_to_datype(kind).unwrap_or_else(DaType::auto)
}

fn writable_type(mut ty: DaType) -> DaType {
    ty.is_const = false;
    ty.is_ref = false;
    ty.is_temporary = false;
    ty
}

fn zero_for_datype(ty: &DaType) -> DaExpr {
    // A daScript function value has no numeric representation to reinterpret.
    if crate::convert_type::is_function_value_type(ty) {
        return DaExpr::DefaultValue(ty.clone());
    }
    match &ty.kind {
        DaTypeKind::Pointer(_) => abi::null_pointer(ty),
        // daScript has no `bool(0)`; C's zero `_Bool` is `false`.
        DaTypeKind::Bool => DaExpr::ConstBool(false),
        // A floating zero is not an integer zero: `x != 0.0` must stay a
        // floating comparison rather than becoming an integer truncation.
        DaTypeKind::Float => DaExpr::ConstFloat(0.0),
        DaTypeKind::Double => DaExpr::ConstDouble(0.0),
        // A declaration may need a temporary default before CFG emits its C
        // initializer assignment. Arrays are aggregates, never numeric zero.
        // `[]` is typed by the declaration and remains distinct from the C
        // InitList assignment that follows.
        DaTypeKind::Array(_) => DaExpr::MakeArray(vec![]),
        DaTypeKind::FixedArray(elem_ty, size) => DaExpr::MakeFixedArray {
            elem_type: elem_ty.as_ref().clone(),
            items: (0..*size).map(|_| zero_for_datype(elem_ty)).collect(),
        },
        // A named non-numeric type is a struct/union wrapper: `Name()` is its
        // daScript default value, `cast<Name>(0)` is not a conversion at all.
        DaTypeKind::Named(name) if !ty.is_numeric() => {
            DaExpr::Call(Box::new(DaExpr::Var(name.clone())), vec![])
        }
        _ => DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(DaExpr::ConstInt(0)),
            to: ty.clone(),
        },
    }
}

fn lower_minmax_conditional(cond: &DaExpr, then_e: &DaExpr, else_e: &DaExpr) -> Option<DaExpr> {
    let DaExpr::Op2 { op, left, right } = cond else {
        return None;
    };
    if !matches!(*op, "<" | "<=" | ">" | ">=") {
        return None;
    }

    let left_is_then = expr_text_eq(left, then_e);
    let right_is_else = expr_text_eq(right, else_e);
    let right_is_then = expr_text_eq(right, then_e);
    let left_is_else = expr_text_eq(left, else_e);

    let op_kind = match (
        *op,
        left_is_then && right_is_else,
        right_is_then && left_is_else,
    ) {
        ("<" | "<=", true, _) => "min",
        (">" | ">=", true, _) => "max",
        ("<" | "<=", _, true) => "max",
        (">" | ">=", _, true) => "min",
        _ => return None,
    };
    let helper_ty = minmax_helper_type(left.as_ref(), right.as_ref());
    let fn_name = format!("c2da_{}_{}", op_kind, helper_ty.suffix);

    Some(DaExpr::Call(
        Box::new(DaExpr::Var(fn_name.to_string())),
        vec![
            cast_minmax_arg(left.as_ref().clone(), helper_ty.ty.clone()),
            cast_minmax_arg(right.as_ref().clone(), helper_ty.ty),
        ],
    ))
}

fn expr_text_eq(lhs: &DaExpr, rhs: &DaExpr) -> bool {
    format!("{}", lhs) == format!("{}", rhs)
}

fn is_zero_initializer_expr(expr: &DaExpr) -> bool {
    match expr {
        DaExpr::ConstInt(0) | DaExpr::ConstUInt(0) => true,
        DaExpr::Cast { expr, .. } => is_zero_initializer_expr(expr),
        _ => false,
    }
}

#[derive(Clone)]
struct MinMaxHelperType {
    suffix: &'static str,
    ty: DaType,
}

fn minmax_helper_type(left: &DaExpr, right: &DaExpr) -> MinMaxHelperType {
    match (minmax_numeric_type(left), minmax_numeric_type(right)) {
        (Some(MinMaxNumericType::UInt64), _) | (_, Some(MinMaxNumericType::UInt64)) => {
            MinMaxHelperType {
                suffix: "uint64",
                ty: DaType::uint64(),
            }
        }
        (Some(MinMaxNumericType::Int64), _) | (_, Some(MinMaxNumericType::Int64)) => {
            MinMaxHelperType {
                suffix: "int64",
                ty: DaType::int64(),
            }
        }
        (Some(MinMaxNumericType::UInt), Some(MinMaxNumericType::UInt)) => MinMaxHelperType {
            suffix: "uint",
            ty: DaType::uint(),
        },
        _ => MinMaxHelperType {
            suffix: "int",
            ty: DaType::int(),
        },
    }
}

#[derive(Copy, Clone)]
enum MinMaxNumericType {
    Int,
    UInt,
    Int64,
    UInt64,
}

fn minmax_numeric_type(expr: &DaExpr) -> Option<MinMaxNumericType> {
    match expr {
        DaExpr::ConstUInt(_) => Some(MinMaxNumericType::UInt),
        DaExpr::ConstInt(_) => Some(MinMaxNumericType::Int),
        DaExpr::Cast { to, .. } => match to.kind {
            DaTypeKind::UInt64 => Some(MinMaxNumericType::UInt64),
            DaTypeKind::Int64 => Some(MinMaxNumericType::Int64),
            DaTypeKind::UInt | DaTypeKind::UInt16 | DaTypeKind::UInt8 => {
                Some(MinMaxNumericType::UInt)
            }
            DaTypeKind::Int | DaTypeKind::Int16 | DaTypeKind::Int8 => Some(MinMaxNumericType::Int),
            _ => None,
        },
        _ => None,
    }
}

fn cast_minmax_arg(expr: DaExpr, to: DaType) -> DaExpr {
    DaExpr::Cast {
        kind: das_ast::CastKind::Cast,
        expr: Box::new(expr),
        to,
    }
}

fn c2da_runtime_helpers() -> Vec<DaDecl> {
    let mut helpers = runtime::declarations();
    helpers.extend(variadic::declarations());
    helpers.extend([
        c2da_minmax_helper("c2da_min_int", DaType::int(), "<"),
        c2da_minmax_helper("c2da_max_int", DaType::int(), ">"),
        c2da_minmax_helper("c2da_min_uint", DaType::uint(), "<"),
        c2da_minmax_helper("c2da_max_uint", DaType::uint(), ">"),
        c2da_minmax_helper("c2da_min_int64", DaType::int64(), "<"),
        c2da_minmax_helper("c2da_max_int64", DaType::int64(), ">"),
        c2da_minmax_helper("c2da_min_uint64", DaType::uint64(), "<"),
        c2da_minmax_helper("c2da_max_uint64", DaType::uint64(), ">"),
        c2da_clip_uint_helper(),
        c2da_bool_to_uint_helper(),
        c2da_assert_fail_helper(),
    ]);
    helpers
}

fn c2da_bool_to_uint_helper() -> DaDecl {
    DaDecl::Function(DaFunction {
        name: "c2da_bool_to_uint".to_string(),
        params: vec![DaStmt::Param {
            name: "v".to_string(),
            param_type: DaType::bool(),
            default: None,
            is_mutable: false,
        }],
        ret_type: DaType::uint(),
        body: Some(DaExpr::Block(DaBlock {
            stmts: vec![DaStmt::Expr(DaExpr::IfThenElse {
                // `DaExpr` currently carries no type on a plain variable.  Make
                // the helper condition explicitly boolean so the printer never
                // routes it through C-style numeric truthiness.
                cond: Box::new(DaExpr::Op2 {
                    op: "==",
                    left: Box::new(DaExpr::Var("v".to_string())),
                    right: Box::new(DaExpr::ConstBool(true)),
                }),
                then: Box::new(DaExpr::Block(DaBlock {
                    stmts: vec![DaStmt::Expr(DaExpr::Return(Some(Box::new(
                        DaExpr::ConstUInt(1),
                    ))))],
                })),
                elifs: vec![],
                else_: Some(Box::new(DaExpr::Block(DaBlock {
                    stmts: vec![DaStmt::Expr(DaExpr::Return(Some(Box::new(
                        DaExpr::ConstUInt(0),
                    ))))],
                }))),
            })],
        })),
        annotations: vec![],
        is_public: false,
        is_unsafe: false,
    })
}

fn c2da_assert_fail_helper() -> DaDecl {
    let ptr = DaType::pointer(DaType::void());
    DaDecl::Function(DaFunction {
        name: "c2da___assert_fail".to_string(),
        params: vec![
            DaStmt::Param {
                name: "expr".to_string(),
                param_type: ptr.clone(),
                default: None,
                is_mutable: false,
            },
            DaStmt::Param {
                name: "file".to_string(),
                param_type: ptr.clone(),
                default: None,
                is_mutable: false,
            },
            DaStmt::Param {
                name: "line".to_string(),
                param_type: DaType::uint(),
                default: None,
                is_mutable: false,
            },
            DaStmt::Param {
                name: "func".to_string(),
                param_type: ptr,
                default: None,
                is_mutable: false,
            },
        ],
        ret_type: DaType::void(),
        body: Some(DaExpr::Block(DaBlock { stmts: vec![] })),
        annotations: vec![],
        is_public: false,
        is_unsafe: false,
    })
}

fn c2da_clip_uint_helper() -> DaDecl {
    DaDecl::Function(DaFunction {
        name: "c2da_clip_uint".to_string(),
        params: vec![DaStmt::Param {
            name: "v".to_string(),
            param_type: DaType::int(),
            default: None,
            is_mutable: false,
        }],
        ret_type: DaType::uint(),
        body: Some(DaExpr::Block(DaBlock {
            stmts: vec![
                DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(DaExpr::Op2 {
                        op: "<",
                        left: Box::new(DaExpr::Var("v".to_string())),
                        right: Box::new(DaExpr::ConstInt(0)),
                    }),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![DaStmt::Expr(DaExpr::Return(Some(Box::new(
                            DaExpr::ConstUInt(0),
                        ))))],
                    })),
                    elifs: vec![],
                    else_: None,
                }),
                DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(DaExpr::Op2 {
                        op: ">",
                        left: Box::new(DaExpr::Var("v".to_string())),
                        right: Box::new(DaExpr::ConstInt(255)),
                    }),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![DaStmt::Expr(DaExpr::Return(Some(Box::new(
                            DaExpr::ConstUInt(255),
                        ))))],
                    })),
                    elifs: vec![],
                    else_: None,
                }),
                DaStmt::Expr(DaExpr::Return(Some(Box::new(DaExpr::Cast {
                    kind: das_ast::CastKind::Cast,
                    expr: Box::new(DaExpr::Var("v".to_string())),
                    to: DaType::uint(),
                })))),
            ],
        })),
        annotations: vec![],
        is_public: false,
        is_unsafe: false,
    })
}

fn c2da_minmax_helper(name: &str, ty: DaType, op: &'static str) -> DaDecl {
    DaDecl::Function(DaFunction {
        name: name.to_string(),
        params: vec![
            DaStmt::Param {
                name: "a".to_string(),
                param_type: ty.clone(),
                default: None,
                is_mutable: false,
            },
            DaStmt::Param {
                name: "b".to_string(),
                param_type: ty.clone(),
                default: None,
                is_mutable: false,
            },
        ],
        ret_type: ty,
        body: Some(DaExpr::Block(DaBlock {
            stmts: vec![DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(DaExpr::Op2 {
                    op,
                    left: Box::new(DaExpr::Var("a".to_string())),
                    right: Box::new(DaExpr::Var("b".to_string())),
                }),
                then: Box::new(DaExpr::Block(DaBlock {
                    stmts: vec![DaStmt::Expr(DaExpr::Return(Some(Box::new(DaExpr::Var(
                        "a".to_string(),
                    )))))],
                })),
                elifs: vec![],
                else_: Some(Box::new(DaExpr::Block(DaBlock {
                    stmts: vec![DaStmt::Expr(DaExpr::Return(Some(Box::new(DaExpr::Var(
                        "b".to_string(),
                    )))))],
                }))),
            })],
        })),
        annotations: vec![],
        is_public: false,
        is_unsafe: false,
    })
}

/// True when a failed type declaration carries no daScript declaration of its
/// own by construction, rather than because its C type cannot be lowered.
///
/// These are the four bookkeeping skips the two-pass export performs: a
/// typedef Clang synthesised, an anonymous record or enum whose declaration is
/// emitted through its typedef, and a `typedef struct Foo Foo` whose struct
/// declaration already introduced the name. Every other failure means a user
/// type has no daScript representation and must fail a strict translation.
fn is_benign_type_skip(error: &TranslationError) -> bool {
    let cause = error.to_string();
    [
        "skipping implicit typedef",
        "anonymous struct (will be handled by typedef)",
        "anonymous enum",
        "redundant self-typedef, skipping",
        "typedef target has no daScript representation; reported at each use",
    ]
    .iter()
    .any(|benign| cause.contains(benign))
}

fn zero_for_ctype_kind(kind: &CTypeKind) -> DaExpr {
    if kind.is_unsigned_integral_type() {
        DaExpr::ConstUInt(0)
    } else {
        DaExpr::ConstInt(0)
    }
}

/// Разворачивает return if(c) a else b → if(c) { return a } else { return b }
fn convert_ifexpr_to_return(expr: &DaExpr, stmts: &mut Vec<DaStmt>) {
    // Extract optional Cast wrapper and inner IfThenElse
    let (cast_kind, cast_to, inner) = match expr {
        DaExpr::IfThenElse { .. } => (None, None, expr),
        DaExpr::Cast {
            kind,
            expr: inner,
            to,
        } => match inner.as_ref() {
            DaExpr::IfThenElse { .. } => (Some(kind.clone()), Some(to.clone()), inner.as_ref()),
            _ => return,
        },
        _ => return,
    };
    // Wraps a branch value with the outer Cast if present
    let wrap = |e: DaExpr| -> DaExpr {
        match &cast_kind {
            Some(k) => DaExpr::Cast {
                kind: k.clone(),
                expr: Box::new(e),
                to: cast_to.clone().unwrap(),
            },
            None => e,
        }
    };
    if let DaExpr::IfThenElse {
        cond,
        then,
        elifs,
        else_,
    } = inner
    {
        let then_ret = DaStmt::Expr(DaExpr::Return(Some(Box::new(wrap(then.as_ref().clone())))));
        if let Some(el) = else_ {
            let else_ret = DaStmt::Expr(DaExpr::Return(Some(Box::new(wrap(el.as_ref().clone())))));
            let mut body = vec![then_ret];
            for (ec, eb) in elifs {
                let eb_ret = DaStmt::Expr(DaExpr::Return(Some(Box::new(wrap(eb.clone())))));
                body.push(DaStmt::Expr(DaExpr::IfThenElse {
                    cond: Box::new(ec.clone()),
                    then: Box::new(DaExpr::Block(DaBlock {
                        stmts: vec![eb_ret],
                    })),
                    elifs: vec![],
                    else_: None,
                }));
            }
            stmts.push(DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(cond.as_ref().clone()),
                then: Box::new(DaExpr::Block(DaBlock { stmts: body })),
                elifs: vec![],
                else_: Some(Box::new(DaExpr::Block(DaBlock {
                    stmts: vec![else_ret],
                }))),
            }));
        } else {
            stmts.push(DaStmt::Expr(DaExpr::IfThenElse {
                cond: Box::new(cond.as_ref().clone()),
                then: Box::new(DaExpr::Block(DaBlock {
                    stmts: vec![then_ret],
                })),
                elifs: vec![],
                else_: None,
            }));
        }
    }
}

fn convert_binop(op: CBinOp) -> Result<&'static str, &'static str> {
    use CBinOp::*;
    match op {
        Add => Ok("+"),
        Subtract => Ok("-"),
        Multiply => Ok("*"),
        Divide => Ok("/"),
        Modulus => Ok("%"),
        And => Ok("&&"),
        Or => Ok("||"),
        BitAnd => Ok("&"),
        BitOr => Ok("|"),
        BitXor => Ok("^"),
        ShiftLeft => Ok("<<"),
        ShiftRight => Ok(">>"),
        EqualEqual => Ok("=="),
        NotEqual => Ok("!="),
        Less => Ok("<"),
        Greater => Ok(">"),
        LessEqual => Ok("<="),
        GreaterEqual => Ok(">="),
        _ => Err("unsupported binary op in daScript"),
    }
}

/// Main entry point: creates a `Translation` and produces a daScript module
/// string. A top-level C declaration that cannot be lowered is an error, never
/// an omitted daScript declaration — there is no lossy alternative that would
/// print a partial module instead.
pub fn translate_checked(
    ast_context: TypedAstContext,
    tcfg: &TranspilerConfig,
    main_file: &Path,
) -> TranslationResult<(
    String,
    Option<()>,
    Vec<(&'static str, Vec<&'static str>)>,
    IndexSet<ExternCrate>,
)> {
    translate_impl(ast_context, tcfg, main_file, true)
}

fn translate_impl(
    ast_context: TypedAstContext,
    tcfg: &TranspilerConfig,
    main_file: &Path,
    strict_top_level: bool,
) -> TranslationResult<(
    String,
    Option<()>,
    Vec<(&'static str, Vec<&'static str>)>,
    IndexSet<ExternCrate>,
)> {
    let mut t = Translation::new(ast_context, tcfg, main_file);

    // Per-translation-unit arenas: the string-literal backing arrays and the
    // builtin prelude helpers are collected while lowering and drained into
    // the module below.
    literals::reset_string_literals();
    builtins::reset_builtin_helpers();

    // Prune unreachable system declarations (removes __-prefixed noise from system headers)
    t.ast_context.prune_unwanted_decls(false);
    t.ast_context.set_prenamed_decls();

    for (&typedef_id, &subdecl_id) in &t.ast_context.prenamed_decls {
        if let CDeclKind::Typedef { ref name, .. } = t.ast_context[typedef_id].kind {
            t.type_converter
                .borrow_mut()
                .ensure_decl_name(subdecl_id, name);
            t.type_converter
                .borrow_mut()
                .alias_decl_name(typedef_id, subdecl_id);
        }
    }
    for (&decl_id, decl) in t.ast_context.iter_decls() {
        use CDeclKind::*;
        match decl.kind {
            Struct {
                name: Some(ref name),
                ..
            }
            | Union {
                name: Some(ref name),
                ..
            }
            | Enum {
                name: Some(ref name),
                ..
            } => {
                t.type_converter
                    .borrow_mut()
                    .ensure_decl_name(decl_id, name);
            }
            Typedef { ref name, .. } if !t.ast_context.prenamed_decls.contains_key(&decl_id) => {
                t.type_converter
                    .borrow_mut()
                    .ensure_decl_name(decl_id, name);
            }
            _ => {}
        }
    }

    // Pass 1: export all type declarations (struct, enum, union, typedef)
    let mut decls: Vec<DaDecl> = vec![];
    let mut exported_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (&decl_id, decl) in t.ast_context.iter_decls() {
        use CDeclKind::*;
        let needs_export = match decl.kind {
            Struct { .. } => true,
            Enum { .. } => true,
            Union { .. } => true,
            Typedef { .. } => true,
            _ => false,
        };
        if needs_export {
            match t.convert_decl(
                ExprContext {
                    used: true,
                    is_const: false,
                    ..Default::default()
                },
                decl_id,
            ) {
                Ok(das_decl) => {
                    // Track emitted type declarations for dedup.
                    // Named structs/enums dedup by name; anonymous structs
                    // dedup by (name, field_type_signature) for accuracy.
                    match &das_decl {
                        DaDecl::Structure(s) => {
                            if s.name.starts_with("Unnamed_") {
                                t.emitted_anon_structs
                                    .borrow_mut()
                                    .insert(anonymous_struct_signature(s));
                            } else {
                                t.emitted_structs.borrow_mut().insert(s.name.clone());
                            }
                        }
                        DaDecl::Enumeration(e) => {
                            t.emitted_structs.borrow_mut().insert(e.name.clone());
                        }
                        _ => {}
                    }
                    // Skip duplicate typedefs and named structs (daScript rejects them).
                    let type_name = decl.kind.get_name().map(|s| s.to_string());
                    if let Some(ref name) = type_name {
                        if !name.starts_with("Unnamed_") && !exported_names.insert(name.clone()) {
                            continue;
                        }
                    }
                    decls.push(das_decl)
                }
                Err(e) => {
                    let name = decl
                        .kind
                        .get_name()
                        .cloned()
                        .unwrap_or_else(|| "?".to_string());
                    // A type declaration that carries no daScript declaration
                    // of its own — a typedef the struct already defines, an
                    // anonymous record reached through its typedef — is an
                    // implementation detail of this lowering, not an
                    // unsupported user program, so it stays a skip even under
                    // strict translation.
                    if strict_top_level && !is_benign_type_skip(&e) {
                        let c_type = match &decl.kind {
                            Typedef { typ, .. } => {
                                format!("{:?}", t.ast_context.resolve_type(typ.ctype).kind)
                            }
                            Struct { .. } => "struct".to_string(),
                            Union { .. } => "union".to_string(),
                            Enum { .. } => "enum".to_string(),
                            _ => "type declaration".to_string(),
                        };
                        return Err(format_translation_err!(
                            t.ast_context.display_loc(&decl.loc),
                            "operation=type declaration lowering; c_type={}; declaration={}; cause={}",
                            c_type,
                            name,
                            e
                        ));
                    }
                    warn!("Skipping type decl {}: {}", name, e);
                }
            }
        }
    }

    // Pass 2: export top-level value declarations (function with bodies, variable, macro)
    for &top_id in &t.ast_context.c_decls_top {
        use CDeclKind::*;
        let needs_export = match t.ast_context[top_id].kind {
            Function { body: Some(_), .. } => true, // only functions with bodies
            Variable { .. } => true,
            // Macro declarations have no standalone daScript declaration.
            // Their expanded C AST is lowered at the use site. The historical
            // permissive path reports them as skipped; strict translation must
            // also skip them rather than mistake that implementation detail for
            // an unsupported user program.
            MacroObject { .. } | MacroFunction { .. } => !strict_top_level,
            _ => false, // types already exported in pass 1; fn decls without body skipped
        };
        if !needs_export {
            continue;
        }
        let decl = &t.ast_context[top_id];
        match t.convert_decl(
            ExprContext {
                used: true,
                is_const: false,
                ..Default::default()
            },
            top_id,
        ) {
            Ok(das_decl) => decls.push(das_decl),
            Err(e) => {
                if strict_top_level {
                    let c_type = match &decl.kind {
                        Function { typ, .. } => {
                            format!("{:?}", t.ast_context.resolve_type(*typ).kind)
                        }
                        Variable { typ, .. } | Typedef { typ, .. } => {
                            format!("{:?}", t.ast_context.resolve_type(typ.ctype).kind)
                        }
                        _ => "declaration".to_string(),
                    };
                    return Err(format_translation_err!(
                        t.ast_context.display_loc(&decl.loc),
                        "operation=top-level declaration lowering; c_type={}; declaration={}; cause={}",
                        c_type,
                        decl.kind.get_name().map(String::as_str).unwrap_or("?"),
                        e
                    ));
                }
                let name = decl
                    .kind
                    .get_name()
                    .cloned()
                    .unwrap_or_else(|| "?".to_string());
                warn!("Skipping decl {}: {}", name, e);
            }
        }
    }

    // Pass 3: export enum constants as global variables (daScript uses `Enum.Constant` syntax,
    // but C code uses bare constant names. Generate `var CONST : EnumType = EnumType.CONST` aliases.)
    let mut enum_const_decls: Vec<DaDecl> = vec![];
    for (&ec_id, decl) in t.ast_context.iter_decls() {
        if let CDeclKind::EnumConstant { ref name, value } = &decl.kind {
            let var_name = t.declare_value_name(ec_id, name);
            if !exported_names.insert(var_name.clone()) {
                continue;
            }
            // C gives an enumeration constant the enumeration's own integer
            // type: a value above INT_MAX is `unsigned int`, never a negative
            // `int`.
            let (das_val, das_type) = match value {
                crate::c_ast::ConstIntExpr::U(v) => {
                    let ty = if *v > u64::from(u32::MAX) {
                        DaType::uint64()
                    } else {
                        DaType::uint()
                    };
                    (
                        DaExpr::Cast {
                            kind: das_ast::CastKind::Cast,
                            expr: Box::new(DaExpr::ConstUInt(*v)),
                            to: ty.clone(),
                        },
                        ty,
                    )
                }
                crate::c_ast::ConstIntExpr::I(v) => {
                    let ty = if *v > i64::from(i32::MAX) || *v < i64::from(i32::MIN) {
                        DaType::int64()
                    } else {
                        DaType::int()
                    };
                    (
                        DaExpr::Cast {
                            kind: das_ast::CastKind::Cast,
                            expr: Box::new(DaExpr::ConstInt(*v)),
                            to: ty.clone(),
                        },
                        ty,
                    )
                }
            };
            enum_const_decls.push(DaDecl::Variable(DaVariable {
                name: var_name,
                var_type: das_type,
                init: Some(das_val),
                annotations: vec![],
            }));
        }
    }
    decls.extend(enum_const_decls);
    let mut module_decls = c2da_runtime_helpers();
    // Function-scope `static` storage lowered while pass 2 walked the bodies.
    // It must precede the functions that read it, and it is initialised once.
    module_decls.extend(t.take_hoisted_statics());
    // A C string literal has static storage duration; its backing byte array
    // is a module-level object, declared before any function that takes its
    // address.
    module_decls.extend(literals::take_string_literal_declarations());
    module_decls.extend(builtins::take_builtin_helper_declarations());
    module_decls.extend(decls);

    // Build the daScript module
    let module = DaModule {
        name: main_file
            .file_stem()
            .map(|s| s.to_string_lossy().to_string()),
        requires: vec![],
        options: vec!["gen2".into()],
        decls: module_decls,
    };

    Ok((module.to_string(), None, vec![], IndexSet::new()))
}
