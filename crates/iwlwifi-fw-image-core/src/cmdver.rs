// SPDX-License-Identifier: GPL-2.0-only
//! `IWL_UCODE_TLV_CMD_VERSIONS` and its two lookups. Ported from Linux `iwl-drv.c:1375-1391`,
//! `fw/file.h:1027-1041`, `fw/img.c:9-54`, `fw/api/commands.h:40`.

use iwlwifi_fw_tlv_core::container::{tlvs, ContainerError};

pub const IWL_UCODE_TLV_CMD_VERSIONS: u32 = 48; // fw/file.h:88
/// sizeof(struct iwl_fw_cmd_version): cmd, group, cmd_ver, notif_ver. // fw/file.h:1036-1041
pub const CMD_VERSION_LEN: usize = 4;
/// "no version recorded": the lookups answer the caller's default. // fw/file.h:1027
pub const IWL_FW_CMD_VER_UNKNOWN: u8 = 99; // fw/file.h:1027
/// Group 0 commands are recorded under LONG_GROUP. // fw/img.c:14
pub const LONG_GROUP: u8 = 0x1; // fw/api/commands.h:40

/// The table, borrowed out of the blob (empty when the file has none).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CmdVersions<'a> {
    raw: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmdVerError {
    Container(ContainerError),
    /// `WARN_ON(capa->cmd_versions)`: a second CMD_VERSIONS TLV. // iwl-drv.c:1383-1384
    Duplicate,
}

/// A trailing partial entry is dropped, as `tlv_len /= 4; tlv_len *= 4`. // iwl-drv.c:1376-1382
pub fn cmd_versions(blob: &[u8]) -> Result<CmdVersions<'_>, CmdVerError> {
    let mut found: Option<&[u8]> = None;
    for t in tlvs(blob).map_err(CmdVerError::Container)? {
        let t = t.map_err(CmdVerError::Container)?;
        if t.ty == IWL_UCODE_TLV_CMD_VERSIONS {
            if found.is_some() {
                return Err(CmdVerError::Duplicate);
            }
            found = Some(&t.data[..t.data.len() / CMD_VERSION_LEN * CMD_VERSION_LEN]);
        }
    }
    Ok(CmdVersions { raw: found.unwrap_or(&[]) })
}

impl CmdVersions<'_> {
    pub fn len(&self) -> usize {
        self.raw.len() / CMD_VERSION_LEN
    }

    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }

    fn find(&self, group: u8, cmd: u8) -> Option<&[u8]> {
        self.raw.chunks_exact(CMD_VERSION_LEN).find(|e| e[1] == group && e[0] == cmd)
    }

    /// `iwl_fw_lookup_cmd_ver(fw, cmd_id, def)`; `group` is the command's group (0 = legacy).
    /// // fw/img.c:9-31
    pub fn cmd_ver(&self, group: u8, cmd: u8, def: u8) -> u8 {
        let grp = if group == 0 { LONG_GROUP } else { group }; // fw/img.c:14
        match self.find(grp, cmd) {
            Some(e) if e[2] != IWL_FW_CMD_VER_UNKNOWN => e[2], // fw/img.c:24-26
            _ => def,
        }
    }

    /// `iwl_fw_lookup_notif_ver(fw, grp, cmd, def)`: the group is NOT remapped. // fw/img.c:34-53
    pub fn notif_ver(&self, group: u8, cmd: u8, def: u8) -> u8 {
        match self.find(group, cmd) {
            Some(e) if e[3] != IWL_FW_CMD_VER_UNKNOWN => e[3], // fw/img.c:46-48
            _ => def,
        }
    }
}
