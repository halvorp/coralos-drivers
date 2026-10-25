// SPDX-License-Identifier: GPL-2.0-only
//! `IWL_UCODE_TLV_DEF_CALIB`: the default calibration triggers a PHY_CONFIGURATION_CMD carries.
//! Ported from Linux `iwl-drv.c:402-410, :522-538, :1014-1019`, `fw/file.h:597-600`.

use crate::sections::UcodeType;
use iwlwifi_fw_tlv_core::container::{tlvs, ContainerError};

pub const IWL_UCODE_TLV_DEF_CALIB: u32 = 22; // fw/file.h:71
/// `IWL_UCODE_TYPE_MAX`: a ucode_type at or past it is refused. // fw/img.h:30
pub const IWL_UCODE_TYPE_MAX: u32 = 4; // fw/img.h:30
/// sizeof(struct iwl_tlv_calib_data): ucode_type + flow_trigger + event_trigger. // iwl-drv.c:407-410
pub const CALIB_DATA_LEN: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CalibCtrl {
    pub flow_trigger: u32,
    pub event_trigger: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibError {
    Container(ContainerError),
    /// "TLV %d has invalid size". // iwl-drv.c:1015-1016
    InvalidTlvLen { len: usize },
    /// "Wrong ucode_type %u for default calibration." // iwl-drv.c:527-530
    WrongUcodeType { ucode_type: u32 },
}

/// The default calibration for `ty`; the last DEF_CALIB for that type wins, as the Linux loop
/// overwrites `drv->fw.default_calib[ucode_type]`. `Ok(None)` is "never set" (zeroes in Linux).
pub fn default_calib(blob: &[u8], ty: UcodeType) -> Result<Option<CalibCtrl>, CalibError> {
    let mut found = None;
    for t in tlvs(blob).map_err(CalibError::Container)? {
        let t = t.map_err(CalibError::Container)?;
        if t.ty != IWL_UCODE_TLV_DEF_CALIB {
            continue;
        }
        if t.data.len() != CALIB_DATA_LEN {
            return Err(CalibError::InvalidTlvLen { len: t.data.len() });
        }
        let w = |i: usize| u32::from_le_bytes([t.data[i], t.data[i + 1], t.data[i + 2], t.data[i + 3]]);
        let ucode_type = w(0);
        if ucode_type >= IWL_UCODE_TYPE_MAX {
            return Err(CalibError::WrongUcodeType { ucode_type });
        }
        if ucode_type == ty as u32 {
            found = Some(CalibCtrl { flow_trigger: w(4), event_trigger: w(8) }); // iwl-drv.c:532-535
        }
    }
    Ok(found)
}
