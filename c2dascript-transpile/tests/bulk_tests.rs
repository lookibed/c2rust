use std::path::Path;

/// Find all C test files from tests/unit/*/src/
fn find_c_files() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/unit");
    let mut files = vec![];
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let dir = entry.path();
            let src_dir = dir.join("src");
            if src_dir.is_dir() {
                if let Ok(src_entries) = std::fs::read_dir(&src_dir) {
                    for src_entry in src_entries.flatten() {
                        let path = src_entry.path();
                        if path.extension().map_or(false, |e| e == "c") {
                            let name = format!(
                                "{}_{}",
                                dir.file_name().unwrap().to_string_lossy(),
                                path.file_stem().unwrap().to_string_lossy()
                            );
                            files.push((name, path.to_string_lossy().to_string()));
                        }
                    }
                }
            }
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

#[test]
fn bulk_transpile_all_unit_tests() {
    let files = find_c_files();
    eprintln!(
        "\n=== Bulk transpile: {} .c files from tests/unit/ ===\n",
        files.len()
    );

    let mut passed = 0u32;
    let mut failed = 0u32;
    let mut skip = 0u32;

    for (name, path) in &files {
        let c_path = Path::new(path);
        let c_path_str = c_path.to_string_lossy();

        // Create temp compile_commands.json
        let (_temp_dir, cc_path) =
            c2dascript_transpile::create_temp_compile_commands(&[c_path.to_path_buf()]);

        // Survey output goes to a scratch directory; the survey must never
        // rewrite the checked-in tests/unit/*/src/*.das files.
        let output_dir = tempfile::tempdir().expect("temporary survey output directory");
        let tcfg = c2dascript_transpile::TranspilerConfig {
            verbose: false,
            output_dir: Some(output_dir.path().to_owned()),
            ..Default::default()
        };

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c2dascript_transpile::transpile_checked(tcfg, &cc_path, &["-w"]).is_ok()
        }));

        let das_path = output_dir
            .path()
            .join(c_path.with_extension("das").file_name().unwrap());
        let das_content = std::fs::read_to_string(&das_path).unwrap_or_default();

        if matches!(result, Ok(true)) && !das_content.is_empty() {
            passed += 1;
            eprintln!("  PASS transpile: {}", name);
        } else if result.is_err() {
            failed += 1;
            // Get panic message
            let msg = match result {
                Err(e) => {
                    if let Some(s) = e.downcast_ref::<String>() {
                        s.clone()
                    } else if let Some(s) = e.downcast_ref::<&str>() {
                        s.to_string()
                    } else {
                        "unknown panic".to_string()
                    }
                }
                _ => "unknown".to_string(),
            };
            eprintln!("  FAIL panic:    {} — {}", name, msg);
        } else {
            skip += 1;
            eprintln!("  SKIP empty:    {} — no output", name);
        }
    }

    let total = files.len();
    eprintln!(
        "\n=== Results: {} PASS, {} FAIL, {} SKIP / {} total ===",
        passed, failed, skip, total
    );
    // Don't assert — this is a survey, not a pass/fail gate
}
