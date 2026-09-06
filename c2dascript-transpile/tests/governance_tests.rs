use std::path::{Path, PathBuf};
use std::process::Command;

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

/// `tests/registry/fixtures.json` must describe exactly the fixture files that exist on disk.
#[test]
fn fixture_registry_is_current_and_complete() {
    let root = workspace_root();
    let output = Command::new("python3")
        .arg(root.join("scripts/check_test_registry.py"))
        .arg("--check")
        .output()
        .expect("python3 must be available to run the registry check");
    assert!(
        output.status.success(),
        "fixture registry check failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The Clang/CBOR exporter must stay a child process: an in-process ClangTool crash would take
/// the translator down with it and hide the diagnostic.
#[test]
fn clang_cbor_exporter_isolated_process_boundary_is_not_optional() {
    let root = workspace_root();
    let exporter = std::fs::read_to_string(root.join("c2rust-ast-exporter/src/lib.rs"))
        .expect("c2rust-ast-exporter Rust boundary");
    let build = std::fs::read_to_string(root.join("c2rust-ast-exporter/build.rs"))
        .expect("exporter build boundary");

    for required in [
        "ExporterFailure",
        "Command::new(&executable)",
        "exporter-timeout",
        "cbor-protocol",
        "C2DAS_AST_EXPORTER_BIN",
        "Stdio::from(stdout)",
        "read_diagnostic_file",
        "--c2das-debug",
    ] {
        assert!(
            exporter.contains(required),
            "exporter boundary missing required mechanism: {required}"
        );
    }
    for forbidden in ["fn ast_exporter(", "marshal_result(", "CLANG_MUTEX"] {
        assert!(
            !exporter.contains(forbidden),
            "in-process exporter execution is forbidden: {forbidden}"
        );
    }
    assert!(build.contains("C2RUST_AST_EXPORTER_LIB_DIR"));
    assert!(build.contains("requires C2DAS_AST_EXPORTER_BIN"));
}
