use std::path::Path;

#[test]
fn c2rust_to_c2dascript_map_covers_required_layers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let map_path = root.join("docs/c2rust_to_c2dascript_map.md");
    let map = std::fs::read_to_string(&map_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", map_path.display()));

    for required in [
        "CFG reconstruction",
        "Decl lifting / temp placement",
        "Expression translation",
        "Implicit / explicit casts",
        "Pointer / null lowering",
        "C runtime / libc compatibility",
        "Anonymous / named type emission",
        "Renaming / namespaces",
        "Statement / expression normalization",
        "Intermediate Invariants",
        "Render Boundary Invariant",
        "c2da_runtime_helpers",
        "Function pointer / callback ABI",
        "invoke(function?)",
    ] {
        assert!(
            map.contains(required),
            "architecture map does not mention required layer/invariant: {required}"
        );
    }

    for removed_debt in [
        "normalize_generated_numeric_patterns",
        "replace_generated_function",
        "normalize_first_phase_shift_assignments",
    ] {
        assert!(
            !map.contains(removed_debt),
            "architecture map must not describe removed generated-text debt as live code: {removed_debt}"
        );
    }
}

#[test]
fn post_render_semantic_repair_is_removed_and_cannot_return() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let translator = root.join("c2dascript-transpile/src/translator");
    let mod_rs = std::fs::read_to_string(translator.join("mod.rs")).expect("translator/mod.rs");
    let inventory = std::fs::read_to_string(root.join("docs/post-render-inventory.md"))
        .expect("post-render inventory");

    for forbidden in [
        "normalize_generated_numeric_patterns",
        "normalize_first_phase_shift_assignments",
        "replace_generated_function",
        "let mut code = module.to_string()",
    ] {
        assert!(
            !mod_rs.contains(forbidden),
            "post-render semantic repair is forbidden in translator/mod.rs: {forbidden}"
        );
    }
    assert!(
        mod_rs.contains("module.to_string(),"),
        "active translation must render the DaModule directly"
    );
    assert_eq!(
        mod_rs.matches("module.to_string()").count(),
        1,
        "DaModule rendering must have one direct terminal use, not an intermediate mutable string"
    );

    for entry in [
        "normalize_generated_numeric_patterns",
        "normalize_first_phase_shift_assignments",
        "replace_generated_function",
        "dead in current render path",
        "DaModule::to_string()",
    ] {
        assert!(
            inventory.contains(entry),
            "inventory missing proof record: {entry}"
        );
    }

    for source in std::fs::read_dir(&translator).expect("translator directory") {
        let source = source.expect("translator entry").path();
        if source.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&source).expect("translator source");
        assert!(
            !text.contains("let mut code = module.to_string()"),
            "mutable rendered source is forbidden: {}",
            source.display()
        );
    }
}

#[test]
fn callback_repro_does_not_emit_invalid_invoke_function_pointer() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let das_path = root.join("tests/manual/repro-architecture/repro_function_pointer_callback.das");
    let das = std::fs::read_to_string(&das_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", das_path.display()));

    assert!(
        !das.contains("invoke("),
        "C function pointer fallback must not emit daScript invoke(function?)"
    );
    assert!(
        das.contains("rc = int(0)"),
        "callback fallback should materialize the C int return default"
    );
}

#[test]
fn local_anonymous_enum_repro_does_not_emit_missing_named_type() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let das_path = root.join("tests/manual/repro-architecture/repro_local_anonymous_enum.das");
    let das = std::fs::read_to_string(&das_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", das_path.display()));

    assert!(
        !das.contains("Unnamed"),
        "anonymous enum variables must lower to their integral type, not to a missing synthetic named type"
    );
    assert!(
        das.contains("var e : uint") || das.contains("var e : int"),
        "local anonymous enum variable should be represented as an integral daScript type"
    );
}

#[test]
fn string_literal_address_repro_does_not_index_string_literal() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let das_path = root.join("tests/manual/repro-architecture/repro_string_literal_address.das");
    let das = std::fs::read_to_string(&das_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", das_path.display()));

    assert!(
        !das.contains("\"SoundHandler\"[0]"),
        "C &string_literal[0] must not lower to daScript string literal indexing"
    );
    assert!(
        das.contains("null"),
        "until static char storage is modeled, string literal address lowering should use a typed null sentinel"
    );
}

#[test]
fn pointer_null_cast_repro_lowers_zero_to_null() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let das_path = root.join("tests/manual/repro-architecture/repro_pointer_null_cast.das");
    let das = std::fs::read_to_string(&das_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", das_path.display()));

    for invalid in [
        "cast<Node?>(0)",
        "reinterpret<Node?>(0)",
        "reinterpret<Node?>(cast<Node?>(0))",
        "reinterpret<Node?>(null)",
    ] {
        assert!(
            !das.contains(invalid),
            "C integer zero pointer conversions must lower to null, not {invalid}"
        );
    }
    assert!(
        das.contains("return null") || das.contains("p = null"),
        "pointer zero conversions should materialize daScript null"
    );
}

