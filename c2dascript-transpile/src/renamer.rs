use std::collections::HashMap;
use std::collections::HashSet;
use std::hash::Hash;

struct Scope<T> {
    name_map: HashMap<T, String>,
    used: HashSet<String>,
}

impl<T: Clone + Eq + Hash> Scope<T> {
    pub fn new() -> Self {
        Self::new_with_reserved(HashSet::new())
    }

    pub fn new_with_reserved(reserved: HashSet<String>) -> Self {
        Scope {
            name_map: HashMap::new(),
            used: reserved,
        }
    }

    pub fn insert(&mut self, key: T, val: String) {
        self.name_map.insert(key, val);
    }

    pub fn contains_key(&self, key: &T) -> bool {
        self.name_map.contains_key(key)
    }

    pub fn contains_value(&self, val: &str) -> bool {
        self.used.contains(val)
    }

    pub fn reserve(&mut self, val: String) {
        self.used.insert(val);
    }
}

// daScript keywords, taken from the gen2 lexer's keyword rules in
// `daScript/src/parser/ds2_lexer.lpp`. A C identifier that collides with one
// of these cannot be printed as-is, so the renamer must know the whole set —
// a missing entry turns into a syntax error in the generated module.
#[rustfmt::skip]
pub const DASCRIPT_KEYWORDS: &[&str] = &[
    // Keywords and built-in vector/range type names that are not in the
    // alphabetical block below.
    "abstract",
    "as",
    "byte16",
    "byte2",
    "byte3",
    "byte4",
    "byte8",
    "false",
    "float16",
    "float2",
    "float3",
    "float4",
    "half2",
    "half3",
    "half4",
    "half8",
    "include",
    "inscope",
    "int2",
    "int3",
    "int4",
    "range",
    "range64",
    "short2",
    "short3",
    "short4",
    "short8",
    "string",
    "true",
    "typeinfo",
    "ubyte16",
    "ubyte2",
    "ubyte3",
    "ubyte4",
    "ubyte8",
    "uint2",
    "uint3",
    "uint4",
    "uninitialized",
    "urange",
    "urange64",
    "ushort2",
    "ushort3",
    "ushort4",
    "ushort8",
    // Keywords
    "addr",
    "aka",
    "alias",
    "array",
    "assume",
    "block",
    "auto",
    "bitfield",
    "bool",
    "break",
    "capture",
    "case",
    "cast",
    "class",
    "const",
    "continue",
    "default",
    "def",
    "delete",
    "deref",
    "do",
    "elif",
    "else",
    "enum",
    "expect",
    "explicit",
    "export",
    "extern",
    "finally",
    "fixed_array",
    "float",
    "for",
    "function",
    "gen2",
    "generator",
    "global",
    "goto",
    "if",
    "implicit",
    "in",
    "init",
    "int",
    "int16",
    "int64",
    "int8",
    "is",
    "iterator",
    "label",
    "lambda",
    "let",
    "module",
    "new",
    "none",
    "not",
    "null",
    "operator",
    "options",
    "override",
    "param",
    "pass",
    "private",
    "public",
    "recover",
    "reinterpret",
    "require",
    "result",
    "return",
    "sealed",
    "shared",
    "sizeof",
    "smart_ptr",
    "static",
    "static_elif",
    "static_if",
    "struct",
    "table",
    "template",
    "then",
    "to",
    "try",
    "tuple",
    "type",
    "typedecl",
    "typedef",
    "uint",
    "uint16",
    "uint64",
    "uint8",
    "unsafe",
    "upcast",
    "var",
    "variant",
    "void",
    "where",
    "while",
    "with",
    "yield",
];

pub const DASCRIPT_PRELUDE_TYPE_NAMESPACE: &[&str] = &[
    // Built-in types
    "bool",
    "int",
    "int8",
    "int16",
    "int64",
    "uint",
    "uint8",
    "uint16",
    "uint64",
    "float",
    "double",
    "string",
    "void",
    "auto",
    "typeid",
    "function",
    "iterator",
    "array",
    "table",
    "fixed_array",
    "range",
    "range64",
    "urange",
    "urange64",
    "tblock",
    "block",
    "lambda",
    "tuple",
    "variant",
    "bitfield",
    "smart_ptr",
    "int2", "int3", "int4",
    "uint2", "uint3", "uint4",
    "float2", "float3", "float4",
    "double2", "double3", "double4",
    "range2", "range3", "range4",
    "half2", "half4",
    "short2", "short4",
    "ushort2", "ushort4",
    "byte16", "sbyte16",
    "float3x3", "float3x4", "float4x4",
];

#[rustfmt::skip]
pub const DASCRIPT_PRELUDE_VALUE_NAMESPACE: &[&str] = &[
    // Built-in functions and constants
    "print",
    "println",
    "debug",
    "assert",
    "sizeof",
    "typeinfo",
    "to_string",
    "int",
    "int64",
    "float",
    "double",
    "string",
    "bool",
];

pub struct Renamer<T> {
    scopes: Vec<Scope<T>>,
    next_fresh: u64,
}

impl<T: Clone + Eq + Hash> Renamer<T> {
    pub fn new(reserved_names: &[&[&str]]) -> Self {
        let set = reserved_names
            .iter()
            .flat_map(|&names| names)
            .map(|&s| s.to_owned())
            .collect::<HashSet<_>>();
        Renamer {
            scopes: vec![Scope::new_with_reserved(set)],
            next_fresh: 0,
        }
    }

    pub fn keywords() -> Self {
        // Include ALL reserved words — keywords + type names + value names.
        // block, for example, is in TYPE_NAMESPACE but not in KEYWORDS.
        Renamer::new(&[DASCRIPT_KEYWORDS, DASCRIPT_PRELUDE_TYPE_NAMESPACE, DASCRIPT_PRELUDE_VALUE_NAMESPACE])
    }

