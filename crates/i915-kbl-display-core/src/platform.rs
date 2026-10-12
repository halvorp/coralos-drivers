// SPDX-License-Identifier: GPL-2.0-only
//! The Kaby Lake PCI device-id table and the display capabilities `skl_display` gives every member of it.
//!
//! Ported from Linux include/drm/intel/pciids.h (the INTEL_KBL_* / INTEL_AML_KBL_GT2_IDS macros),
//! drivers/gpu/drm/i915/i915_pci.c (`kbl_gt1_info`..`kbl_gt3_info`, the PCI id list) and
//! drivers/gpu/drm/i915/display/intel_display_device.c (`skl_display`, `kbl_desc`, `kbl_ult_ids`, `kbl_ulx_ids`,
//! `kbl_steppings`). Copyright (c) Intel Corporation.
//!
//! A device id that is not in [`KBL_IDS`] is REFUSED BY NAME ([`PlatformError`]). It is never aliased to another
//! generation's row: aliasing 8086:591c to the Cherry Trail (Gen8) row would replace an honest refusal with a driver
//! that programs the wrong registers (Astra's ruling, 2026-10-12).

use crate::types::{Ddi, Pipe, Transcoder};

/// PCI_VENDOR_ID_INTEL (include/linux/pci_ids.h:2688).
pub const PCI_VENDOR_ID_INTEL: u16 = 0x8086;

/// Which display-device sub-platform group an id belongs to (`kbl_ult_ids` / `kbl_ulx_ids`,
/// intel_display_device.c:723-735, :747-760). The display capabilities are identical for all three.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubPlatform {
    /// In neither list (desktop, server, halo, workstation, plain mobile).
    None,
    /// `kbl_ult_ids` (INTEL_KBL_ULT_GT1/GT2/GT3_IDS).
    Ult,
    /// `kbl_ulx_ids` (INTEL_KBL_ULX_GT1/GT2_IDS and INTEL_AML_KBL_GT2_IDS).
    Ulx,
}

/// One Kaby Lake PCI device id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KblId {
    pub device_id: u16,
    /// The `.gt` of the `intel_device_info` the id selects (i915_pci.c:546-561); GT4's 0x593b uses `kbl_gt3_info`.
    pub gt: u8,
    pub sub: SubPlatform,
}

const fn id(device_id: u16, gt: u8, sub: SubPlatform) -> KblId {
    KblId { device_id, gt, sub }
}

/// Every id Linux binds to the Kaby Lake rows. Order and membership follow `INTEL_KBL_IDS` (pciids.h:495).
pub const KBL_IDS: [KblId; 22] = [
    id(0x5906, 1, SubPlatform::Ult), // pciids.h:411 INTEL_KBL_ULT_GT1_IDS, ULT GT1
    id(0x5913, 1, SubPlatform::Ult), // pciids.h:412 INTEL_KBL_ULT_GT1_IDS, ULT GT1.5
    id(0x590e, 1, SubPlatform::Ulx), // pciids.h:415 INTEL_KBL_ULX_GT1_IDS, ULX GT1
    id(0x5915, 1, SubPlatform::Ulx), // pciids.h:416 INTEL_KBL_ULX_GT1_IDS, ULX GT1.5
    id(0x5902, 1, SubPlatform::None), // pciids.h:421 DT GT1
    id(0x5908, 1, SubPlatform::None), // pciids.h:422 Halo GT1
    id(0x590a, 1, SubPlatform::None), // pciids.h:423 SRV GT1
    id(0x590b, 1, SubPlatform::None), // pciids.h:424 Halo GT1
    id(0x5916, 2, SubPlatform::Ult), // pciids.h:427 INTEL_KBL_ULT_GT2_IDS, ULT GT2
    id(0x5921, 2, SubPlatform::Ult), // pciids.h:428 INTEL_KBL_ULT_GT2_IDS, ULT GT2F
    id(0x591e, 2, SubPlatform::Ulx), // pciids.h:431 INTEL_KBL_ULX_GT2_IDS, ULX GT2
    id(0x5912, 2, SubPlatform::None), // pciids.h:436 DT GT2
    id(0x5917, 2, SubPlatform::None), // pciids.h:437 Mobile GT2
    id(0x591a, 2, SubPlatform::None), // pciids.h:438 SRV GT2
    id(0x591b, 2, SubPlatform::None), // pciids.h:439 Halo GT2
    id(0x591d, 2, SubPlatform::None), // pciids.h:440 WKS GT2
    id(0x591c, 2, SubPlatform::Ulx), // pciids.h:455 INTEL_AML_KBL_GT2_IDS, ULX GT2 (the Pixelbook Go)
    id(0x87c0, 2, SubPlatform::Ulx), // pciids.h:456 INTEL_AML_KBL_GT2_IDS, ULX GT2
    id(0x5926, 3, SubPlatform::Ult), // pciids.h:443 INTEL_KBL_ULT_GT3_IDS, ULT GT3
    id(0x5923, 3, SubPlatform::None), // pciids.h:447 ULT GT3 (in INTEL_KBL_GT3_IDS but not in kbl_ult_ids)
    id(0x5927, 3, SubPlatform::None), // pciids.h:448 ULT GT3 (likewise)
    id(0x593b, 3, SubPlatform::None), // pciids.h:451 Halo GT4; i915_pci.c:839 binds it to kbl_gt3_info (.gt = 3)
];

