// SPDX-License-Identifier: GPL-2.0-only
//! Host-command payload encoders. Each writes the little-endian, `__packed` Linux layout into a
//! caller buffer and returns the length, or names why it refused. Ported from Linux
//! `fw/api/config.h`, `fw/api/nvm-reg.h`, `mvm/fw.c` (`iwl_send_tx_ant_cfg`, `iwl_send_phy_cfg_cmd`),
//! `mvm/mvm.h` (`iwl_mvm_get_phy_config`) and `mvm/nvm.c` (`iwl_nvm_read_chunk`).

use crate::ids::*;

/// An encoder refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeError {
    /// The caller buffer cannot hold the command.
    BufferTooSmall { need: usize, have: usize },
}

fn check(buf: &[u8], need: usize) -> Result<(), EncodeError> {
    if buf.len() < need {
        return Err(EncodeError::BufferTooSmall { need, have: buf.len() });
    }
    Ok(())
}

/// `struct iwl_tx_ant_cfg_cmd { __le32 valid; }` (fw/api/config.h:22-24), built as
/// `iwl_send_tx_ant_cfg` does (mvm/fw.c:36-40): `.valid = cpu_to_le32(valid_tx_ant)`.
pub fn encode_tx_ant_cfg(valid_tx_ant: u8, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, TX_ANT_CFG_CMD_LEN)?;
    buf[0..4].copy_from_slice(&(valid_tx_ant as u32).to_le_bytes());
    Ok(TX_ANT_CFG_CMD_LEN)
}

/// `iwl_mvm_get_phy_config` (mvm/mvm.h:1789-1800): the firmware's PHY_SKU TLV value
/// (`fw.phy_config`, iwl-drv.c:1023) with both chain fields replaced by the valid antennas. The
/// shifts are NOT masked to four bits, exactly as Linux — an antenna value above 0xf spills into the
/// neighbouring field before the final `&` with the firmware value.
pub fn phy_config(fw_phy_config: u32, valid_tx_ant: u8, valid_rx_ant: u8) -> u32 {
    let mask = !(FW_PHY_CFG_TX_CHAIN | FW_PHY_CFG_RX_CHAIN);
    let cfg = mask
        | ((valid_tx_ant as u32) << FW_PHY_CFG_TX_CHAIN_POS)
        | ((valid_rx_ant as u32) << FW_PHY_CFG_RX_CHAIN_POS);
    fw_phy_config & cfg
}

/// The values `iwl_send_phy_cfg_cmd` (mvm/fw.c:534-577) puts in `struct iwl_phy_cfg_cmd_v1`
/// (fw/api/config.h:84-87).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhyCfgV1 {
    /// Final `phy_cfg`: [`phy_config`] `|` `mac_cfg->extra_phy_cfg_flags` (mvm/fw.c:559-560).
    pub phy_cfg: u32,
    /// `default_calib[INIT].flow_trigger` from the DEF_CALIB TLV (iwl-drv.c:532, mvm/fw.c:565).
    pub flow_trigger: u32,
    /// `default_calib[INIT].event_trigger` from the DEF_CALIB TLV (iwl-drv.c:534, mvm/fw.c:563).
    pub event_trigger: u32,
}

impl PhyCfgV1 {
    /// Build the command values exactly as `iwl_send_phy_cfg_cmd` does: masked PHY_SKU value, then
    /// `|= extra_phy_cfg_flags` (mvm/fw.c:559-560), then the DEF_CALIB triggers (mvm/fw.c:562-565).
    pub fn new(
        fw_phy_config: u32,
        valid_tx_ant: u8,
        valid_rx_ant: u8,
        extra_phy_cfg_flags: u32,
        flow_trigger: u32,
        event_trigger: u32,
    ) -> Self {
        PhyCfgV1 {
            phy_cfg: phy_config(fw_phy_config, valid_tx_ant, valid_rx_ant) | extra_phy_cfg_flags,
            flow_trigger,
            event_trigger,
        }
    }
}

/// `struct iwl_phy_cfg_cmd_v1 { le32 phy_cfg; struct iwl_calib_ctrl { le32 flow_trigger; le32
/// event_trigger; } calib_control; }` (fw/api/config.h:34-37, :84-87).
pub fn encode_phy_cfg_v1(c: &PhyCfgV1, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, PHY_CFG_CMD_V1_LEN)?;
    buf[0..4].copy_from_slice(&c.phy_cfg.to_le_bytes());
    buf[4..8].copy_from_slice(&c.flow_trigger.to_le_bytes());
    buf[8..12].copy_from_slice(&c.event_trigger.to_le_bytes());
    Ok(PHY_CFG_CMD_V1_LEN)
}

/// `struct iwl_nvm_access_cmd` for a READ, as `iwl_nvm_read_chunk` builds it (mvm/nvm.c:75-80):
/// `op_code = NVM_READ_OPCODE`, `target` left 0, `type = section`, `offset`, `length`, no data.
/// Layout `u8 op_code; u8 target; le16 type; le16 offset; le16 length;` (fw/api/nvm-reg.h:108-115).
pub fn encode_nvm_read(section: u16, offset: u16, length: u16, buf: &mut [u8]) -> Result<usize, EncodeError> {
    check(buf, NVM_ACCESS_CMD_LEN)?;
    buf[0] = NVM_READ_OPCODE;
    buf[1] = NVM_ACCESS_TARGET_CACHE;
    buf[2..4].copy_from_slice(&section.to_le_bytes());
    buf[4..6].copy_from_slice(&offset.to_le_bytes());
    buf[6..8].copy_from_slice(&length.to_le_bytes());
    Ok(NVM_ACCESS_CMD_LEN)
}
