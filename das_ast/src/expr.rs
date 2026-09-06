use crate::DaStmt;
use crate::DaType;
use crate::DaTypeKind;
use std::fmt;

/// daScript expression. Analogous to [`syn::Expr`].
///
/// Maps to the C++ AST classes in `daScript/include/daScript/ast/ast_expressions.h`.
#[derive(Clone, Debug)]
pub enum DaExpr {
    // -- constants / literals --
    ConstInt(i64),
    ConstUInt(u64),
    ConstFloat(f64),
    ConstDouble(f64),
    ConstBool(bool),
    ConstString(String),
    ConstNull,

    // -- variable reference --
    /// `name` — maps to [`ExprVar`](ast_expressions.h:221)
    Var(String),

    // -- field access --
    /// `subexpr.name` — maps to [`ExprField`](ast_expressions.h:267)
    Field(Box<DaExpr>, String),
    /// `subexpr?.name` — maps to [`ExprSafeField`](ast_expressions.h:362)
    SafeField(Box<DaExpr>, String),

    // -- index --
    /// `subexpr[index]` — maps to [`ExprAt`](ast_expressions.h:128)
    Index(Box<DaExpr>, Box<DaExpr>),
    /// `subexpr?[index]` — maps to [`ExprSafeAt`](ast_expressions.h:152)
    SafeIndex(Box<DaExpr>, Box<DaExpr>),

    // -- unary operators --
    /// Maps to [`ExprOp1`](ast_expressions.h:436), op in {"-", "!", "~"}
    Op1 {
        op: &'static str,
        expr: Box<DaExpr>,
    },

    // -- binary operators --
    /// Maps to [`ExprOp2`](ast_expressions.h:453)
    /// op in {"+", "-", "*", "/", "%", "==", "!=", "<", ">", "<=", ">=",
    /// "&&", "||", "&", "|", "^", "<<", ">>", "++", "--"}
    Op2 {
        op: &'static str,
        left: Box<DaExpr>,
        right: Box<DaExpr>,
    },

    // -- ternary (да, это if-then-else как выражение) --
    /// Maps to [`ExprOp3`](ast_expressions.h:530) — daScript не имеет ?: синтаксиса
    Op3 {
        cond: Box<DaExpr>,
        then: Box<DaExpr>,
        else_: Box<DaExpr>,
    },

    // -- assignment --
    /// `left = right` — maps to [`ExprCopy`](ast_expressions.h:470)
    Assign(Box<DaExpr>, Box<DaExpr>),

    // -- compound assignment --
    /// `left op= right`, op in {"+=", "-=", "*=", "/=", ...}
    AssignOp {
        op: &'static str,
        left: Box<DaExpr>,
        right: Box<DaExpr>,
    },

    // -- pipe --
    /// `left |> right` — maps to rpipe expression
    Pipe(Box<DaExpr>, Box<DaExpr>),

    // -- call --
    /// `func(args...)` — maps to [`ExprCall`](ast_expressions.h:1307)
    Call(Box<DaExpr>, Vec<DaExpr>),

    // -- block --
    /// `{ stmts }` — maps to [`ExprBlock`](ast_expressions.h:165)
    Block(DaBlock),

    // -- control flow --
    /// `if (cond) then [else else_]` — maps to [`ExprIfThenElse`](ast_expressions.h:1326)
    IfThenElse {
        cond: Box<DaExpr>,
        then: Box<DaExpr>,
        elifs: Vec<(DaExpr, DaExpr)>,
        else_: Option<Box<DaExpr>>,
    },

    /// `while (cond) body` — maps to [`ExprWhile`](ast_expressions.h:975)
    While(Box<DaExpr>, Box<DaExpr>),

    /// `for (vars in sources) body` — maps to [`ExprFor`](ast_expressions.h:937)
    For {
        vars: Vec<String>,
        sources: Vec<DaExpr>,
        body: Box<DaExpr>,
    },

    // -- jump --
    /// `return [value]` — maps to [`ExprReturn`](ast_expressions.h:559)
    Return(Option<Box<DaExpr>>),
    /// `break` — maps to [`ExprBreak`](ast_expressions.h:594)
    Break,
    /// `continue` — maps to [`ExprContinue`](ast_expressions.h:605)
    Continue,
    /// `goto label` — maps to [`ExprGoto`](ast_expressions.h:34)
    Goto(String),
    /// `label:` — maps to [`ExprLabel`](ast_expressions.h:21)
    Label(String),

