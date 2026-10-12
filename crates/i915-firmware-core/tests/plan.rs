// SPDX-License-Identifier: GPL-2.0-only
mod common;
use common::*;
use i915_firmware_core::plan::*;
use i915_firmware_core::{build_plan, parse_package, Stepping};

#[test]
fn constants_are_linux_literals() {
    assert_eq!(DC_STATE_DEBUG, 0x45520); // intel_display_regs.h:2831
    assert_eq!(DC_STATE_DEBUG_MASK_CORES, 1); // :2832
    assert_eq!(DC_STATE_DEBUG_MASK_MEMORY_UP, 2); // :2833
    assert_eq!(DC_STATE_DEBUG_MASK_SET, 3);
}

#[test]
fn plan_writes_program_words_at_start_plus_4i_then_pairs_in_order() {
    let fw = simple(0x0001_0004, &[(0x8F034, 0xc003b400), (0x8F004, 0x500034)], &[10, 20, 30]);
    let pkg = parse_package(&fw, Stepping::WILDCARD, 0x3000).unwrap();
    let plan = build_plan(&pkg).unwrap();
    assert_eq!(plan.version, 0x0001_0004);
    // DMC_PROGRAM(addr, i) = addr + i*4, addr = 0x80000 for v1 (intel_dmc_regs.h:277, intel_dmc.c:258)
    assert_eq!(
        plan.program,
        vec![
            RegWrite { offset: 0x80000, value: 10 },
            RegWrite { offset: 0x80004, value: 20 },
            RegWrite { offset: 0x80008, value: 30 },
        ]
    );
    assert_eq!(
        plan.mmio,
        vec![RegWrite { offset: 0x8F034, value: 0xc003b400 }, RegWrite { offset: 0x8F004, value: 0x500034 }]
    );
    assert_eq!(plan.dc_state_debug_or, 3);
}