/// The Pixelbook Go's GPU (ATLAS): 8086:591c.
pub const PIXELBOOK_GO_DEVICE_ID: u16 = 0x591c; // pciids.h:455

/// `skl_display.__runtime_defaults.ip.ver` (intel_display_device.c:675): the display version of every KBL part.
pub const DISPLAY_VER: u8 = 9; // intel_display_device.c:675

/// The display capabilities `skl_display` gives Kaby Lake (intel_display_device.c:661-684), plus the per-pipe scaler
/// counts `intel_display_device_info_runtime_init` gives display version 9 (intel_display_device.c:1783-1785).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayCaps {
    pub display_ver: u8,
    /// Bit i = `Pipe` i present (`pipe_mask`, intel_display_device.c:678).
    pub pipe_mask: u8,
    /// Bit `Transcoder as u8` (`cpu_transcoder_mask`, intel_display_device.c:679-681).
    pub transcoder_mask: u8,
    /// Bit i = `Ddi` i present (`port_mask`, intel_display_device.c:682).
    pub port_mask: u8,
    /// `dbuf.size` in blocks (intel_display_device.c:662): 896 less 4 for the bypass path.
    pub dbuf_blocks: u16,
    /// Scalers per pipe A, B, C (intel_display_device.c:1783-1785).
    pub scalers: [u8; 3],
    pub has_ddi: bool,    // intel_display_device.c:664
    pub has_dp_mst: bool, // intel_display_device.c:665
    pub has_psr: bool,    // intel_display_device.c:669
    pub has_dmc: bool,    // intel_display_device.c:676 (__runtime_defaults.has_dmc)
}

/// The one set of capabilities every Kaby Lake id gets (`kbl_desc.info = &skl_display`, intel_display_device.c:760).
pub const KBL_DISPLAY: DisplayCaps = DisplayCaps {
    display_ver: DISPLAY_VER,
    pipe_mask: 0b111,       // BIT(PIPE_A) | BIT(PIPE_B) | BIT(PIPE_C): intel_display_device.c:678
    transcoder_mask: 0b10111, // A | B | C | EDP (EDP = bit 4): intel_display_device.c:679-681
    port_mask: 0b11111,     // PORT_A..PORT_E: intel_display_device.c:682
    dbuf_blocks: 896 - 4,   // intel_display_device.c:662
    scalers: [2, 2, 1],     // intel_display_device.c:1783-1785
    has_ddi: true,
    has_dp_mst: true,
    has_psr: true,
    has_dmc: true,
};

impl DisplayCaps {
    pub fn has_pipe(&self, p: Pipe) -> bool {
        self.pipe_mask & (1 << p.index()) != 0
    }
    pub fn has_transcoder(&self, t: Transcoder) -> bool {
        self.transcoder_mask & t.mask_bit() != 0
    }
    pub fn has_port(&self, d: Ddi) -> bool {
        self.port_mask & (1 << d.index()) != 0
    }
    pub fn num_pipes(&self) -> u32 {
        self.pipe_mask.count_ones()
    }
    /// The scalers of `p` (`num_scalers[pipe]`).
    pub fn scalers_of(&self, p: Pipe) -> u8 {
        self.scalers[p.index()]
    }
}

/// `kbl_steppings` (intel_display_device.c:737-745): PCI revision id -> stepping name. Revision 0 and >= 8 are not in
/// the table; Linux leaves those at STEP_NONE, which this reports as `None`.
pub const fn kbl_stepping(revision: u8) -> Option<&'static str> {
    match revision {
        1 => Some("B0"), // intel_display_device.c:738
        2 => Some("B0"), // intel_display_device.c:739
        3 => Some("B0"), // intel_display_device.c:740
        4 => Some("C0"), // intel_display_device.c:741
        5 => Some("B1"), // intel_display_device.c:742
        6 => Some("B1"), // intel_display_device.c:743
        7 => Some("C0"), // intel_display_device.c:744
        _ => None,
    }
}

/// A named refusal to treat a device as Kaby Lake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlatformError {
    /// Not an Intel device (`PCI_VENDOR_ID_INTEL` in `INTEL_VGA_DEVICE`, pciids.h:35).
    NotIntel { vendor: u16 },
    /// An Intel device that Linux does not bind to a Kaby Lake row. Carries the id so the log names it.
    NotKabyLake { device: u16 },
}

impl PlatformError {
    pub fn name(&self) -> &'static str {
        match self {
            PlatformError::NotIntel { .. } => "not-intel",
            PlatformError::NotKabyLake { .. } => "not-kaby-lake",
        }
    }
}

/// What `identify` returns for a Kaby Lake device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KblDevice {
    pub id: KblId,
    pub stepping: Option<&'static str>,
    pub caps: DisplayCaps,
}

/// Look `vendor:device` (PCI revision `revision`) up in the Kaby Lake table.
pub fn identify(vendor: u16, device: u16, revision: u8) -> Result<KblDevice, PlatformError> {
    if vendor != PCI_VENDOR_ID_INTEL {
        return Err(PlatformError::NotIntel { vendor });
    }
    for e in KBL_IDS.iter() {
        if e.device_id == device {
            return Ok(KblDevice { id: *e, stepping: kbl_stepping(revision), caps: KBL_DISPLAY });
        }
    }
    Err(PlatformError::NotKabyLake { device })
}