    // -- function values --
    /// `@@name` — a reference to a named function as a `function<…>` value.
    /// Maps to [`ExprAddr`](ast_expressions.h:88) over a function; it is a
    /// distinct node because `@@` is an operator, not part of the identifier.
    FuncRef(String),
    /// `default<T>` — the zero value of `T`. The only way to spell a null
    /// `function<…>`, which does not accept `null` itself.
    DefaultValue(DaType),

    // -- casts --
    /// `cast<T>(expr)`, `reinterpret<T>(expr)`, `upcast<T>(expr)`
    /// — maps to [`ExprCast`](ast_expressions.h:1275)
    Cast {
        kind: CastKind,
        expr: Box<DaExpr>,
        to: DaType,
    },

    // -- new / delete --
    /// `new Type(args)` — maps to [`ExprNew`](ast_expressions.h:1295)
    New(Box<DaExpr>, Vec<DaExpr>),
    /// `delete expr` — maps to [`ExprDelete`](ast_expressions.h:115)
    Delete(Box<DaExpr>),

    // -- addr / deref --
    /// `addr(expr)` — maps to [`ExprAddr`](ast_expressions.h:88)
    Addr(Box<DaExpr>),
    /// `*expr` — pointer dereference
    Deref(Box<DaExpr>),
    /// `deref(expr)` — explicit dereference
    DerefExplicit(Box<DaExpr>),

    // -- unsafe --
    /// `unsafe { expr }` — maps to [`ExprUnsafe`](ast_expressions.h:962)
    Unsafe(Box<DaExpr>),

    // -- struct literal --
    /// `Type(field=val, ...)` — maps to [`ExprMakeStruct`](ast_expressions.h:1422)
    MakeStruct {
        type_name: String,
        fields: Vec<(String, DaExpr)>,
    },

    // -- array literal --
    /// `[a, b, c]` — maps to [`ExprMakeArray`](ast_expressions.h:1469)
    MakeArray(Vec<DaExpr>),

    /// `fixed_array<T>(a, b, c)` — a fixed-size array value with inline
    /// storage, the daScript form of a C array object.  Distinct from
    /// [`MakeArray`], which builds a heap `array<T>`.
    MakeFixedArray {
        elem_type: DaType,
        items: Vec<DaExpr>,
    },

    // -- typeinfo --
    /// `typeinfo trait_name(type<T>)` — maps to [`ExprTypeInfo`](ast_expressions.h:1222)
    TypeInfo {
        trait_name: String,
        type_arg: Box<DaType>,
    },
}

/// Cast kind, maps to daScript's `cast`, `reinterpret`, `upcast`.
#[derive(Clone, Debug, PartialEq)]
pub enum CastKind {
    Cast,
    Reinterpret,
    Upcast,
}

/// Block expression — `{ stmts }`.
#[derive(Clone, Debug)]
pub struct DaBlock {
    pub stmts: Vec<DaStmt>,
}

impl DaBlock {
    pub fn new() -> Self {
        DaBlock { stmts: vec![] }
    }
}

// ── Display implementations ──────────────────────────────────────────

fn write_block(f: &mut fmt::Formatter, block: &DaBlock, indent: usize) -> fmt::Result {
    writeln!(f, "{{")?;
    for stmt in &block.stmts {
        write_indent(f, indent + 1)?;
        stmt.fmt_with_indent(f, indent + 1)?;
    }
    write_indent(f, indent)?;
    write!(f, "}}")
}

fn write_indent(f: &mut fmt::Formatter, level: usize) -> fmt::Result {
    for _ in 0..level {
        write!(f, "    ")?;
    }
    Ok(())
}

// ── Precedence ───────────────────────────────────────────────────────
//
// The levels below mirror the daScript gen2 grammar exactly; see the
// precedence declarations in `daScript/src/parser/ds2_parser.ypp`.  A larger
// number binds tighter.  Parenthesisation is derived from these levels alone:
// the printer never inspects, rewrites, or pattern-matches printed text.

