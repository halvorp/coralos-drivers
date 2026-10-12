// SPDX-License-Identifier: GPL-2.0-only
//! Parses the REAL Intel blob when present on the build host. The blob (Intel LICENSE.i915: binary redistribution
//! only, with the licence text; no reverse engineering) is NOT checked in; the test decompresses
//! /lib/firmware/i915/kbl_dmc_ver1_04.bin.xz (or $I915_KBL_DMC_BIN, a raw .bin) and SKIPS when absent.
use i915_firmware_core::*;
use std::process::Command;

fn load() -> Option<Vec<u8>> {
    if let Ok(p) = std::env::var("I915_KBL_DMC_BIN") {
        return std::fs::read(p).ok();
    }
    for p in ["/lib/firmware/i915/kbl_dmc_ver1_04.bin.xz", "/usr/lib/firmware/i915/kbl_dmc_ver1_04.bin.xz"] {
        if std::path::Path::new(p).exists() {
            let o = Command::new("xz").args(["-dc", p]).output().ok()?;
            if o.status.success() { return Some(o.stdout); }
        }
    }
    None
}

#[test]
fn real_kbl_dmc_ver1_04_parses_and_plans() {
    let Some(fw) = load() else { eprintln!("SKIP: no kbl_dmc_ver1_04.bin on this host"); return; };
    let c = kbl_firmware();
    let pkg = parse_package(&fw, Stepping::WILDCARD, c.max_fw_size).expect("real blob must validate");
    // the file name promises ver1_04 (intel_dmc.c:176); css->version is DMC_VERSION(1, 4)
    assert_eq!(pkg.version, c.file_version, "css version {:#x}", pkg.version);
    let m = pkg.image(DmcId::Main).expect("main");
    assert_eq!(m.header_ver, 1);
    assert_eq!(m.start_mmioaddr, 0x80000);
    assert!(!m.payload.is_empty() && m.payload.len() * 4 <= c.max_fw_size as usize);
    assert!(!m.mmio.is_empty());
    for &(a, _) in &m.mmio { assert!((0x80000..=0x8FFFF).contains(&a), "{a:#x}"); }
    // intel_dmc_regs.h:502-505,509: DMC_SSP_BASE 0x8F074 = DMC_SSP_BASE_ADDR_GEN9 (:278) 0x2FC0, DMC_HTP_SKL 0x8F004,
    // DMC_LAST_WRITE 0x8F034 = DMC_LAST_WRITE_VALUE 0xc003b400 (the three pairs of the shipped KBL blob)
    assert_eq!(m.mmio[0], (0x8F074, 0x2FC0));
    assert_eq!(m.mmio[1].0, 0x8F004);
    assert_eq!(m.mmio[2], (0x8F034, 0xc003b400));
    let plan = build_plan(&pkg).unwrap();
    assert_eq!(plan.program.len(), m.payload.len());
    assert_eq!(plan.mmio.len(), m.mmio.len());
    eprintln!("real blob: version {:#x}, payload {} dwords, mmio {:x?}", pkg.version, m.payload.len(), m.mmio);
}
