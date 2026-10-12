// SPDX-License-Identifier: GPL-2.0-only
//! Block 1 (general features) and block 2 (general definitions + the child device table):
//! `parse_general_features()` (intel_bios.c:1170), `parse_general_definitions()` (intel_bios.c:2886),
//! `child_device_expected_size()` / `child_device_size_valid()` (intel_bios.c:2833-2884).

use crate::bdb::{BlockId, Vbt};
use crate::child::{ChildDevice, LEGACY_CHILD_DEVICE_CONFIG_SIZE};
use crate::error::VbtError;
use alloc::vec::Vec;

/// sizeof(struct bdb_general_definitions) with its flexible `devices[]` excluded: crt_ddc_gmbus_pin, the DPMS
/// bits byte, boot_display[2], child_dev_size — intel_vbt_defs.h:571-596.
pub const GENERAL_DEFINITIONS_SIZE: usize = 5;

/// `child_device_expected_size()` — intel_bios.c:2833-2855. `None` is Linux's `-ENOENT` (version > 264).
pub const fn child_device_expected_size(version: u16) -> Option<usize> {
    if version > 264 {
        None
    } else if version >= 263 {
        Some(44)
    } else if version >= 256 {
        Some(40)
    } else if version >= 216 {
        Some(39)
    } else if version >= 196 {
        Some(38)
    } else if version >= 195 {
        Some(37)
    } else if version >= 111 {
        Some(LEGACY_CHILD_DEVICE_CONFIG_SIZE)
    } else if version >= 106 {
        Some(27)
    } else {
        Some(22)
    }
}

/// Everything the general definitions block yields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneralDefinitions {
    /// `crt_ddc_gmbus_pin`.
    pub crt_ddc_pin: u8,
    /// `child_dev_size` as stored.
    pub child_dev_size: u8,
    /// Linux logs "Unexpected child device config size" but continues; this carries (actual, expected).
    pub size_mismatch: Option<(usize, usize)>,
    /// Child devices with a non-zero `device_type` (parse_general_definitions skips type 0, intel_bios.c:2924).
    pub children: Vec<ChildDevice>,
}

impl GeneralDefinitions {
    /// The child whose handle is `handle` (e.g. DEVICE_HANDLE_LFP1).
    pub fn child_by_handle(&self, handle: u16) -> Option<&ChildDevice> {
        self.children.iter().find(|c| c.handle() == handle)
    }
}

/// Parse block 2. Mirrors `parse_general_definitions()`; Linux's silent early returns are named refusals.
pub fn parse_general_definitions(vbt: &Vbt) -> Result<GeneralDefinitions, VbtError> {
    let block = vbt.require_block(BlockId::GeneralDefinitions as u8)?;
    // `if (block_size < sizeof(*defs))` — intel_bios.c:2904
    if block.len() < GENERAL_DEFINITIONS_SIZE {
        return Err(VbtError::GeneralDefinitionsTooSmall { size: block.len() });
    }
    let crt_ddc_pin = block.u8_at(0);
    let child_dev_size = block.u8_at(4);
    let size = usize::from(child_dev_size);
    // child_device_size_valid(), intel_bios.c:2857-2884
    let expected = child_device_expected_size(vbt.version).unwrap_or(crate::child::CHILD_DEVICE_CONFIG_SIZE);
    let size_mismatch = if size != expected { Some((size, expected)) } else { None };
    if size < LEGACY_CHILD_DEVICE_CONFIG_SIZE {
        return Err(VbtError::ChildDeviceSizeTooSmall { size });
    }
    // `child_device_num = (block_size - sizeof(*defs)) / defs->child_dev_size` — intel_bios.c:2920
    let num = (block.len() - GENERAL_DEFINITIONS_SIZE) / size;
    let mut children = Vec::new();
    for i in 0..num {
        let off = GENERAL_DEFINITIONS_SIZE + i * size;
        let bytes = &block.data[off..off + size];
        // `if (!child->device_type) continue;` — intel_bios.c:2924
        if bytes[2] == 0 && bytes[3] == 0 {
            continue;
        }
        children.push(ChildDevice::from_bytes(vbt.version, bytes));
    }
    Ok(GeneralDefinitions { crt_ddc_pin, child_dev_size, size_mismatch, children })
}

/// Block 1 fields Linux copies into `display->vbt` (intel_bios.c:1170-1205), for a KBL-class DDI platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeneralFeatures {
    pub int_tv_support: bool,
    /// Only trusted on BDB >= 155 DDI platforms (intel_bios.c:1181); `None` below that.
    pub int_crt_support: Option<bool>,
    pub lvds_use_ssc: bool,
    /// kHz: `intel_bios_ssc_frequency()` default case, intel_bios.c:1156-1167 (display ver >= 5: 100000/120000).
    pub lvds_ssc_freq: u32,
    pub display_clock_mode: bool,
    pub fdi_rx_polarity_inverted: bool,
    /// `rotate_180` (BDB 181+); `None` before.
    pub rotate_180: Option<bool>,
}

/// `parse_general_features()` for display version >= 5 (HAS_DDI). Bit positions per `struct bdb_general_features`
/// (intel_vbt_defs.h:200-240): byte1 bit1 enable_ssc, bit2 ssc_freq, bit6 display_clock_mode; byte2 bit2 rotate_180,
/// bit3 fdi_rx_polarity_inverted; byte4 bit0 int_crt_support, bit1 int_tv_support.
pub fn parse_general_features(vbt: &Vbt) -> Result<Option<GeneralFeatures>, VbtError> {
    let Some(b) = vbt.find_block(BlockId::GeneralFeatures as u8)? else { return Ok(None) };
    b.require(5)?;
    let (b1, b2, b4) = (b.u8_at(1), b.u8_at(2), b.u8_at(4));
    let ssc_alt = b1 & (1 << 2) != 0;
    Ok(Some(GeneralFeatures {
        int_tv_support: b4 & (1 << 1) != 0,
        int_crt_support: if vbt.version >= 155 { Some(b4 & 1 != 0) } else { None },
        lvds_use_ssc: b1 & (1 << 1) != 0,
        lvds_ssc_freq: if ssc_alt { 100000 } else { 120000 },
        display_clock_mode: b1 & (1 << 6) != 0,
        fdi_rx_polarity_inverted: b2 & (1 << 3) != 0,
        rotate_180: if vbt.version >= 181 { Some(b2 & (1 << 2) != 0) } else { None },
    }))
}
