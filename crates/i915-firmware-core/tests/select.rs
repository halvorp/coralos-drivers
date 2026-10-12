// SPDX-License-Identifier: GPL-2.0-only
use i915_firmware_core::select::*;
use i915_firmware_core::kbl_firmware;

#[test]
fn kbl_picks_ver1_04_with_the_bxt_ceiling() {
    // intel_dmc.c:176-177, 185: KBL_DMC_PATH = kbl_dmc_ver1_04.bin; max = BXT_DMC_MAX_FW_SIZE = 0x3000
    let c = kbl_firmware();
    assert_eq!(c.path, "i915/kbl_dmc_ver1_04.bin");
    assert_eq!(c.max_fw_size, 0x3000);
    assert_eq!(KBL_DMC_PATH, "i915/kbl_dmc_ver1_04.bin");
    assert_eq!(BXT_DMC_MAX_FW_SIZE, 0x3000);
    assert_eq!(KBL_DMC_MAX_FW_SIZE, 0x3000);
    assert_eq!(c.file_version, 0x0001_0004);
}

#[test]
fn version_macros() {
    // intel_dmc.c:107-109
    assert_eq!(dmc_version(1, 4), 0x0001_0004);
    assert_eq!(dmc_version(2, 0x16), 0x0002_0016);
    assert_eq!(dmc_version_major(0x0002_0016), 2);
    assert_eq!(dmc_version_minor(0x0002_0016), 0x16);
    assert_eq!(dmc_version_minor(0xffff_ffff), 0xffff);
    assert_eq!(dmc_version_major(0xffff_ffff), 0xffff);
}
