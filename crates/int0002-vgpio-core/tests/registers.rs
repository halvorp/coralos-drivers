// SPDX-License-Identifier: GPL-2.0-only
//! Literal Linux vectors for GPE0a register math.
//!
//! Ported from Linux `drivers/platform/x86/intel/int0002_vgpio.c`.
//! Copyright (C) 2017 Hans de Goede; Copyright (c) 2014 Intel Corporation.

use int0002_vgpio_core::registers::{
    acknowledge_value, disable_pme, enable_pme, GPE0A_EN_PORT, GPE0A_PME_B0_EN_BIT,
    GPE0A_PME_B0_STS_BIT, GPE0A_STS_PORT,
};

/// int0002_vgpio.c:47-:50. These are Linux literals, not values derived from the production code.
#[test]
fn gpe0a_ports_and_bits_match_linux() {
    assert_eq!(GPE0A_PME_B0_STS_BIT, 1 << 13);
    assert_eq!(GPE0A_PME_B0_EN_BIT, 1 << 13);
    assert_eq!(GPE0A_STS_PORT, 0x420);
    assert_eq!(GPE0A_EN_PORT, 0x428);
    assert_eq!(GPE0A_PME_B0_STS_BIT.count_ones(), 1);
    assert_eq!(GPE0A_PME_B0_EN_BIT.count_ones(), 1);
}

/// int0002_vgpio.c:80-:83 writes the status bit itself to acknowledge the interrupt.
#[test]
fn acknowledgement_is_a_literal_write_one_to_clear_value() {
    assert_eq!(acknowledge_value(), 0x0000_2000);
}

/// int0002_vgpio.c:93-:95 ORs bit 13 into the sampled enable word.
#[test]
fn enabling_sets_only_the_pme_bit() {
    assert_eq!(enable_pme(0), 0x0000_2000);
    assert_eq!(enable_pme(0x8000_0001), 0x8000_2001);
    assert_eq!(enable_pme(0xffff_ffff), 0xffff_ffff);
}

/// int0002_vgpio.c:104-:106 clears bit 13 and preserves all unrelated enable bits.
#[test]
fn disabling_clears_only_the_pme_bit() {
    assert_eq!(disable_pme(0x0000_2000), 0);
    assert_eq!(disable_pme(0x8000_2001), 0x8000_0001);
    assert_eq!(disable_pme(0xffff_ffff), 0xffff_dfff);
}
