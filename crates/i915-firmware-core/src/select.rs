// SPDX-License-Identifier: GPL-2.0-only
//! Which DMC file a Kaby Lake machine loads: `dmc_firmware_default()` (intel_dmc.c:225-251).

/// Linux's per-platform choice: path under /lib/firmware plus the payload ceiling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareChoice {
    pub path: &'static str,
    pub max_fw_size: u32,
    /// The version the file name promises (`DMC_LEGACY_PATH(kbl, 1, 04)` = ver1_04) as DMC_VERSION(major, minor).
    pub file_version: u32,
}

/// `#define BXT_DMC_MAX_FW_SIZE 0x3000` — intel_dmc.c:185.
pub const BXT_DMC_MAX_FW_SIZE: u32 = 0x3000;
/// `#define KBL_DMC_MAX_FW_SIZE BXT_DMC_MAX_FW_SIZE` — intel_dmc.c:177.
pub const KBL_DMC_MAX_FW_SIZE: u32 = BXT_DMC_MAX_FW_SIZE;
/// `#define KBL_DMC_PATH DMC_LEGACY_PATH(kbl, 1, 04)` — intel_dmc.c:176 (the macro: intel_dmc.c:119-123).
pub const KBL_DMC_PATH: &str = "i915/kbl_dmc_ver1_04.bin";

/// `DMC_VERSION(major, minor) ((major) << 16 | (minor))` — intel_dmc.c:107.
pub const fn dmc_version(major: u32, minor: u32) -> u32 {
    (major << 16) | minor
}
/// `DMC_VERSION_MAJOR(version) ((version) >> 16)` — intel_dmc.c:108.
pub const fn dmc_version_major(version: u32) -> u32 {
    version >> 16
}
/// `DMC_VERSION_MINOR(version) ((version) & 0xffff)` — intel_dmc.c:109.
pub const fn dmc_version_minor(version: u32) -> u32 {
    version & 0xffff
}

/// The branch `IS_KABYLAKE || IS_COFFEELAKE || IS_COMETLAKE` of `dmc_firmware_default()` — intel_dmc.c:237-239.
pub const fn kbl_firmware() -> FirmwareChoice {
    FirmwareChoice { path: KBL_DMC_PATH, max_fw_size: KBL_DMC_MAX_FW_SIZE, file_version: dmc_version(1, 4) }
}
