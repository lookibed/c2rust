//! Snapshot tests for c2dascript.
//!
//! This mirrors c2rust's integration snapshot harness, with the Rust-specific
//! compile checks replaced by daScript output snapshots.

use std::env::current_dir;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use c2dascript_transpile::renamer::DASCRIPT_KEYWORDS;
use c2dascript_transpile::TranspilerConfig;
use c2rust_rust_tools::sanitize_file_name;
use itertools::Itertools;

fn config(output_dir: &Path) -> TranspilerConfig {
    TranspilerConfig {
        dump_untyped_context: false,
        dump_typed_context: false,
        pretty_typed_context: false,
        verbose: false,
        debug_ast_exporter: false,
        filter: None,
        translate_valist: true,
        overwrite_existing: true,
        // Never write next to the checked-in fixture: the snapshot is the
        // only artefact of this test, the source tree must stay clean.
        output_dir: Some(output_dir.to_owned()),
        log_level: log::LevelFilter::Warn,
        edition: c2rust_rust_tools::RustEdition::Edition2021,
    }
}

/// Validate that the given C file compiles, then translate it with the strict
/// API into `output_dir`. Returns the generated daScript text, or the
/// translation diagnostic when the input is (intentionally) unsupported, so
/// that the diagnostic itself becomes the snapshot.
fn compile_and_transpile_file(c_path: &Path, output_dir: &Path) -> String {
    let status = std::process::Command::new("clang")
        .args([
            "-c",
            "-o",
            "/dev/null",
            "-w", // Disable warnings.
        ])
        .arg(c_path)
        .status();
    assert!(
        status.unwrap().success(),
        "clang failed on {}",
        c_path.display()
    );

    let (_temp_dir, temp_path) =
        c2dascript_transpile::create_temp_compile_commands(&[c_path.to_owned()]);
    match c2dascript_transpile::transpile_checked(config(output_dir), &temp_path, &["-w"]) {
        Ok(outputs) => {
            let das_path = outputs
                .into_iter()
                .next()
                .expect("strict translation reported success without an output path");
            fs::read_to_string(&das_path).unwrap_or_else(|error| {
                panic!("cannot read generated {}: {error}", das_path.display())
            })
        }
        Err(error) => format!("TRANSLATION FAILED\n{error}\n"),
    }
}

/// Transpile one input and compare output against the corresponding snapshot.
///
/// For outputs that vary in different environments, `platform` should contain
/// the platform-specific parts, such as `target_arch` or `target_os` or both.
fn transpile_snapshot(platform: &[&str], c_path: &Path) {
    let c_file_name = c_path.file_name().unwrap().to_str().unwrap();
    let c_file_name = sanitize_file_name(c_file_name);

    let output_dir = tempfile::tempdir().expect("temporary snapshot output directory");
    let das = compile_and_transpile_file(c_path, output_dir.path());

    let cwd = current_dir().unwrap();
    let debug_expr = format!("transpile --strict {}", c_path.display());

    // Replace real paths with placeholders for reproducible snapshots.
    let das = das
        .replace(output_dir.path().to_str().unwrap(), "<out>")
        .replace(cwd.to_str().unwrap(), ".");

    let suffix = platform.iter().copied().filter(|s| !s.is_empty()).join(".");
    let snapshot_name = if suffix.is_empty() {
        format!(
            "transpile@{}.das",
            c_path.file_stem().unwrap().to_str().unwrap()
        )
    } else {
        format!("transpile@{c_file_name}.{suffix}.das")
    };

    insta::assert_snapshot!(snapshot_name, &das, &debug_expr);
}

#[must_use]
struct TranspileTest<'a> {
    c_file_name: &'a str,
    arch_specific: bool,
    os_specific: bool,
}

fn transpile(c_file_name: &str) -> TranspileTest {
    TranspileTest {
        c_file_name,
        arch_specific: false,
        os_specific: false,
    }
}

impl<'a> TranspileTest<'a> {
    pub fn arch_specific(self, arch_specific: bool) -> Self {
        Self {
            arch_specific,
            ..self
        }
    }

    pub fn os_specific(self, os_specific: bool) -> Self {
        Self {
            os_specific,
            ..self
        }
    }

    pub fn run(self) {
        let Self {
            c_file_name,
            arch_specific,
            os_specific,
        } = self;

        let specific_dir_prefix = [arch_specific.then_some("arch"), os_specific.then_some("os")]
            .into_iter()
            .flatten()
            .join("-");
        let c_path = {
            let mut path = PathBuf::from("tests/snapshots");
            if !specific_dir_prefix.is_empty() {
                path.push(format!("{specific_dir_prefix}-specific"));
            }
            path.push(c_file_name);
            path
        };

        #[allow(unused)]
        let os = "unknown";

        #[cfg(target_os = "linux")]
        let os = "linux";
        #[cfg(target_os = "macos")]
        let os = "macos";

        #[allow(unused)]
        let arch = "unknown";

        #[cfg(target_arch = "x86")]
        let arch = "x86";
        #[cfg(target_arch = "x86_64")]
        let arch = "x86_64";
        #[cfg(target_arch = "arm")]
        let arch = "arm";
        #[cfg(target_arch = "aarch64")]
        let arch = "aarch64";

        let platform = [arch_specific.then_some(arch), os_specific.then_some(os)]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        transpile_snapshot(&platform, &c_path);
    }
}