#[test]
fn canonical_abi_owns_storage_literals_bool_and_pointer_raw_conversions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let translator = root.join("c2dascript-transpile/src/translator");
    let abi = std::fs::read_to_string(translator.join("abi.rs")).expect("abi.rs");

    // The numeric side of the ABI is the C conversion table: integer promotion
    // (`promoted_arith_type`), usual arithmetic conversions
    // (`usual_arithmetic_type`) and the promote/narrow pair every operand and
    // store goes through. The former `storage_byte_to_numeric` helper stripped
    // explicit narrowing casts and was replaced by that table.
    for api in [
        "fn raw_address_to_pointer",
        "fn pointer_to_raw_address",
        "fn null_pointer",
        "fn promoted_arith_type",
        "fn usual_arithmetic_type",
        "fn promote_operand",
        "fn narrow_to_storage",
        "fn integer_literal_for_type",
        "fn bool_to_integer",
    ] {
        assert!(abi.contains(api), "canonical ABI API missing: {api}");
    }

    // These are conversion owners. A local reinterpret here would bypass the
    // ABI contract rather than expressing ordinary C numeric type lowering.
    for file in [
        "functions.rs",
        "operators.rs",
        "pointers.rs",
        "value_lowering.rs",
    ] {
        let source = std::fs::read_to_string(translator.join(file)).expect("translator source");
        assert!(
            !source.contains("CastKind::Reinterpret"),
            "{file} must use translator/abi.rs for pointer/raw reinterpret"
        );
    }

    for obsolete in [
        "lower_bool_numeric_cast",
        "lower_bool_numeric_cast_arg",
        "fn integer_literal_for_type",
        "fn strip_numeric_literal_casts",
    ] {
        let functions =
            std::fs::read_to_string(translator.join("functions.rs")).expect("functions.rs");
        assert!(
            !functions.contains(obsolete),
            "legacy ABI helper survived outside abi.rs: {obsolete}"
        );
    }
}

#[test]
fn real_world_driver_fixtures_are_present() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");

    // compile_commands.json is machine-local (gitignored); the canonical runner generates its
    // own per case, so only the C graph entry points are required here.
    for fixture in [
        "tests/manual/real-world-h264bsd-mp4/src/all.c",
        "tests/manual/real-world-plmpeg-stream/src/all.c",
    ] {
        let path = root.join(fixture);
        assert!(
            path.exists(),
            "missing real-world driver fixture: {}",
            path.display()
        );
    }
}

#[test]
fn plmpeg_target_graph_excludes_fixture_libc_and_reference_graph_keeps_it() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let fixture = root.join("tests/manual/real-world-plmpeg-stream");
    let src = fixture.join("src");
    let all = std::fs::read_to_string(src.join("all.c")).expect("PLMPEG all.c");
    let reference =
        std::fs::read_to_string(src.join("all_reference.c")).expect("PLMPEG C reference graph");
    let implementation =
        std::fs::read_to_string(src.join("pl_mpeg.c")).expect("PLMPEG implementation owner");
    let module = std::fs::read_to_string(src.join("module.c")).expect("PLMPEG module");
    let shim = std::fs::read_to_string(src.join("shim.c")).expect("PLMPEG shim");
    let graph =
        std::fs::read_to_string(fixture.join("PLMPEG_GRAPH.md")).expect("PLMPEG graph contract");
    let functions =
        std::fs::read_to_string(root.join("c2dascript-transpile/src/translator/functions.rs"))
            .expect("runtime call lowering");
    let runtime =
        std::fs::read_to_string(root.join("c2dascript-transpile/src/translator/runtime.rs"))
            .expect("canonical runtime registry");

    assert_eq!(
        implementation
            .matches("#define PL_MPEG_IMPLEMENTATION")
            .count(),
        1,
        "pl_mpeg.c must be the sole single-header implementation owner"
    );
    assert!(!module.contains("PL_MPEG_IMPLEMENTATION"));
    assert!(!shim.contains("PL_MPEG_IMPLEMENTATION"));

    let implementation_include = all
        .find("#include \"pl_mpeg.c\"")
        .expect("all.c includes implementation");
    let undef = all
        .find("#undef PL_MPEG_IMPLEMENTATION")
        .expect("all.c releases implementation macro");
    let module_include = all
        .find("#include \"module.c\"")
        .expect("all.c includes public wrapper");
    assert!(implementation_include < undef && undef < module_include);
    assert!(
        !all.contains("#include \"shim.c\""),
        "target graph must not translate fixture allocator/libc definitions"
    );
    let reference_shim = reference
        .find("#include \"shim.c\"")
        .expect("reference graph includes fixture shim");
    let reference_implementation = reference
        .find("#include \"pl_mpeg.c\"")
        .expect("reference graph includes implementation");
    let reference_undef = reference
        .find("#undef PL_MPEG_IMPLEMENTATION")
        .expect("reference graph releases implementation macro");
    let reference_module = reference
        .find("#include \"module.c\"")
        .expect("reference graph includes public wrapper");
    assert!(
        reference_shim < reference_implementation
            && reference_implementation < reference_undef
            && reference_undef < reference_module
    );
    assert!(module.contains("void c2da_rt_reset(void);"));
    assert!(shim.contains("void c2da_rt_reset(void)"));
    assert!(
        !functions.contains("fn canonical_runtime_function"),
        "call lowering must use runtime.rs registry, not a second name table"
    );
    for symbol in [
        "Malloc", "Calloc", "Realloc", "Free", "Memset", "Memcpy", "Memmove", "Memcmp", "Memchr",
    ] {
        assert!(
            runtime.contains(symbol),
            "runtime registry missing {symbol}"
        );
    }
    assert!(graph.contains("src/all.c"));
    assert!(graph.contains("src/all_reference.c"));
    assert!(graph.contains("src/all.das"));
}