/// Statement-shaped expressions (`if`, `while`, `return`, …). They are never
/// valid as an operand, so they are always parenthesised when nested.
const PREC_STMT: u8 = 0;
/// `=`, `+=`, `-=`, … — right associative.
const PREC_ASSIGN: u8 = 3;
/// `? :` — right associative.
const PREC_TERNARY: u8 = 4;
const PREC_OROR: u8 = 5;
const PREC_ANDAND: u8 = 7;
const PREC_OR: u8 = 8;
const PREC_XOR: u8 = 9;
const PREC_AND: u8 = 10;
const PREC_EQ: u8 = 11;
const PREC_REL: u8 = 12;
/// `<<`, `>>`, `<<<`, `>>>`.
const PREC_SHIFT: u8 = 13;
const PREC_ADD: u8 = 14;
const PREC_MUL: u8 = 15;
/// `??` (null coalescing) — right associative.
const PREC_QQ: u8 = 16;
/// Prefix `-`, `+`, `~`, `!` — right associative.
const PREC_UNARY: u8 = 17;
/// `|>`, `<|`.
const PREC_PIPE: u8 = 19;
/// Prefix `*` (pointer dereference).
const PREC_DEREF: u8 = 20;
/// Postfix chain: `.`, `?.`, `[]`, `?[]`, `()`.
const PREC_POSTFIX: u8 = 21;
/// Self-delimiting forms: literals, names, `f(x)`-shaped constructs, blocks.
const PREC_ATOM: u8 = 22;

/// Associativity of a binary operator, needed to decide whether an operand of
/// equal precedence has to be parenthesised.
#[derive(Clone, Copy, PartialEq)]
enum Assoc {
    Left,
    Right,
}

/// Precedence and associativity of a daScript binary operator.
fn binary_op_info(op: &str) -> (u8, Assoc) {
    match op {
        "||" => (PREC_OROR, Assoc::Left),
        "&&" => (PREC_ANDAND, Assoc::Left),
        "|" => (PREC_OR, Assoc::Left),
        "^" => (PREC_XOR, Assoc::Left),
        "&" => (PREC_AND, Assoc::Left),
        "==" | "!=" => (PREC_EQ, Assoc::Left),
        "<" | ">" | "<=" | ">=" => (PREC_REL, Assoc::Left),
        "<<" | ">>" | "<<<" | ">>>" => (PREC_SHIFT, Assoc::Left),
        "+" | "-" => (PREC_ADD, Assoc::Left),
        "*" | "/" | "%" => (PREC_MUL, Assoc::Left),
        "??" => (PREC_QQ, Assoc::Right),
        // An operator the AST invents but the grammar does not know about must
        // never be printed unparenthesised on a guess.
        _ => (PREC_STMT, Assoc::Left),
    }
}

/// Printed precedence of a whole expression node.
fn expr_precedence(expr: &DaExpr) -> u8 {
    match expr {
        // A negative numeric literal prints with a leading `-`, so it binds
        // exactly like a unary expression rather than like an atom.
        DaExpr::ConstInt(n) => {
            if *n < 0 {
                PREC_UNARY
            } else {
                PREC_ATOM
            }
        }
        DaExpr::ConstFloat(n) | DaExpr::ConstDouble(n) => {
            if n.is_sign_negative() {
                PREC_UNARY
            } else {
                PREC_ATOM
            }
        }
        DaExpr::Op2 { op, .. } => binary_op_info(op).0,
        DaExpr::Assign(_, _) | DaExpr::AssignOp { .. } => PREC_ASSIGN,
        DaExpr::Op3 { .. } => PREC_TERNARY,
        DaExpr::Op1 { .. } | DaExpr::New(_, _) => PREC_UNARY,
        DaExpr::Pipe(_, _) => PREC_PIPE,
        DaExpr::Deref(_) => PREC_DEREF,
        DaExpr::Field(_, _)
        | DaExpr::SafeField(_, _)
        | DaExpr::Index(_, _)
        | DaExpr::SafeIndex(_, _)
        | DaExpr::Call(_, _) => PREC_POSTFIX,
        // `unsafe { … }` is a block statement, not an operand; the call-shaped
        // `unsafe(expr)` form is an atom.
        DaExpr::Unsafe(inner) => {
            if matches!(**inner, DaExpr::Block(_)) {
                PREC_STMT
            } else {
                PREC_ATOM
            }
        }
        DaExpr::IfThenElse { .. }
        | DaExpr::While(_, _)
        | DaExpr::For { .. }
        | DaExpr::Return(_)
        | DaExpr::Break
        | DaExpr::Continue
        | DaExpr::Goto(_)
        | DaExpr::Label(_)
        | DaExpr::Delete(_) => PREC_STMT,
        _ => PREC_ATOM,
    }
}