    pub fn type_namespace() -> Self {
        Renamer::new(&[DASCRIPT_KEYWORDS, DASCRIPT_PRELUDE_TYPE_NAMESPACE])
    }

    pub fn value_namespace() -> Self {
        Renamer::new(&[DASCRIPT_KEYWORDS, DASCRIPT_PRELUDE_VALUE_NAMESPACE])
    }

    pub fn global_value_namespace() -> Self {
        Renamer::new(&[
            DASCRIPT_KEYWORDS,
            DASCRIPT_PRELUDE_TYPE_NAMESPACE,
            DASCRIPT_PRELUDE_VALUE_NAMESPACE,
            &["main"],
        ])
    }

    pub fn add_scope(&mut self) {
        self.scopes.push(Scope::new())
    }

    pub fn drop_scope(&mut self) {
        if self.scopes.len() == 1 {
            panic!("Attempting to drop outermost scope")
        }
        self.scopes.pop();
    }

    fn current_scope(&self) -> &Scope<T> {
        self.scopes.last().expect("Expected a scope")
    }

    fn current_scope_mut(&mut self) -> &mut Scope<T> {
        self.scopes.last_mut().expect("Expected a scope")
    }

    fn is_target_used(&self, key: &str) -> bool {
        let key = key.to_string();
        self.scopes.iter().any(|x| x.contains_value(&key))
    }

    fn pick_name_in_scope(&mut self, basename: &str, scope: Option<usize>) -> String {
        let normalized = normalize_das_name(basename);
        let mut target = normalized.clone();
        for i in 0.. {
            if self.is_target_used(&target) {
                target = format!("{}_{}", normalized, i);
            } else {
                break;
            }
        }
        match scope {
            Some(scope_index) => self.scopes[scope_index].reserve(target.clone()),
            None => self.current_scope_mut().reserve(target.clone()),
        }
        target
    }

    pub fn pick_name(&mut self, basename: &str) -> String {
        check_c2da_name(basename);
        self.pick_name_in_scope(basename, None)
    }

    pub fn pick_name_root(&mut self, basename: &str) -> String {
        check_c2da_name(basename);
        self.pick_name_in_scope(basename, Some(0))
    }

    fn insert_in_scope(&mut self, key: T, basename: &str, scope: Option<usize>) -> Option<String> {
        let contains_key = match scope {
            Some(scope_index) => self.scopes[scope_index].contains_key(&key),
            None => self.current_scope().contains_key(&key),
        };
        if contains_key {
            return None;
        }
        let target = self.pick_name_in_scope(basename, scope);
        match scope {
            Some(scope_index) => self.scopes[scope_index].insert(key, target.clone()),
            None => self.current_scope_mut().insert(key, target.clone()),
        }
        Some(target)
    }

    pub fn insert(&mut self, key: T, basename: &str) -> Option<String> {
        self.insert_in_scope(key, basename, None)
    }

    pub fn insert_root(&mut self, key: T, basename: &str) -> Option<String> {
        self.insert_in_scope(key, basename, Some(0))
    }

    pub fn alias(&mut self, new_key: T, old_key: &T) {
        match self.get(old_key) {
            Some(name) => self.current_scope_mut().insert(new_key, name),
            None => panic!("Failed to overlap name"),
        }
    }

    pub fn get(&self, key: &T) -> Option<String> {
        for scope in self.scopes.iter().rev() {
            if let Some(target) = scope.name_map.get(key) {
                return Some(target.to_string());
            }
        }
        None
    }

    pub fn fresh(&mut self) -> String {
        let fresh = self.next_fresh;
        self.next_fresh += 1;
        self.pick_name(&format!("c2da_fresh{fresh}"))
    }
}

fn normalize_das_name(basename: &str) -> String {
    if basename.starts_with("__") {
        format!("c2da_{basename}")
    } else {
        basename.to_string()
    }
}

fn check_c2da_name(basename: &str) {
    assert!(basename.starts_with("c2da_") || basename.starts_with("C2Da_"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple() {
        let mut renamer = Renamer::new(&[&["reserved"]]);
        let one1 = renamer.insert(1, "one").unwrap();
        let one2 = renamer.get(&1).unwrap();
        assert_eq!(one1, one2);
        let reserved1 = renamer.insert(2, "reserved").unwrap();
        let reserved2 = renamer.get(&2).unwrap();
        assert_eq!(reserved1, "reserved_0");
        assert_eq!(reserved2, "reserved_0");
    }

    #[test]
    fn reserved_das_prefix() {
        let mut renamer = Renamer::new(&[]);
        assert_eq!(renamer.insert(1, "__private").unwrap(), "c2da___private");
    }

    #[test]
    fn scoped() {
        let mut renamer = Renamer::new(&[]);
        let one1 = renamer.insert(10, "one").unwrap();
        renamer.add_scope();
        let one2 = renamer.get(&10).unwrap();
        assert_eq!(one1, one2);
        let one3 = renamer.insert(20, "one").unwrap();
        let one4 = renamer.get(&20).unwrap();
        assert_eq!(one3, one4);
        assert_ne!(one3, one2);
        renamer.drop_scope();
        let one5 = renamer.get(&10).unwrap();
        assert_eq!(one5, one2);
    }

    #[test]
    fn forgets() {
        let mut renamer = Renamer::new(&[]);
        assert_eq!(renamer.get(&1), None);
        renamer.add_scope();
        renamer.insert(1, "example");
        renamer.drop_scope();
        assert_eq!(renamer.get(&1), None);
    }
}