#[test]
fn variadic_macro_and_simd_boundaries_are_owned_before_printing() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    let translator = root.join("c2dascript-transpile/src/translator");
    let variadic = std::fs::read_to_string(translator.join("variadic.rs")).expect("variadic owner");
    let macros = std::fs::read_to_string(translator.join("macros.rs")).expect("macro owner");
    let assembly = std::fs::read_to_string(translator.join("assembly.rs")).expect("asm owner");
    let simd = std::fs::read_to_string(translator.join("simd.rs")).expect("simd owner");
    let functions =
        std::fs::read_to_string(translator.join("functions.rs")).expect("call boundary");
    let printer = std::fs::read_dir(root.join("das_ast/src"))
        .expect("das_ast source directory")
        .filter_map(Result::ok)
        .filter_map(|entry| std::fs::read_to_string(entry.path()).ok())
        .collect::<String>();

    for api in [
        "fn convert_vaarg",
        "fn pack_variadic_call_tail",
        "fn pack_variadic_argument",
    ] {
        assert!(variadic.contains(api), "variadic ABI owner missing {api}");
    }
    assert!(!functions.contains("fn pack_variadic_argument"));
    assert!(macros.contains("fn convert_gnu_statement_expression"));
    assert!(macros.contains("fn convert_predefined_expression"));
    assert!(assembly.contains("unsupported inline asm"));
    assert!(simd.contains("unsupported SIMD shuffle vector"));
    assert!(simd.contains("unsupported SIMD convert vector"));

    // daScript printing must be a serializer, never an ABI repair layer.
    for forbidden in ["va_arg", "va_start", "va_end", "C2daVaArg"] {
        assert!(
            !printer.contains(forbidden),
            "printer must not normalize variadic ABI: {forbidden}"
        );
    }
}

#[test]
fn real_world_asm_simd_inventory_is_taken_from_typed_c_ast() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root");
    for fixture in [
        // Individual TUs are the compact ASM/SIMD inventory corpus.  PLMPEG's
        // `all.c` is separately validated as the canonical decoder graph;
        // this inventory stays per-TU so a new surface has a precise source.
        "tests/manual/real-world-plmpeg-stream/src/module.c",
        "tests/manual/real-world-plmpeg-stream/src/pl_mpeg.c",
        "tests/manual/real-world-plmpeg-stream/src/shim.c",
        "tests/manual/real-world-h264bsd-mp4/src/h264bsd.c",
        "tests/manual/real-world-h264bsd-mp4/src/minimp4.c",
        "tests/manual/real-world-h264bsd-mp4/src/module.c",
        "tests/manual/real-world-h264bsd-mp4/src/shim.c",
    ] {
        let source = root.join(fixture);
        let (_temp, commands) = c2dascript_transpile::create_temp_compile_commands(&[source]);
        let inventory = c2dascript_transpile::inventory_asm_simd(
            &c2dascript_transpile::TranspilerConfig::default(),
            &commands,
            &["-w"],
        )
        .unwrap_or_else(|error| panic!("AST inventory for {fixture} failed: {error}"));
        assert_eq!(
            inventory.inline_asm, 0,
            "unclassified inline asm in {fixture}"
        );
        assert_eq!(
            inventory.shuffle_vector, 0,
            "unclassified shuffle vector in {fixture}"
        );
        assert_eq!(
            inventory.convert_vector, 0,
            "unclassified convert vector in {fixture}"
        );
        assert_eq!(
            inventory.vector_type, 0,
            "unclassified vector type in {fixture}"
        );
    }
}
