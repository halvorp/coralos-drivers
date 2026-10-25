// SPDX-License-Identifier: GPL-2.0-only
//! The `ADD_STA_KEY` response (`iwl_mvm_send_cmd_pdu_status`, mvm/utils.c): one little-endian
//! status word. `iwl_mvm_send_sta_key` accepts only `ADD_STA_SUCCESS`.

use crate::ids::ADD_STA_SUCCESS;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusRefusal {
    /// The response payload is not four bytes; carries its length.
    Length(usize),
    /// A status other than `ADD_STA_SUCCESS`; carries the word (sta.c:3677 turns it into -EIO).
    NotSuccess(u32),
}

pub fn status(resp: &[u8]) -> Result<(), StatusRefusal> {
    if resp.len() != 4 {
        return Err(StatusRefusal::Length(resp.len()));
    }
    let s = u32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
    if s == ADD_STA_SUCCESS {
        Ok(())
    } else {
        Err(StatusRefusal::NotSuccess(s))
    }
}
