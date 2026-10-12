// SPDX-License-Identifier: GPL-2.0-only
//! The PCI id table: every id Linux binds to the Skylake, Kaby Lake (incl. AML Y) and Cherryview rows, in pciids.h order, and
//! the bind order of i915_pci.c:829-840. Anything not listed is `I915Error::UnknownDevice`.
use crate::rows::*;
use crate::{DeviceInfo, I915Error, PlatformDescriptor, SubPlatform};

/// One `{ id -> row }` entry.
#[derive(Clone, Copy, Debug)]
pub struct IdRow {
    pub devid: u16,
    pub info: &'static DeviceInfo,
    pub sub: SubPlatform,
}

const fn row(devid: u16, info: &'static DeviceInfo, sub: SubPlatform) -> IdRow {
    IdRow { devid, info, sub }
}

/// 51 ids: 4 Cherryview (i915_pci.c:829), 25 Skylake (:830-833), 22 Kaby Lake (:836-840).
pub static ID_TABLE: [IdRow; 51] = [
    row(0x22b0, &CHV, SubPlatform::None), // pciids.h:340 INTEL_CHV_IDS
    row(0x22b1, &CHV, SubPlatform::None), // pciids.h:341 INTEL_CHV_IDS
    row(0x22b2, &CHV, SubPlatform::None), // pciids.h:342 INTEL_CHV_IDS
    row(0x22b3, &CHV, SubPlatform::None), // pciids.h:343 INTEL_CHV_IDS
    row(0x1906, &SKL_GT1, SubPlatform::Ult), // pciids.h:346 INTEL_SKL_ULT_GT1_IDS, ULT GT1
    row(0x1913, &SKL_GT1, SubPlatform::Ult), // pciids.h:347 INTEL_SKL_ULT_GT1_IDS, ULT GT1.5
    row(0x190e, &SKL_GT1, SubPlatform::Ulx), // pciids.h:350 INTEL_SKL_ULX_GT1_IDS, ULX GT1
    row(0x1915, &SKL_GT1, SubPlatform::Ulx), // pciids.h:351 INTEL_SKL_ULX_GT1_IDS, ULX GT1.5
    row(0x1902, &SKL_GT1, SubPlatform::None), // pciids.h:356 INTEL_SKL_GT1_IDS, DT GT1
    row(0x190a, &SKL_GT1, SubPlatform::None), // pciids.h:357 INTEL_SKL_GT1_IDS, SRV GT1
    row(0x190b, &SKL_GT1, SubPlatform::None), // pciids.h:358 INTEL_SKL_GT1_IDS, Halo GT1
    row(0x1917, &SKL_GT1, SubPlatform::None), // pciids.h:359 INTEL_SKL_GT1_IDS, DT GT1.5
    row(0x1916, &SKL_GT2, SubPlatform::Ult), // pciids.h:362 INTEL_SKL_ULT_GT2_IDS, ULT GT2
    row(0x1921, &SKL_GT2, SubPlatform::Ult), // pciids.h:363 INTEL_SKL_ULT_GT2_IDS, ULT GT2F
    row(0x191e, &SKL_GT2, SubPlatform::Ulx), // pciids.h:366 INTEL_SKL_ULX_GT2_IDS, ULX GT2
    row(0x1912, &SKL_GT2, SubPlatform::None), // pciids.h:371 INTEL_SKL_GT2_IDS, DT GT2
    row(0x191a, &SKL_GT2, SubPlatform::None), // pciids.h:372 INTEL_SKL_GT2_IDS, SRV GT2
    row(0x191b, &SKL_GT2, SubPlatform::None), // pciids.h:373 INTEL_SKL_GT2_IDS, Halo GT2
    row(0x191d, &SKL_GT2, SubPlatform::None), // pciids.h:374 INTEL_SKL_GT2_IDS, WKS GT2
    row(0x1923, &SKL_GT3, SubPlatform::Ult), // pciids.h:377 INTEL_SKL_ULT_GT3_IDS, ULT GT3
    row(0x1926, &SKL_GT3, SubPlatform::Ult), // pciids.h:378 INTEL_SKL_ULT_GT3_IDS, ULT GT3e
    row(0x1927, &SKL_GT3, SubPlatform::Ult), // pciids.h:379 INTEL_SKL_ULT_GT3_IDS, ULT GT3e
    row(0x192a, &SKL_GT3, SubPlatform::None), // pciids.h:383 INTEL_SKL_GT3_IDS, SRV GT3
    row(0x192b, &SKL_GT3, SubPlatform::None), // pciids.h:384 INTEL_SKL_GT3_IDS, Halo GT3e
    row(0x192d, &SKL_GT3, SubPlatform::None), // pciids.h:385 INTEL_SKL_GT3_IDS, SRV GT3e
    row(0x1932, &SKL_GT4, SubPlatform::None), // pciids.h:388 INTEL_SKL_GT4_IDS, DT GT4
    row(0x193a, &SKL_GT4, SubPlatform::None), // pciids.h:389 INTEL_SKL_GT4_IDS, SRV GT4e
    row(0x193b, &SKL_GT4, SubPlatform::None), // pciids.h:390 INTEL_SKL_GT4_IDS, Halo GT4e
    row(0x193d, &SKL_GT4, SubPlatform::None), // pciids.h:391 INTEL_SKL_GT4_IDS, WKS GT4e
    row(0x5906, &KBL_GT1, SubPlatform::Ult), // pciids.h:411 INTEL_KBL_ULT_GT1_IDS, ULT GT1
    row(0x5913, &KBL_GT1, SubPlatform::Ult), // pciids.h:412 INTEL_KBL_ULT_GT1_IDS, ULT GT1.5
    row(0x590e, &KBL_GT1, SubPlatform::Ulx), // pciids.h:415 INTEL_KBL_ULX_GT1_IDS, ULX GT1
    row(0x5915, &KBL_GT1, SubPlatform::Ulx), // pciids.h:416 INTEL_KBL_ULX_GT1_IDS, ULX GT1.5
    row(0x5902, &KBL_GT1, SubPlatform::None), // pciids.h:421 INTEL_KBL_GT1_IDS, DT GT1
    row(0x5908, &KBL_GT1, SubPlatform::None), // pciids.h:422 INTEL_KBL_GT1_IDS, Halo GT1
    row(0x590a, &KBL_GT1, SubPlatform::None), // pciids.h:423 INTEL_KBL_GT1_IDS, SRV GT1
    row(0x590b, &KBL_GT1, SubPlatform::None), // pciids.h:424 INTEL_KBL_GT1_IDS, Halo GT1
    row(0x5916, &KBL_GT2, SubPlatform::Ult), // pciids.h:427 INTEL_KBL_ULT_GT2_IDS, ULT GT2
    row(0x5921, &KBL_GT2, SubPlatform::Ult), // pciids.h:428 INTEL_KBL_ULT_GT2_IDS, ULT GT2F
    row(0x591e, &KBL_GT2, SubPlatform::Ulx), // pciids.h:431 INTEL_KBL_ULX_GT2_IDS, ULX GT2
    row(0x5912, &KBL_GT2, SubPlatform::None), // pciids.h:436 INTEL_KBL_GT2_IDS, DT GT2
    row(0x5917, &KBL_GT2, SubPlatform::None), // pciids.h:437 INTEL_KBL_GT2_IDS, Mobile GT2
    row(0x591a, &KBL_GT2, SubPlatform::None), // pciids.h:438 INTEL_KBL_GT2_IDS, SRV GT2
    row(0x591b, &KBL_GT2, SubPlatform::None), // pciids.h:439 INTEL_KBL_GT2_IDS, Halo GT2
    row(0x591d, &KBL_GT2, SubPlatform::None), // pciids.h:440 INTEL_KBL_GT2_IDS, WKS GT2
    row(0x5926, &KBL_GT3, SubPlatform::Ult), // pciids.h:443 INTEL_KBL_ULT_GT3_IDS, ULT GT3
    row(0x5923, &KBL_GT3, SubPlatform::None), // pciids.h:447 INTEL_KBL_GT3_IDS, ULT GT3
    row(0x5927, &KBL_GT3, SubPlatform::None), // pciids.h:448 INTEL_KBL_GT3_IDS, ULT GT3
    row(0x593b, &KBL_GT3, SubPlatform::None), // pciids.h:451 INTEL_KBL_GT4_IDS, Halo GT4
    row(0x591c, &KBL_GT2, SubPlatform::Ulx), // pciids.h:455 INTEL_AML_KBL_GT2_IDS, ULX GT2
    row(0x87c0, &KBL_GT2, SubPlatform::Ulx), // pciids.h:456 INTEL_AML_KBL_GT2_IDS, ULX GT2
];

/// The descriptor for `devid`, or the NAMED refusal. A linear scan over a 51-entry table; no aliasing, no fallthrough.
pub fn lookup(devid: u16) -> Result<PlatformDescriptor, I915Error> {
    let mut i = 0;
    while i < ID_TABLE.len() {
        if ID_TABLE[i].devid == devid {
            let r = ID_TABLE[i];
            return Ok(PlatformDescriptor { devid, sub: r.sub, info: *r.info });
        }
        i += 1;
    }
    Err(I915Error::UnknownDevice { devid })
}
