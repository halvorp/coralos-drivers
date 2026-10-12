// SPDX-License-Identifier: GPL-2.0-only
//! The command-parser / media / MI_MATH block (intel_gpu_commands.h:344-446) against GCC's evaluation of the Linux header,
//! written out literally; plus the canonical-address helpers.
use i915_gpu_commands_core::*;

#[test]
fn parser_block_constants_equal_the_values_gcc_computes_from_the_linux_header() {
    assert_eq!(MI_ARB_CHECK, 0x02800000, "MI_ARB_CHECK");
    assert_eq!(MI_RS_CONTROL, 0x03000000, "MI_RS_CONTROL");
    assert_eq!(MI_URB_ATOMIC_ALLOC, 0x04800000, "MI_URB_ATOMIC_ALLOC");
    assert_eq!(MI_PREDICATE, 0x06000000, "MI_PREDICATE");
    assert_eq!(MI_RS_CONTEXT, 0x07800000, "MI_RS_CONTEXT");
    assert_eq!(MI_TOPOLOGY_FILTER, 0x06800000, "MI_TOPOLOGY_FILTER");
    assert_eq!(MI_LOAD_SCAN_LINES_EXCL, 0x09800000, "MI_LOAD_SCAN_LINES_EXCL");
    assert_eq!(MI_URB_CLEAR, 0x0c800000, "MI_URB_CLEAR");
    assert_eq!(MI_UPDATE_GTT, 0x11800000, "MI_UPDATE_GTT");
    assert_eq!(MI_CLFLUSH, 0x13800000, "MI_CLFLUSH");
    assert_eq!(MI_REPORT_PERF_COUNT, 0x14000000, "MI_REPORT_PERF_COUNT");
    assert_eq!(MI_REPORT_PERF_COUNT_GGTT, 0x00000001, "MI_REPORT_PERF_COUNT_GGTT");
    assert_eq!(MI_RS_STORE_DATA_IMM, 0x15800000, "MI_RS_STORE_DATA_IMM");
    assert_eq!(MI_LOAD_URB_MEM, 0x16000000, "MI_LOAD_URB_MEM");
    assert_eq!(MI_STORE_URB_MEM, 0x16800000, "MI_STORE_URB_MEM");
    assert_eq!(MI_CONDITIONAL_BATCH_BUFFER_END, 0x1b000000, "MI_CONDITIONAL_BATCH_BUFFER_END");
    assert_eq!(MI_DO_COMPARE, 0x00200000, "MI_DO_COMPARE");
    assert_eq!(STATE_BASE_ADDRESS, 0x61010000, "STATE_BASE_ADDRESS");
    assert_eq!(BASE_ADDRESS_MODIFY, 0x00000001, "BASE_ADDRESS_MODIFY");
    assert_eq!(PIPELINE_SELECT, 0x69040000, "PIPELINE_SELECT");
    assert_eq!(PIPELINE_SELECT_MEDIA, 0x00000001, "PIPELINE_SELECT_MEDIA");
    assert_eq!(GFX_OP_3DSTATE_VF_STATISTICS, 0x680b0000, "GFX_OP_3DSTATE_VF_STATISTICS");
    assert_eq!(MEDIA_VFE_STATE, 0x70000000, "MEDIA_VFE_STATE");
    assert_eq!(MEDIA_VFE_STATE_MMIO_ACCESS_MASK, 0x00000018, "MEDIA_VFE_STATE_MMIO_ACCESS_MASK");
    assert_eq!(MEDIA_INTERFACE_DESCRIPTOR_LOAD, 0x70020000, "MEDIA_INTERFACE_DESCRIPTOR_LOAD");
    assert_eq!(MEDIA_OBJECT, 0x71000000, "MEDIA_OBJECT");
    assert_eq!(GPGPU_OBJECT, 0x71040000, "GPGPU_OBJECT");
    assert_eq!(GPGPU_WALKER, 0x71050000, "GPGPU_WALKER");
    assert_eq!(GFX_OP_3DSTATE_DX9_CONSTANTF_VS, 0x78390000, "GFX_OP_3DSTATE_DX9_CONSTANTF_VS");
    assert_eq!(GFX_OP_3DSTATE_DX9_CONSTANTF_PS, 0x783a0000, "GFX_OP_3DSTATE_DX9_CONSTANTF_PS");
    assert_eq!(GFX_OP_3DSTATE_SO_DECL_LIST, 0x79170000, "GFX_OP_3DSTATE_SO_DECL_LIST");
    assert_eq!(GFX_OP_3DSTATE_BINDING_TABLE_EDIT_VS, 0x78430000, "GFX_OP_3DSTATE_BINDING_TABLE_EDIT_VS");
    assert_eq!(GFX_OP_3DSTATE_BINDING_TABLE_EDIT_GS, 0x78440000, "GFX_OP_3DSTATE_BINDING_TABLE_EDIT_GS");
    assert_eq!(GFX_OP_3DSTATE_BINDING_TABLE_EDIT_HS, 0x78450000, "GFX_OP_3DSTATE_BINDING_TABLE_EDIT_HS");
    assert_eq!(GFX_OP_3DSTATE_BINDING_TABLE_EDIT_DS, 0x78460000, "GFX_OP_3DSTATE_BINDING_TABLE_EDIT_DS");
    assert_eq!(GFX_OP_3DSTATE_BINDING_TABLE_EDIT_PS, 0x78470000, "GFX_OP_3DSTATE_BINDING_TABLE_EDIT_PS");
    assert_eq!(COLOR_BLT, 0x50000000, "COLOR_BLT");
    assert_eq!(SRC_COPY_BLT, 0x50c00000, "SRC_COPY_BLT");
    assert_eq!(MI_MATH_NOOP, 0x00000000, "MI_MATH_NOOP");
    assert_eq!(MI_MATH_ADD, 0x10000000, "MI_MATH_ADD");
    assert_eq!(MI_MATH_SUB, 0x10100000, "MI_MATH_SUB");
    assert_eq!(MI_MATH_AND, 0x10200000, "MI_MATH_AND");
    assert_eq!(MI_MATH_OR, 0x10300000, "MI_MATH_OR");
    assert_eq!(MI_MATH_XOR, 0x10400000, "MI_MATH_XOR");
    assert_eq!(MI_MATH_REG_SRCA, 0x00000020, "MI_MATH_REG_SRCA");
    assert_eq!(MI_MATH_REG_SRCB, 0x00000021, "MI_MATH_REG_SRCB");
    assert_eq!(MI_MATH_REG_ACCU, 0x00000031, "MI_MATH_REG_ACCU");
    assert_eq!(MI_MATH_REG_ZF, 0x00000032, "MI_MATH_REG_ZF");
    assert_eq!(MI_MATH_REG_CF, 0x00000033, "MI_MATH_REG_CF");
    assert_eq!(MFX_WAIT, 0x68000000, "MFX_WAIT");
    assert_eq!(MFX_WAIT_DW0_MFX_SYNC_CONTROL_FLAG, 0x00000100, "MFX_WAIT_DW0_MFX_SYNC_CONTROL_FLAG");
    assert_eq!(MFX_WAIT_DW0_PXP_SYNC_CONTROL_FLAG, 0x00000200, "MFX_WAIT_DW0_PXP_SYNC_CONTROL_FLAG");
    assert_eq!(GEN8_HIGH_ADDRESS_BIT, 0x0000002f, "GEN8_HIGH_ADDRESS_BIT");
}

