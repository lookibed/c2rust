use std::fmt;

/// Base type kind — analogous to daScript's `enum Type` in `debug_info.h`.
#[derive(Clone, Debug, PartialEq)]
pub enum DaTypeKind {
    Void,
    Bool,
    Int,
    Int8,
    Int16,
    Int64,
    UInt,
    UInt8,
    UInt16,
    UInt64,
    Float,
    Double,
    String_,
    /// Pointer type: inner type with its own qualifiers
    Pointer(Box<DaType>),
    /// Dynamic array: `array<T>`
    Array(Box<DaType>),
    /// Fixed-size array: `fixed_array<T, N>`
    FixedArray(Box<DaType>, usize),
    /// Named type reference (struct, enum, alias)
    Named(String),
    /// Auto-inferred type
    Auto,
}

/// Type declaration — wraps a [`DaTypeKind`] with qualifier flags.
/// Analogous to daScript's `TypeDecl` in `ast_typedecl.h`.
#[derive(Clone, Debug, PartialEq)]
pub struct DaType {
    pub kind: DaTypeKind,
    pub is_const: bool,
    pub is_ref: bool,
    pub is_temporary: bool,
}

impl DaType {
    pub fn new(kind: DaTypeKind) -> Self {
        DaType { kind, is_const: false, is_ref: false, is_temporary: false }
    }

    /// True for simple numeric/scalar types that can use function-style cast: `uint(expr)`.
    pub fn is_numeric(&self) -> bool {
        matches!(
            &self.kind,
            DaTypeKind::Void
                | DaTypeKind::Bool
                | DaTypeKind::Int
                | DaTypeKind::Int8
                | DaTypeKind::Int16
                | DaTypeKind::Int64
                | DaTypeKind::UInt
                | DaTypeKind::UInt8
                | DaTypeKind::UInt16
                | DaTypeKind::UInt64
                | DaTypeKind::Float
                | DaTypeKind::Double
        ) || matches!(&self.kind, DaTypeKind::Named(n) if matches!(
            // Only the C standard integer aliases belong here. A name taken
            // from one particular corpus is not a language fact and must never
            // decide how a cast is printed.
            n.as_str(),
            "size_t" | "int8_t" | "int16_t" | "int32_t" | "int64_t"
                | "uint8_t" | "uint16_t" | "uint32_t" | "uint64_t"
                | "intptr_t" | "uintptr_t" | "ptrdiff_t" | "ssize_t"
        ))
    }

    pub fn const_(mut self) -> Self {
        self.is_const = true;
        self
    }

    pub fn ref_(mut self) -> Self {
        self.is_ref = true;
        self
    }

    /// Shortcut: `DaType::int()` → `DaType { kind: Int, const: false, ref: false }`
    pub fn int() -> Self { DaType::new(DaTypeKind::Int) }
    pub fn uint() -> Self { DaType::new(DaTypeKind::UInt) }
    pub fn int8() -> Self { DaType::new(DaTypeKind::Int8) }
    pub fn uint8() -> Self { DaType::new(DaTypeKind::UInt8) }
    pub fn int16() -> Self { DaType::new(DaTypeKind::Int16) }
    pub fn uint16() -> Self { DaType::new(DaTypeKind::UInt16) }
    pub fn int64() -> Self { DaType::new(DaTypeKind::Int64) }
    pub fn uint64() -> Self { DaType::new(DaTypeKind::UInt64) }
    pub fn float() -> Self { DaType::new(DaTypeKind::Float) }
    pub fn double() -> Self { DaType::new(DaTypeKind::Double) }
    pub fn bool() -> Self { DaType::new(DaTypeKind::Bool) }
    pub fn void() -> Self { DaType::new(DaTypeKind::Void) }
    pub fn string() -> Self { DaType::new(DaTypeKind::String_) }
    pub fn auto() -> Self { DaType::new(DaTypeKind::Auto) }
    pub fn named(name: &str) -> Self { DaType::new(DaTypeKind::Named(name.to_string())) }
    pub fn pointer(inner: DaType) -> Self { DaType::new(DaTypeKind::Pointer(Box::new(inner))) }
    pub fn array(inner: DaType) -> Self { DaType::new(DaTypeKind::Array(Box::new(inner))) }
    pub fn fixed_array(inner: DaType, n: usize) -> Self {
        DaType::new(DaTypeKind::FixedArray(Box::new(inner), n))
    }
}

impl fmt::Display for DaType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.kind {
            DaTypeKind::Void => write!(f, "void"),
            DaTypeKind::Bool => write!(f, "bool"),
            DaTypeKind::Int => write!(f, "int"),
            DaTypeKind::Int8 => write!(f, "int8"),
            DaTypeKind::Int16 => write!(f, "int16"),
            DaTypeKind::Int64 => write!(f, "int64"),
            DaTypeKind::UInt => write!(f, "uint"),
            DaTypeKind::UInt8 => write!(f, "uint8"),
            DaTypeKind::UInt16 => write!(f, "uint16"),
            DaTypeKind::UInt64 => write!(f, "uint64"),
            DaTypeKind::Float => write!(f, "float"),
            DaTypeKind::Double => write!(f, "double"),
            DaTypeKind::String_ => write!(f, "string"),
            DaTypeKind::Pointer(inner) => write!(f, "{}?", inner),
            DaTypeKind::Array(inner) => write!(f, "array<{}>", inner),
            // daScript spells a fixed array `T[d0][d1]…` with the outermost
            // dimension first, exactly like C.  The nesting in the AST is
            // outermost-first too, so the dimensions are collected before the
            // element type is printed.
            DaTypeKind::FixedArray(inner, n) => {
                let mut dims = vec![*n];
                let mut element = inner.as_ref();
                while let DaTypeKind::FixedArray(next, count) = &element.kind {
                    dims.push(*count);
                    element = next.as_ref();
                }
                write!(f, "{}", element)?;
                for dim in dims {
                    write!(f, "[{}]", dim)?;
                }
                Ok(())
            }
            DaTypeKind::Named(name) => write!(f, "{}", name),
            DaTypeKind::Auto => write!(f, "auto"),
        }?;
        // Qualifiers after the type
        if self.is_ref {
            write!(f, "&")?;
        }
        // For non-pointer types, write const qualifier. Pointer const-ness is handled
        // by `var`/non-`var` on the binding (daScript doesn't support `? const` syntax).
        if self.is_const && !matches!(self.kind, DaTypeKind::Pointer(_)) {
            write!(f, " const")?;
        }
        Ok(())
    }
}
