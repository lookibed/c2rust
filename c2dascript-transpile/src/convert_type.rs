use crate::c_ast::*;
use crate::diagnostics::{TranslationError, TranslationResult};
use crate::renamer::Renamer;
use crate::translator::Translation;
use crate::TranspilerConfig;
use crate::{CrateSet, ExternCrate};
use das_ast::{DaType, DaTypeKind};
use indexmap::IndexSet;
use std::collections::{HashMap, HashSet};
use std::ops::Index;

/// Placeholder for a C function type that has no daScript function value:
/// today only the variadic ones, whose ABI boundary is rejected at the call
/// site with its own diagnostic.
pub(crate) const UNTYPED_FUNCTION: &str = "function";

/// True for the daScript types produced by `TypeConverter::function_value_type`,
/// i.e. the ones that are already callable values and must never be wrapped in
/// `?` or crossed through the raw-pointer ABI.
pub(crate) fn is_function_value_type(ty: &DaType) -> bool {
    matches!(&ty.kind, DaTypeKind::Named(name)
        if name == UNTYPED_FUNCTION || name.starts_with("function<"))
}

#[derive(Debug, Hash, PartialEq, Eq, Clone)]
enum FieldKey {
    Field(CFieldId),
    Padding(usize),
}

pub struct TypeConverter {
    pub translate_valist: bool,
    renamer: Renamer<CDeclId>,
    fields: HashMap<CDeclId, Renamer<FieldKey>>,
    suffix_names: HashMap<(CDeclId, &'static str), String>,
    features: HashSet<&'static str>,
    pub extern_crates: CrateSet,
}

impl TypeConverter {
    pub fn new(tcfg: &TranspilerConfig) -> TypeConverter {
        TypeConverter {
            translate_valist: tcfg.translate_valist,
            renamer: Renamer::type_namespace(),
            fields: HashMap::new(),
            suffix_names: HashMap::new(),
            features: HashSet::new(),
            extern_crates: IndexSet::new(),
        }
    }

    fn use_crate(&mut self, extern_crate: ExternCrate) {
        self.extern_crates.insert(extern_crate);
    }

    pub fn features_used(&self) -> &HashSet<&'static str> {
        &self.features
    }
    pub fn extern_crates_used(&self) -> &CrateSet {
        &self.extern_crates
    }

    pub fn declare_decl_name(&mut self, decl_id: CDeclId, name: &str) -> String {
        self.renamer
            .insert(decl_id, name)
            .expect("Name already assigned")
    }
    pub fn ensure_decl_name(&mut self, decl_id: CDeclId, name: &str) -> String {
        self.resolve_decl_name(decl_id)
            .unwrap_or_else(|| self.declare_decl_name(decl_id, name))
    }
    pub fn alias_decl_name(&mut self, new_id: CDeclId, old_id: CDeclId) {
        self.renamer.alias(new_id, &old_id)
    }
    pub fn resolve_decl_name(&self, decl_id: CDeclId) -> Option<String> {
        self.renamer.get(&decl_id)
    }
    pub fn declare_field_name(
        &mut self,
        rec_id: CRecordId,
        fld_id: CFieldId,
        name: &str,
    ) -> String {
        let key = FieldKey::Field(fld_id);
        if let Some(existing) = self.fields.get(&rec_id).and_then(|r| r.get(&key)) {
            return existing;
        }
        let name = if name.is_empty() {
            "c2da_unnamed"
        } else {
            name
        };
        self.fields
            .entry(rec_id)
            .or_insert_with(|| Renamer::keywords())
            .insert(key, name)
            .expect("Field already declared")
    }
    pub fn resolve_field_name(
        &self,
        rec_id: Option<CRecordId>,
        fld_id: CFieldId,
    ) -> Option<String> {
        let key = FieldKey::Field(fld_id);
        match rec_id {
            Some(id) => self.fields.get(&id)?.get(&key),
            None => self.fields.values().flat_map(|x| x.get(&key)).next(),
        }
    }

    pub fn convert(&mut self, ctxt: &TypedAstContext, ctype: CTypeId) -> TranslationResult<DaType> {
        if self.translate_valist && ctxt.is_va_list(ctype) {
            return Ok(DaType::uint64());
        }
        self.convert_inner(ctxt, ctype)
    }

