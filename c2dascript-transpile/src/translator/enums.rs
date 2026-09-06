use super::*;
use das_ast::{DaExpr, DaType};

impl<'c> Translation<'c> {
    pub fn convert_enum(
        &self,
        enum_id: CEnumId,
        name: &Option<String>,
        variants: &[CEnumConstantId],
        integral_type: Option<CQualTypeId>,
    ) -> TranslationResult<DaDecl> {
        let raw_ename = name
            .as_ref()
            .ok_or_else(|| TranslationError::generic("anonymous enum"))?
            .clone();
        let ename = self
            .type_converter
            .borrow_mut()
            .ensure_decl_name(enum_id, &raw_ename);
        let base = match integral_type {
            Some(qt) => {
                let dt = self.convert_type(qt)?;
                match dt.kind {
                    DaTypeKind::Int
                    | DaTypeKind::UInt
                    | DaTypeKind::Int8
                    | DaTypeKind::UInt8
                    | DaTypeKind::Int16
                    | DaTypeKind::UInt16
                    | DaTypeKind::Int64
                    | DaTypeKind::UInt64 => dt,
                    _ => DaType::int(),
                }
            }
            None => DaType::int(),
        };
        let mut das_variants = vec![];
        for &vid in variants {
            if let CDeclKind::EnumConstant { ref name, value } = self.ast_context[vid].kind {
                let das_val = match value {
                    ConstIntExpr::U(v) => Some(DaExpr::ConstUInt(v)),
                    ConstIntExpr::I(v) => Some(DaExpr::ConstInt(v)),
                };
                // A C enumerator can be spelled with a daScript reserved word.
                // It must go through the same renamer as the global alias that
                // Pass 3 exports, or the two names drift apart.
                das_variants.push(DaEnumVariant {
                    name: self.declare_value_name(vid, name),
                    value: das_val,
                });
            }
        }
        Ok(DaDecl::Enumeration(DaEnumeration {
            name: ename,
            base_type: base,
            variants: das_variants,
        }))
    }

    pub fn convert_cast_from_enum(
        &self,
        target_cty: CTypeId,
        val: DaExpr,
    ) -> TranslationResult<DaExpr> {
        let ty = self.convert_type(CQualTypeId::new(target_cty))?;
        Ok(DaExpr::Cast {
            kind: das_ast::CastKind::Cast,
            expr: Box::new(val),
            to: ty,
        })
    }

}
