use crate::c_ast::*;
use crate::diagnostics::{TranslationError, TranslationResult};
use crate::translator::Translation;
use das_ast::*;
use std::cell::RefCell;

// type_kind_to_datype is defined in super::mod.rs
use super::type_kind_to_datype;

/// Backing storage for the C string literals of one translation unit.
///
/// A daScript `string` is an immutable, non-indexable value, so it cannot
/// stand in for a C `char[]` object: C code indexes it, walks it with a
/// pointer, and reads its bytes individually.  Each distinct C string literal
/// therefore becomes a module-level byte array with static storage duration,
/// exactly like the C object it translates, and the literal expression is the
/// array itself — array-to-pointer decay then yields `addr(array[0])` through
/// the ordinary decay path.
///
/// The pool lives in a thread-local because a `Translation` is built and
/// consumed on one thread inside a single `translate_impl` call; that function
/// clears the pool before a translation and drains it afterwards.
#[derive(Default)]
pub struct StringLiteralPool {
    /// Interned literals, keyed by (unit byte width, raw bytes).
    entries: Vec<((u8, Vec<u8>), String)>,
}

thread_local! {
    static STRING_LITERALS: RefCell<StringLiteralPool> = RefCell::new(StringLiteralPool::default());
}

/// daScript element type for a C string literal of the given code-unit width.
fn string_unit_type(width: u8) -> Option<DaType> {
    match width {
        1 => Some(DaType::int8()),
        2 => Some(DaType::int16()),
        4 => Some(DaType::int()),
        _ => None,
    }
}

/// Reassembles the raw literal bytes into little-endian code units of `width`
/// bytes, sign-extended into the daScript element type, and appends the NUL
/// terminator that C guarantees.
fn string_units(bytes: &[u8], width: u8) -> Vec<i64> {
    let width = width as usize;
    let mut units: Vec<i64> = bytes
        .chunks(width)
        .map(|chunk| {
            let mut raw: u64 = 0;
            for (index, byte) in chunk.iter().enumerate() {
                raw |= (*byte as u64) << (8 * index);
            }
            match width {
                1 => raw as u8 as i8 as i64,
                2 => raw as u16 as i16 as i64,
                _ => raw as u32 as i32 as i64,
            }
        })
        .collect();
    units.push(0);
    units
}

/// Floating zero in `ty`'s own precision, for lowering a C truth test on a
/// floating value. `float` and `double` zeros are spelled differently in
/// daScript, and comparing either against an integer zero would truncate.
pub fn floating_zero_for_datype(ty: &DaType) -> DaExpr {
    if matches!(ty.kind, DaTypeKind::Float) {
        DaExpr::ConstFloat(0.0)
    } else {
        DaExpr::ConstDouble(0.0)
    }
}

/// Clears the pool at the start of a translation unit.
pub fn reset_string_literals() {
    STRING_LITERALS.with(|pool| pool.borrow_mut().entries.clear());
}

/// Returns the module-level declarations backing every interned literal, in
/// the order the literals were first seen, and empties the pool.
pub fn take_string_literal_declarations() -> Vec<DaDecl> {
    STRING_LITERALS.with(|pool| {
        let entries = std::mem::take(&mut pool.borrow_mut().entries);
        entries
            .into_iter()
            .map(|((width, bytes), name)| {
                // `string_unit_type` already accepted this width when the
                // literal was interned.
                let unit_type = string_unit_type(width).unwrap_or_else(DaType::int8);
                let items = string_units(&bytes, width)
                    .into_iter()
                    .map(|unit| DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(DaExpr::ConstInt(unit)),
                        to: unit_type.clone(),
                    })
                    .collect();
                DaDecl::Variable(DaVariable {
                    name,
                    var_type: DaType::array(unit_type),
                    init: Some(DaExpr::MakeArray(items)),
                    annotations: vec![],
                })
            })
            .collect()
    })
}

/// Interns one literal and returns the name of its backing array.
fn intern_string_literal(bytes: &[u8], width: u8) -> String {
    STRING_LITERALS.with(|pool| {
        let mut pool = pool.borrow_mut();
        let key = (width, bytes.to_vec());
        if let Some((_, name)) = pool.entries.iter().find(|(entry, _)| *entry == key) {
            return name.clone();
        }
        let name = format!("c2da_str_{}", pool.entries.len());
        pool.entries.push((key, name.clone()));
        name
    })
}

impl Translation<'_> {
    pub fn convert_literal(&self, ty: CQualTypeId, lit: &CLiteral) -> TranslationResult<DaExpr> {
        let target_is_unsigned = self
            .ast_context
            .resolve_type(ty.ctype)
            .kind
            .is_unsigned_integral_type();
        // If target type maps to uint64 in daScript, wrap literal in explicit uint64() cast.
        // This ensures hex literals used in uint64 context have `uL` suffix via the Cast Display.
        let target_type = self.ast_context.resolve_type(ty.ctype).kind.clone();
        match lit {
            CLiteral::Integer(0, _) if self.is_pointer_type(ty.ctype) => self.null_for_type(ty),
            CLiteral::Integer(val, _base) => {
                let base = if target_is_unsigned || *val > 0x7FFFFFFF {
                    DaExpr::ConstUInt(*val)
                } else {
                    DaExpr::ConstInt(*val as i64)
                };
                // C integer literals acquire their type from their C use-site.
                // Preserve that contract explicitly in daScript AST instead of
                // relying on the printer's default int/uint literal spelling.
                let target_da = type_kind_to_datype(&target_type);
                if target_da.is_numeric() && !matches!(target_da.kind, DaTypeKind::Bool) {
                    Ok(DaExpr::Cast {
                        kind: das_ast::CastKind::Cast,
                        expr: Box::new(base),
                        to: target_da,
                    })
                } else {
                    Ok(base)
                }
            }
            CLiteral::Character(val) => {
                // Clang reports a character literal's value as an unsigned
                // 32-bit word. `'\xff'` on a signed-char target is -1, so the
                // word has to be read back as the signed value it encodes
                // rather than as a large positive constant.
                let value = if *val <= u32::MAX as u64 {
                    *val as u32 as i32 as i64
                } else {
                    *val as i64
                };
                Ok(DaExpr::ConstInt(value))
            }
            CLiteral::Floating(val, _) => {
                // A C floating literal's type decides its daScript spelling:
                // `float` literals are plain, `double` literals take `lf`.
                // Printing every literal as a double would make `float f =
                // 1.5f` a type error, and printing every one as a float would
                // silently truncate a double to a 24-bit mantissa.
                if matches!(target_type, CTypeKind::Float) {
                    Ok(DaExpr::ConstFloat(*val))
                } else {
                    Ok(DaExpr::ConstDouble(*val))
                }
            }
            CLiteral::String(val, width) => {
                if string_unit_type(*width).is_none() {
                    return Err(TranslationError::generic(
                        "unsupported string literal code unit width",
                    ));
                }
                Ok(DaExpr::Var(intern_string_literal(val, *width)))
            }
        }
    }
}