    fn convert_inner(
        &mut self,
        ctxt: &TypedAstContext,
        ctype: CTypeId,
    ) -> TranslationResult<DaType> {
        match ctxt.index(ctype).kind {
            CTypeKind::Void => Ok(DaType::void()),
            CTypeKind::Bool => Ok(DaType::bool()),
            CTypeKind::Short | CTypeKind::Int => Ok(DaType::int()),
            CTypeKind::Long | CTypeKind::LongLong => Ok(DaType::int64()),
            CTypeKind::UShort | CTypeKind::UInt => Ok(DaType::uint()),
            CTypeKind::ULong | CTypeKind::ULongLong => Ok(DaType::uint64()),
            CTypeKind::SChar | CTypeKind::Char => Ok(DaType::int8()),
            CTypeKind::UChar => Ok(DaType::uint8()),
            CTypeKind::Double | CTypeKind::LongDouble | CTypeKind::Float128 => Ok(DaType::double()),
            CTypeKind::Float => Ok(DaType::float()),
            CTypeKind::Int8 => Ok(DaType::int8()),
            CTypeKind::Int16 => Ok(DaType::int16()),
            CTypeKind::Int32 => Ok(DaType::int()),
            CTypeKind::Int64 => Ok(DaType::int64()),
            CTypeKind::IntPtr | CTypeKind::SSize | CTypeKind::PtrDiff => Ok(DaType::int64()),
            CTypeKind::UInt8 => Ok(DaType::uint8()),
            CTypeKind::UInt16 => Ok(DaType::uint16()),
            CTypeKind::UInt32 => Ok(DaType::uint()),
            CTypeKind::UInt64 => Ok(DaType::uint64()),
            CTypeKind::UIntPtr | CTypeKind::Size => Ok(DaType::uint64()),
            CTypeKind::Int128 => Ok(DaType::int64()),
            CTypeKind::UInt128 => Ok(DaType::uint64()),
            CTypeKind::IntMax => Ok(DaType::int64()),
            CTypeKind::UIntMax => Ok(DaType::uint64()),
            CTypeKind::WChar => Ok(DaType::int()),
            CTypeKind::BFloat16 => Ok(DaType::float()),
            CTypeKind::Pointer(qtype) => {
                // A C pointer-to-function has no pointer analogue in daScript:
                // `function<…>` *is* the callable value.  See
                // `TypeConverter::function_value_type`.
                if let CTypeKind::Function(ret, ref params, is_variadic, _, _) =
                    ctxt.resolve_type(qtype.ctype).kind
                {
                    let params = params.clone();
                    return self.function_value_type(ctxt, ret, &params, is_variadic);
                }
                let pointee = self.convert_pointee(ctxt, qtype.ctype)?;
                Ok(DaType::pointer(pointee))
            }
            CTypeKind::Elaborated(inner) | CTypeKind::Decayed(inner) | CTypeKind::Paren(inner) => {
                self.convert_inner(ctxt, inner)
            }
            CTypeKind::Struct(decl_id) | CTypeKind::Union(decl_id) | CTypeKind::Enum(decl_id) => {
                let name = self
                    .resolve_decl_name(decl_id)
                    .or_else(|| {
                        ctxt.prenamed_decls.iter().find(|(_, &v)| v == decl_id).and_then(|(k, _)| {
                            if let CDeclKind::Typedef { name, .. } = &ctxt[*k].kind { Some(name.clone()) } else { None }
                        })
                    })
                    .unwrap_or_else(|| "Unnamed".into());
                Ok(DaType::named(&name))
            }
            CTypeKind::Typedef(decl_id) => {
                let name = self
                    .resolve_decl_name(decl_id)
                    .or_else(|| {
                        if let CDeclKind::Typedef { name, .. } = &ctxt[decl_id].kind {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                    .unwrap_or_else(|| "Unnamed".into());
                Ok(DaType::named(&name))
            }
            CTypeKind::ConstantArray(inner, _)
            | CTypeKind::IncompleteArray(inner)
            | CTypeKind::VariableArray(inner, _) => {
                let elt = self.convert(ctxt, inner)?;
                Ok(DaType::array(elt))
            }
            CTypeKind::Attributed(ty, _) | CTypeKind::Atomic(ty) => self.convert(ctxt, ty.ctype),
            CTypeKind::Function(ret, ref params, is_variadic, _, _) => {
                let params = params.clone();
                self.function_value_type(ctxt, ret, &params, is_variadic)
            }
            CTypeKind::TypeOf(ty) | CTypeKind::Auto(ty) => self.convert(ctxt, ty),
            ref t => {
                log::warn!("Unsupported C type kind {:?}, using auto", t);
                Ok(DaType::auto())
            }
        }
    }

    pub fn convert_pointee(
        &mut self,
        ctxt: &TypedAstContext,
        ctype: CTypeId,
    ) -> TranslationResult<DaType> {
        match ctxt.resolve_type(ctype).kind {
            CTypeKind::Void => Ok(DaType::uint64()),
            CTypeKind::VariableArray(mut elt, _) => {
                while let CTypeKind::VariableArray(elt_, _) = ctxt.resolve_type(elt).kind {
                    elt = elt_
                }
                self.convert(ctxt, elt)
            }
            _ => self.convert(ctxt, ctype),
        }
    }

    pub fn convert_function(
        &mut self,
        ctxt: &TypedAstContext,
        ret: Option<CQualTypeId>,
        params: &[CQualTypeId],
        is_var: bool,
    ) -> TranslationResult<DaType> {
        match ret {
            None => Ok(DaType::named(UNTYPED_FUNCTION)),
            Some(ret) => self.function_value_type(ctxt, ret, params, is_var),
        }
    }

    /// daScript's typed function value for a C function type.
    ///
    /// C `int (*)(int, int)` has no pointer analogue in daScript: the language's
    /// own `function<(a:int;b:int):int>` *is* the callable value.  It compares
    /// against `null`, `default<T>` is its null value, `@@name` takes one from a
    /// function and `invoke` calls it.  Parameter names are mandatory in the type
    /// syntax but take no part in type identity, so they are synthesised.
    ///
    /// A variadic function pointer keeps the old untyped placeholder: the
    /// variadic ABI boundary is diagnosed at the call site
    /// (`is_variadic_function_pointer_callee`), and producing a `TranslationError`
    /// here instead would replace that precise diagnostic with a type-conversion
    /// one.
    fn function_value_type(
        &mut self,
        ctxt: &TypedAstContext,
        ret: CQualTypeId,
        params: &[CQualTypeId],
        is_variadic: bool,
    ) -> TranslationResult<DaType> {
        if is_variadic {
            return Ok(DaType::named(UNTYPED_FUNCTION));
        }
        let mut rendered = String::from("function<(");
        for (index, param) in params.iter().enumerate() {
            if index > 0 {
                rendered.push(';');
            }
            let param_type = self.convert(ctxt, param.ctype)?;
            rendered.push_str(&format!("_arg{index}:{param_type}"));
        }
        rendered.push(')');
        let ret_type = self.convert(ctxt, ret.ctype)?;
        rendered.push_str(&format!(":{ret_type}>"));
        Ok(DaType::named(&rendered))
    }

    pub fn convert_function_param(
        &mut self,
        ctxt: &TypedAstContext,
        ctype: CTypeId,
    ) -> TranslationResult<DaType> {
        if ctxt.is_va_list(ctype) {
            return Ok(DaType::uint64());
        }
        self.convert(ctxt, ctype)
    }
}

// ====== Translation convenience methods ======

impl<'c> Translation<'c> {
    pub fn convert_type(&self, qual: CQualTypeId) -> TranslationResult<DaType> {
        let dt = self.convert_type_inner(qual.ctype)?;
        // Propagate const qualifier from C type.
        // daScript supports `int const` for values and `int const?` for pointers.
        // Struct fields strip const separately if needed.
        Ok(DaType {
            is_const: qual.qualifiers.is_const,
            ..dt
        })
    }

    pub fn convert_type_inner(&self, typ: CTypeId) -> TranslationResult<DaType> {
        let mut cur = typ;
        loop {
            match &self.ast_context[cur].kind {
                CTypeKind::Typedef(decl_id) => {
                    if let CDeclKind::Typedef { name, typ, .. } = &self.ast_context[*decl_id].kind {
                        // Skip __-prefixed — resolve to base
                        if name.starts_with("__") {
                            cur = typ.ctype;
                            continue;
                        }
                        // Если typedef ссылается на built-in тип (u8, u32, int и т.д.),
                        // не используем alias — daScript не принимает alias?
                        // (например, uint8_t? валидно, но uint8_t? в struct field — нет)
                        // Пропускаем alias для built-in типов
                        let base = self.ast_context.resolve_type(typ.ctype);
                        if matches!(
                            base.kind,
                            CTypeKind::Int
                                | CTypeKind::UInt
                                | CTypeKind::Int8
                                | CTypeKind::UInt8
                                | CTypeKind::Int16
                                | CTypeKind::UInt16
                                | CTypeKind::Int32
                                | CTypeKind::UInt32
                                | CTypeKind::Int64
                                | CTypeKind::UInt64
                                | CTypeKind::Float
                                | CTypeKind::Double
                                | CTypeKind::Bool
                                | CTypeKind::Void
                                | CTypeKind::SChar
                                | CTypeKind::UChar
                                | CTypeKind::Char
                                | CTypeKind::Short
                                | CTypeKind::UShort
                                | CTypeKind::Long
                                | CTypeKind::ULong
                                | CTypeKind::LongLong
                                | CTypeKind::ULongLong
                        ) {
                            break;
                        }
                        // For struct/enum typedefs, register under the struct's record ID
                        // rather than the typedef's decl_id. The typedef handler's
                        // ensure_decl_name(rec_id, &name) searches by the record ID;
                        // using the same key ensures the name is found and reused.
                        let name_key = match &base.kind {
                            CTypeKind::Struct(r) | CTypeKind::Union(r) | CTypeKind::Enum(r) => *r,
                            _ => *decl_id,
                        };
                        let resolved_name = self
                            .type_converter
                            .borrow_mut()
                            .ensure_decl_name(name_key, name);
                        return Ok(DaType::named(&resolved_name));
                    }
                    break;
                }
                CTypeKind::Elaborated(inner) | CTypeKind::Paren(inner) => {
                    cur = *inner;
                }
                _ => break,
            }
        }
        let resolved = self.ast_context.resolve_type(typ);
        use CTypeKind::*;
        match resolved.kind {
            Void => Ok(DaType::void()),
            Bool => Ok(DaType::bool()),
            Int | Int32 => Ok(DaType::int()),
            SChar | Char | Int8 => Ok(DaType::int8()),
            Short | Int16 => Ok(DaType::int16()),
            Int64 | Long | LongLong => Ok(DaType::int64()),
            IntPtr | SSize | PtrDiff | IntMax => Ok(DaType::int64()),
            UChar | UInt8 => Ok(DaType::uint8()),
            UShort | UInt16 => Ok(DaType::uint16()),
            UInt | UInt32 => Ok(DaType::uint()),
            UInt64 | ULong | ULongLong | UIntPtr | Size | WChar => Ok(DaType::uint64()),
            Float | BFloat16 => Ok(DaType::float()),
            Double => Ok(DaType::double()),
            // daScript has no 128-bit integer and no wider-than-double float.
            // Silently narrowing them would change observable C results, so
            // the strict translator refuses instead.
            Int128 | UInt128 => Err(TranslationError::generic(
                "C 128-bit integer type has no daScript representation",
            )),
            LongDouble | Float128 => Err(TranslationError::generic(
                "C extended-precision floating type has no daScript representation",
            )),
            Pointer(inner) => {
                // A pointer to a function is the daScript function value itself.
                if let Function(ret, ref params, is_variadic, _, _) =
                    self.ast_context.resolve_type(inner.ctype).kind
                {
                    let params = params.clone();
                    return self.type_converter.borrow_mut().convert_function(
                        &self.ast_context,
                        Some(ret),
                        &params,
                        is_variadic,
                    );
                }
                if matches!(self.ast_context.resolve_type(inner.ctype).kind, Void) {
                    // C `void *` is still a pointer at the source boundary.
                    // Only the canonical runtime ABI represents exposed
                    // addresses as uint64; collapsing void* here loses the
                    // type needed to convert that address back to `T?`.
                    return Ok(DaType::pointer(DaType::uint8()));
                }
                let inner_ty = self.convert_type(inner)?;
                Ok(DaType::pointer(inner_ty))
            }
            // A C array of constant extent owns inline storage whose layout
            // Clang already described.  daScript's `T[N]` is the only array
            // form with that property: it is stored inline, it copies by
            // value, and `addr(a[0])` decays to a pointer over the same
            // bytes.  `array<T>` is a heap handle and would make every Clang
            // offset in object_memory.rs wrong.
            ConstantArray(inner, size) => {
                let inner_ty = self.convert_type_raw(inner)?;
                if size == 0 {
                    return Err(TranslationError::generic(
                        "zero-length C array has no daScript storage",
                    ));
                }
                Ok(DaType::fixed_array(inner_ty, size))
            }
            IncompleteArray(inner) | VariableArray(inner, _) => {
                let inner_ty = self.convert_type_raw(inner)?;
                Ok(DaType::array(inner_ty))
            }
            Vector(_, _) | UnhandledSveType => self.reject_vector_type(typ),
            Function(ret, ref params, is_variadic, _, _) => {
                let params = params.clone();
                self.type_converter.borrow_mut().convert_function(
                    &self.ast_context,
                    Some(ret),
                    &params,
                    is_variadic,
                )
            }
            Struct(decl_id) | Union(decl_id) | Enum(decl_id) => {
                let decl = &self.ast_context[decl_id];
                if let Some(name) = decl.kind.get_name() {
                    let resolved_name = self
                        .type_converter
                        .borrow_mut()
                        .ensure_decl_name(decl_id, name);
                    Ok(DaType::named(&resolved_name))
                } else {
                    let tn = self
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
                    let name = tn.unwrap_or_else(|| "Unnamed".into());
                    let resolved_name = self
                        .type_converter
                        .borrow_mut()
                        .ensure_decl_name(decl_id, &name);
                    Ok(DaType::named(&resolved_name))
                }
            }
            _ => Ok(DaType::auto()),
        }
    }

    pub fn convert_type_raw(&self, typ: CTypeId) -> TranslationResult<DaType> {
        self.convert_type(CQualTypeId::new(typ))
    }

    pub fn is_pointer_type(&self, typ: CTypeId) -> bool {
        matches!(
            self.ast_context.resolve_type(typ).kind,
            CTypeKind::Pointer(_)
        )
    }
}