fn generate_keywords_test() {
    // C keywords cannot be used as function names in the generated fixture.
    let c_keywords = [
        "auto",
        "break",
        "case",
        "char",
        "const",
        "continue",
        "default",
        "do",
        "double",
        "else",
        "enum",
        "extern",
        "float",
        "for",
        "goto",
        "if",
        "inline",
        "int",
        "long",
        "register",
        "restrict",
        "return",
        "short",
        "signed",
        "sizeof",
        "static",
        "struct",
        "switch",
        "typedef",
        "typeof",
        "union",
        "unsigned",
        "void",
        "volatile",
        "while",
        "_Alignas",
        "_Alignof",
        "_Atomic",
        "_Bool",
        "_Complex",
        "_Generic",
        "_Imaginary",
        "_Noreturn",
        "_Static_assert",
        "_Thread_local",
    ];
    let mut c_code = DASCRIPT_KEYWORDS
        .into_iter()
        .filter(|keyword| !c_keywords.contains(keyword))
        .map(|name| format!("void {name}(void) {{}}"))
        .join("\n\n");
    c_code.push('\n');
    // The fixture is derived from the renamer's keyword list; keep it in sync
    // but only rewrite it when the content actually changed, so an unchanged
    // run leaves the working tree untouched.
    let path = "tests/snapshots/keywords.c";
    if fs::read_to_string(path).ok().as_deref() != Some(c_code.as_str()) {
        fs_err::write(path, c_code).unwrap();
    }
}

// NOTE: Tests should be listed in alphabetical order.

#[test]
fn test_alloca() {
    transpile("alloca.c").run();
}

#[test]
fn test_arrays() {
    transpile("arrays.c").run();
}

#[test]
fn test_atomics() {
    transpile("atomics.c").run();
}

#[test]
fn test_auto_type() {
    transpile("auto_type.c").run();
}

#[test]
fn test_bitfields() {
    transpile("bitfields.c").run();
}

#[test]
fn test_bool() {
    transpile("bool.c").run();
}

#[test]
fn test_compound_literals() {
    transpile("compound_literals.c").run();
}

#[test]
fn test_empty_init() {
    transpile("empty_init.c").run();
}

#[test]
fn test_exprs() {
    transpile("exprs.c").run();
}

#[test]
fn test_factorial() {
    transpile("factorial.c").run();
}

#[test]
fn test_fn_attrs() {
    transpile("fn_attrs.c").run();
}

#[test]
fn test_frame_address() {
    transpile("frame_address.c").run();
}

#[test]
fn test_generics() {
    transpile("generics.c").run();
}

#[test]
fn test_gotos() {
    transpile("gotos.c").run();
}

#[test]
fn test_if_else() {
    transpile("if_else.c").run();
}

#[test]
fn test_incomplete_arrays() {
    transpile("incomplete_arrays.c").run();
}

#[test]
fn test_insertion() {
    transpile("insertion.c").run();
}

#[test]
fn test_keywords() {
    generate_keywords_test();
    transpile("keywords.c").run();
}

#[test]
fn test_lift_const() {
    transpile("lift_const.c").run();
}

#[test]
fn test_loops() {
    transpile("loops.c").run();
}

#[test]
fn test_macrocase() {
    transpile("macrocase.c").run();
}

#[test]
fn test_macros() {
    transpile("macros.c").run();
}

#[test]
fn test_main_fn() {
    transpile("main_fn.c").run();
}

#[test]
fn test_predefined() {
    transpile("predefined.c").run();
}

#[test]
fn test_records() {
    transpile("records.c").run();
}

#[test]
fn test_ref_ub() {
    transpile("ref_ub.c").run();
}

#[test]
fn test_return_addr_helpers() {
    transpile("return_addr_helpers.c").run();
}

#[test]
fn test_return_address() {
    transpile("return_address.c").run();
}

#[test]
fn test_rotate() {
    transpile("rotate.c").run();
}

#[test]
fn test_scalar_init() {
    transpile("scalar_init.c").run();
}

#[test]
fn test_static_assert() {
    transpile("static_assert.c").run();
}

#[test]
fn test_str_init() {
    transpile("str_init.c").run();
}

#[test]
fn test_types_compatible() {
    transpile("types_compatible.c").run();
}

#[test]
fn test_volatile() {
    transpile("volatile.c").run();
}

// arch-specific

#[test]
fn test_asm() {
    transpile("asm.c").arch_specific(true).run();
}

#[test]
fn test_spin() {
    transpile("spin.c").arch_specific(true).run();
}

#[test]
fn test_vm_x86() {
    transpile("vm_x86.c").arch_specific(true).run();
}

// os-specific

#[test]
fn test_call_only_once() {
    transpile("call_only_once.c").os_specific(true).run();
}

#[test]
fn test_f128() {
    transpile("f128.c").os_specific(true).run();
}

#[test]
fn test_irreducible() {
    transpile("irreducible.c").os_specific(true).run();
}

#[test]
fn test_macros_os_specific() {
    transpile("macros.c").os_specific(true).run();
}

#[test]
fn test_out_of_range_lit() {
    transpile("out_of_range_lit.c").os_specific(true).run();
}

#[test]
fn test_rnd() {
    transpile("rnd.c").os_specific(true).run();
}

#[test]
fn test_rotate_os_specific() {
    transpile("rotate.c").os_specific(true).run();
}

#[test]
fn test_sigign() {
    transpile("sigign.c").os_specific(true).run();
}

#[test]
fn test_switch() {
    transpile("switch.c").os_specific(true).run();
}

#[test]
fn test_typedefidx() {
    transpile("typedefidx.c").os_specific(true).run();
}

#[test]
fn test_types() {
    transpile("types.c").os_specific(true).run();
}

#[test]
fn test_wide_strings() {
    transpile("wide_strings.c").os_specific(true).run();
}

// arch-os-specific

#[test]
fn test_varargs() {
    transpile("varargs.c")
        .arch_specific(true)
        .os_specific(true)
        .run();
}
