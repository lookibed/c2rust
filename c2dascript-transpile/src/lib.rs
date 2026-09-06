#![allow(clippy::too_many_arguments)]

mod diagnostics;

pub mod build_files;
pub mod c_ast;
pub mod cfg;
mod compile_cmds;
pub mod convert_type;
pub mod renamer;
pub mod translator;
pub mod with_stmts;

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use log::warn;
use regex::Regex;
pub use tempfile::TempDir;

use crate::c_ast::*;
pub use crate::diagnostics::Diagnostic;
use crate::diagnostics::TranslationError;
use c2rust_ast_exporter as ast_exporter;

use crate::compile_cmds::get_compile_commands;
use std::prelude::v1::Vec;

/// Failure produced by the translation API.
///
/// [`transpile`] continues past a failed translation unit so the rest of a
/// compilation database is still processed, while [`transpile_checked`] stops
/// at the first one; both report every failure, and neither writes an output
/// file for a unit that failed. An unsupported C construct can therefore never
/// be mistaken for a successfully printed partial module.
#[derive(Debug)]
pub enum TranspileError {
    CompileCommands(String),
    MissingInput(PathBuf),
    ClangAst(ast_exporter::ExporterFailure),
    Translation(TranslationError),
    Output {
        path: PathBuf,
        error: std::io::Error,
    },
}

impl std::fmt::Display for TranspileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CompileCommands(error) => write!(f, "compile_commands: {error}"),
            Self::MissingInput(path) => {
                write!(f, "input C file does not exist: {}", path.display())
            }
            Self::ClangAst(error) => write!(f, "Clang AST export: {error}"),
            Self::Translation(error) => write!(f, "{error}"),
            Self::Output { path, error } => write!(f, "cannot write {}: {error}", path.display()),
        }
    }
}

impl std::error::Error for TranspileError {}

type PragmaVec = Vec<(&'static str, Vec<&'static str>)>;
type PragmaSet = indexmap::IndexSet<(&'static str, &'static str)>;
type CrateSet = indexmap::IndexSet<ExternCrate>;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ExternCrate {
    C2RustBitfields,
    C2RustAsmCasts,
    F128,
    NumTraits,
    Memoffset,
    Libc,
}

/// Configuration settings for the translation process
#[derive(Debug)]
pub struct TranspilerConfig {
    pub dump_untyped_context: bool,
    pub dump_typed_context: bool,
    pub pretty_typed_context: bool,
    pub verbose: bool,
    pub debug_ast_exporter: bool,
    pub filter: Option<Regex>,
    pub translate_valist: bool,
    pub overwrite_existing: bool,
    pub output_dir: Option<PathBuf>,
    pub log_level: log::LevelFilter,
    pub edition: c2rust_rust_tools::RustEdition,
}

/// AST-level inventory for target-specific C surfaces.  These counts are
/// taken after Clang CBOR has become the typed C AST, so they cannot be
/// confused with comments, disabled preprocessor branches, or source text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AsmSimdInventory {
    pub inline_asm: usize,
    pub shuffle_vector: usize,
    pub convert_vector: usize,
    pub vector_type: usize,
}

pub fn inventory_asm_simd(
    tcfg: &TranspilerConfig,
    cc_db: &Path,
    extra_clang_args: &[&str],
) -> Result<AsmSimdInventory, String> {
    let lcmds = get_compile_commands(cc_db, &tcfg.filter).map_err(|err| err.to_string())?;
    let mut inventory = AsmSimdInventory::default();
    for lcmd in &lcmds {
        for cmd in &lcmd.cmd_inputs {
            let input_path = cmd.abs_file();
            let untyped = ast_exporter::get_untyped_ast(
                &input_path,
                cc_db,
                extra_clang_args,
                tcfg.debug_ast_exporter,
            )
            .map_err(|err| format!("{}: {err}", input_path.display()))?;
            let typed = ConversionContext::new(&input_path, &untyped).into_typed_context();
            inventory.inline_asm += typed
                .iter_stmts()
                .filter(|(_, stmt)| matches!(stmt.kind, CStmtKind::Asm { .. }))
                .count();
            inventory.shuffle_vector += typed
                .iter_exprs()
                .filter(|(_, expr)| matches!(expr.kind, CExprKind::ShuffleVector(..)))
                .count();
            inventory.convert_vector += typed
                .iter_exprs()
                .filter(|(_, expr)| matches!(expr.kind, CExprKind::ConvertVector(..)))
                .count();
            inventory.vector_type += typed
                .iter_types()
                .filter(|(_, ty)| matches!(ty.kind, CTypeKind::Vector(..)))
                .count();
        }
    }
    Ok(inventory)
}

impl Default for TranspilerConfig {
    fn default() -> Self {
        TranspilerConfig {
            dump_untyped_context: false,
            dump_typed_context: false,
            pretty_typed_context: false,
            verbose: false,
            debug_ast_exporter: false,
            filter: None,
            translate_valist: false,
            overwrite_existing: false,
            output_dir: None,
            log_level: log::LevelFilter::Warn,
            edition: c2rust_rust_tools::RustEdition::Edition2021,
        }
    }
}