/// Writes `expr` as an operand that must bind at least as tightly as
/// `min_prec`, adding parentheses when it does not.
fn write_operand(f: &mut fmt::Formatter, expr: &DaExpr, min_prec: u8) -> fmt::Result {
    if expr_precedence(expr) < min_prec {
        write!(f, "({})", expr)
    } else {
        write!(f, "{}", expr)
    }
}

/// Writes one operand of a binary operator. The operand on the associativity
/// side may share the operator's precedence; the other side may not.
fn write_binary_operand(
    f: &mut fmt::Formatter,
    operand: &DaExpr,
    op: &str,
    is_right: bool,
) -> fmt::Result {
    let (prec, assoc) = binary_op_info(op);
    let keeps_precedence = match assoc {
        Assoc::Left => !is_right,
        Assoc::Right => is_right,
    };
    let min_prec = if keeps_precedence { prec } else { prec + 1 };
    write_operand(f, operand, min_prec)
}

// ── Literal spelling ─────────────────────────────────────────────────

/// Escapes a daScript string constant. daScript unescapes `\"`, `\\`, `\n`,
/// `\r`, `\t`, `\b`, `\f`, `\v`, `\{`, `\}` and `\xNN`; braces additionally
/// open string interpolation and must always be escaped.
fn write_escaped_string(f: &mut fmt::Formatter, value: &str) -> fmt::Result {
    write!(f, "\"")?;
    for ch in value.chars() {
        match ch {
            '"' => write!(f, "\\\"")?,
            '\\' => write!(f, "\\\\")?,
            '\n' => write!(f, "\\n")?,
            '\r' => write!(f, "\\r")?,
            '\t' => write!(f, "\\t")?,
            '{' => write!(f, "\\{{")?,
            '}' => write!(f, "\\}}")?,
            c if (c as u32) < 0x20 || (c as u32) == 0x7f => write!(f, "\\x{:02x}", c as u32)?,
            c => write!(f, "{}", c)?,
        }
    }
    write!(f, "\"")
}

/// Spells an `int`/`int64` constant. daScript reads a plain decimal literal as
/// `int` and an `l`-suffixed one as `int64`; `i64::MIN` has no direct spelling
/// because the lexer range-checks the unsigned magnitude first.
fn write_signed_literal(f: &mut fmt::Formatter, value: i64) -> fmt::Result {
    if value >= i32::MIN as i64 && value <= i32::MAX as i64 {
        write!(f, "{}", value)
    } else if value == i64::MIN {
        write!(f, "(-9223372036854775807l - 1l)")
    } else {
        write!(f, "{}l", value)
    }
}

/// Spells a `uint`/`uint64` constant. daScript reads `0xHHHHHHHH` as `uint`
/// and `0xH…uL` as `uint64`.
fn write_unsigned_literal(f: &mut fmt::Formatter, value: u64) -> fmt::Result {
    if value <= u32::MAX as u64 {
        write!(f, "0x{:x}", value)
    } else {
        write!(f, "0x{:x}uL", value)
    }
}

/// Shortest decimal form that round-trips back to the same binary value.
/// Rust's `Debug` formatting for floats guarantees a `.`- or exponent-bearing
/// spelling, which is exactly what daScript needs to lex a real constant.
fn float_repr(value: f64) -> String {
    let text = format!("{:?}", value);
    if text.contains('.') || text.contains('e') || text.contains('E') || text.contains("inf")
        || text.contains("NaN")
    {
        text
    } else {
        format!("{}.0", text)
    }
}

