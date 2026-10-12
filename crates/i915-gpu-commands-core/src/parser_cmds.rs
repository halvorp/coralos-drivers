// SPDX-License-Identifier: GPL-2.0-only
//! The "commands used only by the command parser" block and the Gen6-Gen9 3D / media / MI_MATH headers of
//! intel_gpu_commands.h (`:344-446`), plus the canonical-address helpers (`:456-465`). Same file, same rules as the crate root:
//! one definition each, `file:line` on every constant.

use crate::mi_instr;

/// `MI_MATH(x)` (intel_gpu_commands.h:344): `x` instruction dwords follow; `x` must be at least 1.
pub const fn mi_math(x: u32) -> u32 {
    assert!(x >= 1, "MI_MATH(0) would encode length -1");
    mi_instr(0x1a, x - 1)
}
/// `MI_MATH_INSTR(opcode, op1, op2)` (intel_gpu_commands.h:345).
pub const fn mi_math_instr(opcode: u32, op1: u32, op2: u32) -> u32 {
    (opcode << 20) | (op1 << 10) | op2
}
/// `MI_MATH_LOAD(op1, op2)` (intel_gpu_commands.h:348).
pub const fn mi_math_load(op1: u32, op2: u32) -> u32 {
    mi_math_instr(0x080, op1, op2)
}
/// `MI_MATH_LOADINV(op1, op2)` (intel_gpu_commands.h:349).
pub const fn mi_math_loadinv(op1: u32, op2: u32) -> u32 {
    mi_math_instr(0x480, op1, op2)
}
/// `MI_MATH_LOAD0(op1)` (intel_gpu_commands.h:350).
pub const fn mi_math_load0(op1: u32) -> u32 {
    mi_math_instr(0x081, op1, 0)
}
/// `MI_MATH_LOAD1(op1)` (intel_gpu_commands.h:351).
pub const fn mi_math_load1(op1: u32) -> u32 {
    mi_math_instr(0x481, op1, 0)
}
/// `MI_MATH_STORE(op1, op2)` (intel_gpu_commands.h:357).
pub const fn mi_math_store(op1: u32, op2: u32) -> u32 {
    mi_math_instr(0x180, op1, op2)
}
/// `MI_MATH_STOREINV(op1, op2)` (intel_gpu_commands.h:358).
pub const fn mi_math_storeinv(op1: u32, op2: u32) -> u32 {
    mi_math_instr(0x580, op1, op2)
}
/// `MEDIA_INSTR(pipe, op, sub_op, flags)` (intel_gpu_commands.h:370-372).
pub const fn media_instr(pipe: u32, op: u32, sub_op: u32, flags: u32) -> u32 {
    crate::instr(crate::INSTR_RC_CLIENT) | (pipe << 27) | (op << 24) | (sub_op << 16) | flags
}