pub fn create_temp_compile_commands(sources: &[PathBuf]) -> (TempDir, PathBuf) {
    let temp_dir = tempfile::Builder::new()
        .prefix("c2dascript-")
        .tempdir()
        .expect("Failed to create temporary directory");
    let temp_path = temp_dir.path().join("compile_commands.json");
    let compile_commands: Vec<CompileCmd> = sources
        .iter()
        .map(|source_file| {
            let absolute_path = fs::canonicalize(source_file)
                .unwrap_or_else(|_| panic!("Could not canonicalize {}", source_file.display()));
            CompileCmd {
                directory: PathBuf::from("."),
                file: absolute_path.clone(),
                arguments: vec![
                    "clang".to_string(),
                    absolute_path.to_str().unwrap().to_owned(),
                ],
                command: None,
                output: None,
            }
        })
        .collect();
    let json_content = serde_json::to_string(&compile_commands).unwrap();
    let mut file =
        File::create(&temp_path).expect("Failed to create temporary compile_commands.json");
    file.write_all(json_content.as_bytes())
        .expect("Failed to write to temporary compile_commands.json");
    (temp_dir, temp_path)
}

/// Translate every selected command, continuing past a failed translation unit
/// so the rest of a compilation database is still processed, and report every
/// failure to the caller.
///
/// This is the permissive counterpart of [`transpile_checked`]: it differs only
/// in *when* it stops, never in what it accepts. A translation unit that cannot
/// be lowered produces no output file and is returned here as an error, so a
/// caller can never mistake a skipped unit for a successful one.
pub fn transpile(
    tcfg: TranspilerConfig,
    cc_db: &Path,
    extra_clang_args: &[&str],
) -> Result<Vec<PathBuf>, Vec<TranspileError>> {
    diagnostics::init(HashSet::new(), tcfg.log_level);

    let lcmds = match get_compile_commands(cc_db, &tcfg.filter) {
        Ok(l) => l,
        Err(e) => {
            return Err(vec![TranspileError::CompileCommands(e.to_string())]);
        }
    };

    let mut outputs = Vec::new();
    let mut failures = Vec::new();
    for lcmd in &lcmds {
        for cmd in &lcmd.cmd_inputs {
            match transpile_single_checked(&tcfg, &cmd.abs_file(), cc_db, extra_clang_args) {
                Ok(path) => outputs.push(path),
                Err(error) => {
                    warn!("Failed to transpile {}", cmd.abs_file().display());
                    failures.push(error);
                }
            }
        }
    }
    if failures.is_empty() {
        Ok(outputs)
    } else {
        Err(failures)
    }
}

/// Translate every selected command and return every output path, failing on
/// the first unsupported user declaration. This is the canonical API for
/// fixture assertions and executable cases; it never writes next to a source
/// file when [`TranspilerConfig::output_dir`] is set.
pub fn transpile_checked(
    tcfg: TranspilerConfig,
    cc_db: &Path,
    extra_clang_args: &[&str],
) -> Result<Vec<PathBuf>, TranspileError> {
    diagnostics::init(HashSet::new(), tcfg.log_level);
    let lcmds = get_compile_commands(cc_db, &tcfg.filter)
        .map_err(|error| TranspileError::CompileCommands(error.to_string()))?;
    let mut outputs = Vec::new();
    for lcmd in &lcmds {
        for cmd in &lcmd.cmd_inputs {
            outputs.push(transpile_single_checked(
                &tcfg,
                &cmd.abs_file(),
                cc_db,
                extra_clang_args,
            )?);
        }
    }
    Ok(outputs)
}

fn output_path_for(tcfg: &TranspilerConfig, input_path: &Path) -> Result<PathBuf, TranspileError> {
    if let Some(output_dir) = &tcfg.output_dir {
        fs::create_dir_all(output_dir).map_err(|error| TranspileError::Output {
            path: output_dir.clone(),
            error,
        })?;
        let filename = input_path
            .file_name()
            .ok_or_else(|| TranspileError::MissingInput(input_path.to_path_buf()))?;
        return Ok(output_dir.join(filename).with_extension("das"));
    }
    Ok(input_path.with_extension("das"))
}

fn transpile_single_checked(
    tcfg: &TranspilerConfig,
    input_path: &Path,
    cc_db: &Path,
    extra_clang_args: &[&str],
) -> Result<PathBuf, TranspileError> {
    let file = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    if !input_path.exists() {
        warn!(
            "Input C file {} does not exist, skipping!",
            input_path.display()
        );
        return Err(TranspileError::MissingInput(input_path.to_path_buf()));
    }

    println!("Transpiling {}", file);

    let untyped_context = match ast_exporter::get_untyped_ast(
        input_path,
        cc_db,
        extra_clang_args,
        tcfg.debug_ast_exporter,
    ) {
        Err(e) => {
            warn!(
                "Error: {}. Skipping {}; is it well-formed C?",
                e,
                input_path.display()
            );
            return Err(TranspileError::ClangAst(e));
        }
        Ok(cxt) => cxt,
    };

    let typed_context = {
        let conv = ConversionContext::new(input_path, &untyped_context);
        conv.into_typed_context()
    };

    let (das_code, _maybe_decl_map, _pragmas, _crates) =
        translator::translate_checked(typed_context, tcfg, input_path)
            .map_err(TranspileError::Translation)?;

    let output_path = output_path_for(tcfg, input_path)?;
    let mut file = File::create(&output_path).map_err(|error| TranspileError::Output {
        path: output_path.clone(),
        error,
    })?;
    file.write_all(das_code.as_bytes())
        .map_err(|error| TranspileError::Output {
            path: output_path.clone(),
            error,
        })?;

    println!("Wrote {}", output_path.display());
    Ok(output_path)
}

use crate::compile_cmds::CompileCmd;