impl DaExpr {
    pub(crate) fn fmt_with_indent(&self, f: &mut fmt::Formatter, indent: usize) -> fmt::Result {
        use DaExpr::*;
        match self {
            ConstInt(n) => write_signed_literal(f, *n),
            ConstUInt(n) => write_unsigned_literal(f, *n),
            ConstFloat(n) => {
                // A daScript real literal without a suffix is `float`.
                write!(f, "{}", float_repr(*n as f32 as f64))
            }
            ConstDouble(n) => {
                // A daScript `double` literal carries the `lf` suffix. Without
                // it the literal is lexed as `float` and only then widened,
                // which would silently truncate the mantissa to 24 bits.
                write!(f, "{}lf", float_repr(*n))
            }
            ConstBool(b) => write!(f, "{}", b),
            ConstString(s) => write_escaped_string(f, s),
            ConstNull => write!(f, "null"),

            Var(name) => write!(f, "{}", name),

            Field(obj, name) => {
                write_operand(f, obj, PREC_POSTFIX)?;
                write!(f, ".{}", name)
            }
            SafeField(obj, name) => {
                write_operand(f, obj, PREC_POSTFIX)?;
                write!(f, "?.{}", name)
            }

            Index(arr, idx) => {
                write_operand(f, arr, PREC_POSTFIX)?;
                write!(f, "[{}]", idx)
            }
            SafeIndex(arr, idx) => {
                write_operand(f, arr, PREC_POSTFIX)?;
                write!(f, "?[{}]", idx)
            }

            Op1 { op, expr } => {
                // Prefix operators are right associative, so an operand that is
                // itself unary still needs parentheses: `- -a` would otherwise
                // print as `--a`, which lexes as a decrement.
                write!(f, "{}", op)?;
                write_operand(f, expr, PREC_UNARY + 1)
            }

            Op2 { op, left, right } => {
                write_binary_operand(f, left, op, false)?;
                write!(f, " {} ", op)?;
                write_binary_operand(f, right, op, true)
            }

            Op3 { cond, then, else_ } => {
                // daScript spells the conditional expression `c ? a : b`; it is
                // right associative and binds just above assignment.
                write_operand(f, cond, PREC_TERNARY + 1)?;
                write!(f, " ? ")?;
                write_operand(f, then, PREC_TERNARY + 1)?;
                write!(f, " : ")?;
                write_operand(f, else_, PREC_TERNARY)
            }

            Assign(left, right) => {
                write_operand(f, left, PREC_ASSIGN + 1)?;
                write!(f, " = ")?;
                write_operand(f, right, PREC_ASSIGN)
            }

            AssignOp { op, left, right } => {
                write_operand(f, left, PREC_ASSIGN + 1)?;
                write!(f, " {} ", op)?;
                write_operand(f, right, PREC_ASSIGN)
            }

            Pipe(left, right) => {
                write_operand(f, left, PREC_PIPE)?;
                write!(f, " |> ")?;
                write_operand(f, right, PREC_PIPE + 1)
            }

            Call(func, args) => {
                write_operand(f, func, PREC_POSTFIX)?;
                let args_str: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                write!(f, "({})", args_str.join(", "))
            }

            Block(block) => write_block(f, block, indent),

            IfThenElse {
                cond,
                then,
                elifs,
                else_,
            } => {
                write!(f, "if ({}) ", cond)?;
                then.fmt_with_indent(f, indent)?;
                for (elif_cond, elif_body) in elifs {
                    write!(f, " elif ({}) ", elif_cond)?;
                    elif_body.fmt_with_indent(f, indent)?;
                }
                let mut tail = else_.as_deref();
                while let Some(DaExpr::IfThenElse {
                    cond,
                    then,
                    elifs,
                    else_,
                }) = tail
                {
                    write!(f, " elif ({}) ", cond)?;
                    then.fmt_with_indent(f, indent)?;
                    for (elif_cond, elif_body) in elifs {
                        write!(f, " elif ({}) ", elif_cond)?;
                        elif_body.fmt_with_indent(f, indent)?;
                    }
                    tail = else_.as_deref();
                }
                if let Some(else_body) = tail {
                    write!(f, " else ")?;
                    else_body.fmt_with_indent(f, indent)?;
                }
                Ok(())
            }

            While(cond, body) => {
                write!(f, "while ({}) ", cond)?;
                body.fmt_with_indent(f, indent)
            }

            For {
                vars,
                sources,
                body,
            } => {
                let vars_str = vars.join(", ");
                let srcs_str: Vec<String> = sources.iter().map(|s| format!("{}", s)).collect();
                write!(
                    f,
                    "for ({v} in {s}) ",
                    v = vars_str,
                    s = srcs_str.join(", ")
                )?;
                body.fmt_with_indent(f, indent)
            }

            Return(None) => write!(f, "return"),
            Return(Some(val)) => write!(f, "return {}", val),

            Break => write!(f, "break"),
            Continue => write!(f, "continue"),
            Goto(label) => write!(f, "goto {}", label),
            Label(label) => write!(f, "{}:", label),

            FuncRef(name) => write!(f, "@@{}", name),
            DefaultValue(ty) => write!(f, "default<{}>", ty),

            Cast { kind, expr, to } => {
                // For primitive types, use function-style cast: `uint(expr)` instead of `cast<uint>(expr)`.
                // This includes numeric types (int, uint64, size_t) and named types (enums, typedefs).
                // daScript `cast<T>` preserves const on source, causing `can't cast int const to uint64`.
                // Function-style calls (`uint(expr)`) accept const args — they're regular function calls.
                // Named types are constructible if they're enums or numeric typedefs.
                if *kind == CastKind::Cast && matches!(&to.kind, DaTypeKind::Named(_)) {
                    write!(f, "unsafe(reinterpret<{}>({}))", to, expr)
                } else if *kind == CastKind::Cast && to.is_numeric() {
                    write!(f, "{}({})", to, expr)
                } else if *kind == CastKind::Reinterpret || *kind == CastKind::Upcast {
                    // reinterpret/upcast require `unsafe()` in daScript
                    let kw = match kind {
                        CastKind::Reinterpret => "reinterpret",
                        CastKind::Upcast => "upcast",
                        _ => unreachable!(),
                    };
                    write!(f, "unsafe({}<{}>({}))", kw, to, expr)
                } else {
                    let kw = match kind {
                        CastKind::Cast => "cast",
                        _ => unreachable!(),
                    };
                    write!(f, "{}<{}>({})", kw, to, expr)
                }
            }

            New(type_expr, args) => {
                let args_str: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                write!(f, "new {}({})", type_expr, args_str.join(", "))
            }

            Delete(expr) => write!(f, "delete {}", expr),

            Addr(expr) => write!(f, "addr({})", expr),
            Deref(expr) => {
                // `*` binds looser than the postfix chain, so `*p.f` means
                // `*(p.f)`; the operand of a deref must therefore bind at
                // least as tightly as `*` itself.
                write!(f, "*")?;
                write_operand(f, expr, PREC_DEREF)
            }
            DerefExplicit(expr) => write!(f, "deref({})", expr),

            Unsafe(expr) => {
                match &**expr {
                    DaExpr::Block(b) => {
                        // Block form: unsafe { stmts }
                        writeln!(f, "unsafe {{")?;
                        for stmt in &b.stmts {
                            write_indent(f, indent + 1)?;
                            stmt.fmt_with_indent(f, indent + 1)?;
                        }
                        write_indent(f, indent)?;
                        write!(f, "}}")
                    }
                    e => write!(f, "unsafe({})", e),
                }
            }

            MakeStruct { type_name, fields } => {
                let fields_str: Vec<String> = fields
                    .iter()
                    .map(|(name, val)| format!("{} = {}", name, val))
                    .collect();
                write!(f, "{}({})", type_name, fields_str.join(", "))
            }

            MakeArray(items) => {
                let items_str: Vec<String> = items.iter().map(|i| format!("{}", i)).collect();
                write!(f, "[{}]", items_str.join(", "))
            }

            MakeFixedArray { elem_type, items } => {
                let items_str: Vec<String> = items.iter().map(|i| format!("{}", i)).collect();
                write!(f, "fixed_array<{}>({})", elem_type, items_str.join(", "))
            }

            TypeInfo {
                trait_name,
                type_arg,
            } => {
                write!(f, "typeinfo {}(type<{}>)", trait_name, type_arg)
            }
        }
    }
}

impl fmt::Display for DaExpr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}

impl fmt::Display for DaBlock {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write_block(f, self, 0)
    }
}
