#!/usr/bin/env bash
set -euo pipefail

syntax_dir="$(cd "$(dirname "$0")" && pwd)"
daslang="$(bash "$syntax_dir/../../scripts/find_daslang.sh")"

for entry in \
    p17_runtime_malloc:runtime_malloc_returns_typed_pointer \
    p18_runtime_calloc_memset:runtime_calloc_and_memset_lower_to_runtime \
    p19_runtime_memory_calls:runtime_memory_calls_lower_to_runtime \
    p20_pointer_abi_edges:pointer_abi_edges \
    p21_byte_numeric:byte_numeric_edges \
    p22_typed_literals:typed_literals \
    p23_bool_numeric:bool_numeric_runtime \
    p24_nonruntime_pointer_call:nonruntime_pointer_call \
    p25_array_initializers:array_initializers_runtime \
    p26_variadic_sum:variadic_sum_runtime \
    p27_variadic_promotions:variadic_promotions_runtime \
    p28_variadic_multiple_types:variadic_multiple_types_runtime \
    p30_macro_constant_expression:macro_constant_expression_runtime \
    p31_macro_side_effect:macro_side_effect_runtime \
    p32_macro_statement_expression:macro_statement_expression_runtime \
    p33_predefined_sizeof_builtin:predefined_sizeof_builtin_runtime \
    p34_c_layout_records:c_layout_records_runtime \
    p35_pointer_backed_struct:pointer_backed_struct_runtime \
    p37_union_overlay:union_overlay_runtime \
    p38_local_union_init:local_union_init_runtime \
    p39_packed_scalar:packed_scalar_runtime \
    p40_bitfield_rmw:bitfield_rmw_runtime \
    p41_union_cast:union_cast_runtime
do
    test_name="${entry%%:*}"
    main_name="${entry#*:}"
    "$daslang" "$syntax_dir/$test_name.das" -main "$main_name"
done