pub const MI_ARB_CHECK: u32 = mi_instr(0x05, 0); // intel_gpu_commands.h:384
pub const MI_RS_CONTROL: u32 = mi_instr(0x06, 0); // intel_gpu_commands.h:385
pub const MI_URB_ATOMIC_ALLOC: u32 = mi_instr(0x09, 0); // intel_gpu_commands.h:386
pub const MI_PREDICATE: u32 = mi_instr(0x0c, 0); // intel_gpu_commands.h:387
pub const MI_RS_CONTEXT: u32 = mi_instr(0x0f, 0); // intel_gpu_commands.h:388
pub const MI_TOPOLOGY_FILTER: u32 = mi_instr(0x0d, 0); // intel_gpu_commands.h:389
pub const MI_LOAD_SCAN_LINES_EXCL: u32 = mi_instr(0x13, 0); // intel_gpu_commands.h:390
pub const MI_URB_CLEAR: u32 = mi_instr(0x19, 0); // intel_gpu_commands.h:391
pub const MI_UPDATE_GTT: u32 = mi_instr(0x23, 0); // intel_gpu_commands.h:392
pub const MI_CLFLUSH: u32 = mi_instr(0x27, 0); // intel_gpu_commands.h:393
pub const MI_REPORT_PERF_COUNT: u32 = mi_instr(0x28, 0); // intel_gpu_commands.h:394
pub const MI_REPORT_PERF_COUNT_GGTT: u32 = 1 << 0; // intel_gpu_commands.h:395
pub const MI_RS_STORE_DATA_IMM: u32 = mi_instr(0x2b, 0); // intel_gpu_commands.h:396
pub const MI_LOAD_URB_MEM: u32 = mi_instr(0x2c, 0); // intel_gpu_commands.h:397
pub const MI_STORE_URB_MEM: u32 = mi_instr(0x2d, 0); // intel_gpu_commands.h:398
pub const MI_CONDITIONAL_BATCH_BUFFER_END: u32 = mi_instr(0x36, 0); // intel_gpu_commands.h:399
pub const MI_DO_COMPARE: u32 = 1 << 21; // intel_gpu_commands.h:400
pub const STATE_BASE_ADDRESS: u32 = (0x3 << 29) | (0x0 << 27) | (0x1 << 24) | (0x1 << 16); // intel_gpu_commands.h:402
pub const BASE_ADDRESS_MODIFY: u32 = 1 << 0; // intel_gpu_commands.h:404
pub const PIPELINE_SELECT: u32 = (0x3 << 29) | (0x1 << 27) | (0x1 << 24) | (0x4 << 16); // intel_gpu_commands.h:405
pub const PIPELINE_SELECT_MEDIA: u32 = 1 << 0; // intel_gpu_commands.h:407
pub const GFX_OP_3DSTATE_VF_STATISTICS: u32 = (0x3 << 29) | (0x1 << 27) | (0x0 << 24) | (0xb << 16); // intel_gpu_commands.h:408
pub const MEDIA_VFE_STATE: u32 = (0x3 << 29) | (0x2 << 27) | (0x0 << 24) | (0x0 << 16); // intel_gpu_commands.h:410
pub const MEDIA_VFE_STATE_MMIO_ACCESS_MASK: u32 = 0x18; // intel_gpu_commands.h:412
pub const MEDIA_INTERFACE_DESCRIPTOR_LOAD: u32 = (0x3 << 29) | (0x2 << 27) | (0x0 << 24) | (0x2 << 16); // intel_gpu_commands.h:413
pub const MEDIA_OBJECT: u32 = (0x3 << 29) | (0x2 << 27) | (0x1 << 24) | (0x0 << 16); // intel_gpu_commands.h:415
pub const GPGPU_OBJECT: u32 = (0x3 << 29) | (0x2 << 27) | (0x1 << 24) | (0x4 << 16); // intel_gpu_commands.h:417
pub const GPGPU_WALKER: u32 = (0x3 << 29) | (0x2 << 27) | (0x1 << 24) | (0x5 << 16); // intel_gpu_commands.h:418
pub const GFX_OP_3DSTATE_DX9_CONSTANTF_VS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x39 << 16); // intel_gpu_commands.h:419
pub const GFX_OP_3DSTATE_DX9_CONSTANTF_PS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x3a << 16); // intel_gpu_commands.h:421
pub const GFX_OP_3DSTATE_SO_DECL_LIST: u32 = (0x3 << 29) | (0x3 << 27) | (0x1 << 24) | (0x17 << 16); // intel_gpu_commands.h:423
pub const GFX_OP_3DSTATE_BINDING_TABLE_EDIT_VS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x43 << 16); // intel_gpu_commands.h:426
pub const GFX_OP_3DSTATE_BINDING_TABLE_EDIT_GS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x44 << 16); // intel_gpu_commands.h:428
pub const GFX_OP_3DSTATE_BINDING_TABLE_EDIT_HS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x45 << 16); // intel_gpu_commands.h:430
pub const GFX_OP_3DSTATE_BINDING_TABLE_EDIT_DS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x46 << 16); // intel_gpu_commands.h:432
pub const GFX_OP_3DSTATE_BINDING_TABLE_EDIT_PS: u32 = (0x3 << 29) | (0x3 << 27) | (0x0 << 24) | (0x47 << 16); // intel_gpu_commands.h:434
pub const COLOR_BLT: u32 = (0x2 << 29) | (0x40 << 22); // intel_gpu_commands.h:437
pub const SRC_COPY_BLT: u32 = (0x2 << 29) | (0x43 << 22); // intel_gpu_commands.h:438
pub const MI_MATH_NOOP: u32 = mi_math_instr(0x000, 0x0, 0x0); // intel_gpu_commands.h:347
pub const MI_MATH_ADD: u32 = mi_math_instr(0x100, 0x0, 0x0); // intel_gpu_commands.h:352
pub const MI_MATH_SUB: u32 = mi_math_instr(0x101, 0x0, 0x0); // intel_gpu_commands.h:353
pub const MI_MATH_AND: u32 = mi_math_instr(0x102, 0x0, 0x0); // intel_gpu_commands.h:354
pub const MI_MATH_OR: u32 = mi_math_instr(0x103, 0x0, 0x0); // intel_gpu_commands.h:355
pub const MI_MATH_XOR: u32 = mi_math_instr(0x104, 0x0, 0x0); // intel_gpu_commands.h:356
pub const MI_MATH_REG_SRCA: u32 = 0x20; // intel_gpu_commands.h:361
pub const MI_MATH_REG_SRCB: u32 = 0x21; // intel_gpu_commands.h:362
pub const MI_MATH_REG_ACCU: u32 = 0x31; // intel_gpu_commands.h:363
pub const MI_MATH_REG_ZF: u32 = 0x32; // intel_gpu_commands.h:364
pub const MI_MATH_REG_CF: u32 = 0x33; // intel_gpu_commands.h:365
pub const MFX_WAIT: u32 = media_instr(1, 0, 0, 0); // intel_gpu_commands.h:374
pub const MFX_WAIT_DW0_MFX_SYNC_CONTROL_FLAG: u32 = 1 << 8; // intel_gpu_commands.h:375
pub const MFX_WAIT_DW0_PXP_SYNC_CONTROL_FLAG: u32 = 1 << 9; // intel_gpu_commands.h:376
pub const GEN8_HIGH_ADDRESS_BIT: u32 = 47; // intel_gpu_commands.h:456

/// `gen8_canonical_addr(address)` (intel_gpu_commands.h:457-460): bits 63:48 copy bit 47.
pub const fn gen8_canonical_addr(address: u64) -> u64 {
    (((address << (63 - GEN8_HIGH_ADDRESS_BIT as u64)) as i64) >> (63 - GEN8_HIGH_ADDRESS_BIT as u64)) as u64
}
/// `gen8_noncanonical_addr(address)` (intel_gpu_commands.h:462-465): keep bits 47:0.
pub const fn gen8_noncanonical_addr(address: u64) -> u64 {
    address & ((1u64 << (GEN8_HIGH_ADDRESS_BIT + 1)) - 1)
}
/// `__gen6_emit_bb_start` payload (intel_gpu_commands.h:467-473): the two dwords `[MI_BATCH_BUFFER_START | flags, addr]`.
pub const fn gen6_bb_start(addr: u32, flags: u32) -> [u32; 2] {
    [crate::MI_BATCH_BUFFER_START | flags, addr]
}