#[test]
fn mi_math_and_media_macros_match_the_linux_expansions() {
    // expected values: GCC on the Linux header (see the generator transcript in the commit message)
    assert_eq!(mi_math(4), 0x0d00_0003);
    assert_eq!(mi_math_load(MI_MATH_REG_SRCA, 5), 0x0800_8005);
    assert_eq!(mi_math_loadinv(MI_MATH_REG_SRCB, 1), 0x4800_8401);
    assert_eq!(mi_math_store(0x10, MI_MATH_REG_ACCU), 0x1800_4031);
    assert_eq!(mi_math_storeinv(0x10, MI_MATH_REG_ZF), 0x5800_4032);
    assert_eq!(media_instr(1, 0, 0, 0), 0x6800_0000);
    assert_eq!(media_instr(2, 6, 9, 0), 0x7609_0000);
    assert_eq!(media_instr(1, 2, 3, 0x55), 0x6a03_0055);
    // DEVIATION: Linux's MI_MATH_LOAD0/LOAD1 (intel_gpu_commands.h:350-351) pass TWO arguments to the three-argument
    // MI_MATH_INSTR and cannot compile if ever expanded (they are unused). The port supplies op2 = 0, the only reading
    // that matches the opcode map (0x081 / 0x481 take a single operand).
    assert_eq!(mi_math_load0(MI_MATH_REG_SRCA), (0x081 << 20) | (0x20 << 10));
    assert_eq!(mi_math_load1(MI_MATH_REG_SRCB), (0x481 << 20) | (0x21 << 10));
}

#[test]
fn canonical_address_helpers_follow_sign_extend64_and_genmask() {
    assert_eq!(gen8_canonical_addr(0x0000_7fff_ffff_f000), 0x0000_7fff_ffff_f000);
    assert_eq!(gen8_canonical_addr(0x0000_8000_0000_0000), 0xffff_8000_0000_0000);
    assert_eq!(gen8_canonical_addr(0xffff_ffff_ffff_ffff), 0xffff_ffff_ffff_ffff);
    assert_eq!(gen8_noncanonical_addr(0xffff_8000_0000_1000), 0x0000_8000_0000_1000);
    assert_eq!(gen8_noncanonical_addr(0x0000_1234_5678_9abc), 0x0000_1234_5678_9abc);
    // __gen6_emit_bb_start (intel_gpu_commands.h:467): { MI_BATCH_BUFFER_START | flags, addr }
    assert_eq!(gen6_bb_start(0x1000, 1), [0x1880_0001, 0x1000]);
}

#[test]
#[should_panic(expected = "MI_MATH(0)")]
fn mi_math_of_zero_dwords_is_refused_by_name() {
    let _ = mi_math(0);
}
