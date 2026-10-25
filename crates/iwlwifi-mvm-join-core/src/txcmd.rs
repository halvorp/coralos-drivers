// SPDX-License-Identifier: GPL-2.0-only
//! The `TX_CMD` (version 6) that precedes every transmitted frame, the layout of the transfer
//! buffers it travels in, the scheduler byte-count entry, and the `TX_CMD` response.
//! Ported from `mvm/tx.c` (`iwl_mvm_set_tx_cmd`, `iwl_mvm_set_tx_cmd_rate`, `iwl_mvm_set_tx_cmd_crypto`,
//! `iwl_mvm_rx_tx_cmd_single`), `fw/api/tx.h` and `pcie/gen1_2/tx.c` (`iwl_trans_pcie_tx`,
//! `iwl_txq_gen1_update_byte_cnt_tbl`).

use crate::ids::*;
use crate::{check, le16, le32, put16, put32, EncodeError};

/// What one transmitted frame says to the firmware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxParams {
    pub sta_id: u8,
    /// `IWL_TID_NON_QOS` (0) for non-QoS data, `IWL_MAX_TID_COUNT` (8) for management frames.
    pub tid: u8,
    /// Frame length in bytes from the 802.11 header through the payload, INCLUDING the 8-byte CCMP
    /// header of a protected frame and EXCLUDING the FCS and the CCMP MIC. // iwl_mvm_set_tx_cmd: skb->len
    pub frame_len: u16,
    /// The 802.11 header length (24 for everything this crate builds).
    pub hdr_len: u8,
    /// `rate_n_flags` (rates version 1), see [`crate::rates::rate_n_flags_v1`].
    pub rate_n_flags: u32,
    /// The pairwise key for a CCMP-protected data frame; `None` for everything else.
    pub key: Option<[u8; 16]>,
    /// Expect an ACK (every unicast frame).
    pub ack: bool,
    /// Let the firmware assign the sequence number (frames with a zero sequence-control field).
    pub fw_seq: bool,
    /// `pm_frame_timeout`: [`PM_FRAME_NONE`], [`PM_FRAME_MGMT`] or [`PM_FRAME_ASSOC`].
    pub pm_frame_timeout: u16,
    /// `iwl_mvm_bt_coex_tx_prio`: 3 for management frames on 2.4 GHz, 0 otherwise here.
    pub bt_prio: u8,
}

/// Build the 56-byte `struct iwl_tx_cmd_v6_params`. `scratch_phys` is the bus address of the
/// scratch field inside the first transfer buffer (`dram_lsb_ptr` / `dram_msb_ptr`).
pub fn tx_cmd_params(p: &TxParams, scratch_phys: u64, out: &mut [u8]) -> Result<usize, EncodeError> {
    check(out, TX_CMD_PARAMS_LEN)?;
    let mut flags = 0u32;
    if p.ack {
        flags |= TX_CMD_FLG_ACK;
    }
    if p.fw_seq {
        flags |= TX_CMD_FLG_SEQ_CTL;
    }
    flags |= (p.bt_prio as u32 & 3) << 11; // TX_CMD_FLG_BT_PRIO_POS
    let mh_len = p.hdr_len as u32 + if p.key.is_some() { 8 } else { 0 }; // mac header includes the IV on the old TX API
    let mut offload = (mh_len / 2) << TX_CMD_OFFLD_MH_SIZE;
    if p.hdr_len % 4 != 0 {
        offload |= 1 << TX_CMD_OFFLD_PAD;
    }
    put16(out, 0, p.frame_len);
    put16(out, 2, offload as u16);
    put32(out, 4, flags);
    // scratch @8 stays zero
    put32(out, 12, p.rate_n_flags);
    out[16] = p.sta_id;
    if let Some(k) = p.key {
        out[17] = TX_CMD_SEC_CCM;
        out[20..36].copy_from_slice(&k);
    }
    put32(out, 40, TX_CMD_LIFE_TIME_INFINITE);
    put32(out, 44, scratch_phys as u32);
    out[48] = (scratch_phys >> 32) as u8;
    out[49] = IWL_RTS_DFAULT_RETRY_LIMIT;
    out[50] = IWL_DEFAULT_TX_RETRY;
    out[51] = p.tid;
    put16(out, 52, p.pm_frame_timeout);
    Ok(TX_CMD_PARAMS_LEN)
}

/// How the device command + 802.11 header split into the first two transfer buffers.
/// TB0 is the first [`IWL_FIRST_TB_SIZE`] bytes of `{cmd header, tx params, header}`; TB1 is the rest,
/// rounded up to a dword with `TX_CMD_FLG_MH_PAD` set in the command when rounding added a pad.
/// // iwl_trans_pcie_tx, pcie/gen1_2/tx.c:2108-2200
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TbPlan {
    pub tb0_len: usize,
    pub tb1_len: usize,
    pub mh_pad: bool,
}

pub fn tb_plan(hdr_len: usize) -> TbPlan {
    let len = TX_CMD_PARAMS_LEN + CMD_HEADER_LEN + hdr_len - IWL_FIRST_TB_SIZE;
    let tb1_len = (len + 3) & !3;
    TbPlan { tb0_len: IWL_FIRST_TB_SIZE, tb1_len, mh_pad: tb1_len != len }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteCountRefusal {
    /// The scheduler length does not fit the 12-bit field; carries the value.
    TooLong(u32),
}

/// The scheduler byte-count entry (`struct iwl_bc_tbl_entry`): the frame length plus FCS and
/// delimiter and the CCMP MIC, in dwords for the 7000..AX210 families, with the station id in bits
/// 12..15. // iwl_txq_gen1_update_byte_cnt_tbl, pcie/gen1_2/tx.c:2061-2100
pub fn byte_count_entry(frame_len: u16, sec_ctl: u8, sta_id: u8) -> Result<u16, ByteCountRefusal> {
    let mut len = frame_len as u32 + IWL_TX_CRC_SIZE as u32 + IWL_TX_DELIMITER_SIZE as u32;
    if sec_ctl & TX_CMD_SEC_MSK == TX_CMD_SEC_CCM {
        len += 8; // IEEE80211_CCMP_MIC_LEN
    }
    len = len.div_ceil(4);
    if len > 0xfff {
        return Err(ByteCountRefusal::TooLong(len));
    }
    Ok(len as u16 | ((sta_id as u16) << 12))
}

/// The fields of a `TX_CMD` response the driver acts on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TxResp {
    pub frame_count: u8,
    pub failure_rts: u8,
    pub failure_frame: u8,
    pub initial_rate: u32,
    /// `agg_status[0].status & TX_STATUS_MSK`.
    pub status: u32,
    /// The SCD's starting sequence number after this response: frames before it are reclaimable.
    pub ssn: u16,
    pub ra: u8,
    pub tid: u8,
}

impl TxResp {
    pub fn success(&self) -> bool {
        self.status == TX_STATUS_SUCCESS || self.status == TX_STATUS_DIRECT_DONE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxRespRefusal {
    /// The payload cannot hold the fixed part, `frame_count` statuses and the SSN; carries both lengths.
    Short { have: usize, need: usize },
}

/// Decode a non-aggregated `TX_CMD` response of a firmware without the new TX API. // iwl_mvm_rx_tx_cmd_single,
/// mvm/tx.c:1547; iwl_mvm_get_scd_ssn, mvm/tx.c:1536
pub fn decode_tx_resp(payload: &[u8]) -> Result<TxResp, TxRespRefusal> {
    if payload.len() < TX_RESP_STATUS_OFF + 4 {
        return Err(TxRespRefusal::Short { have: payload.len(), need: TX_RESP_STATUS_OFF + 4 });
    }
    let frame_count = payload[0];
    let need = TX_RESP_STATUS_OFF + 4 * frame_count as usize + 4;
    if payload.len() < need || frame_count == 0 {
        return Err(TxRespRefusal::Short { have: payload.len(), need: need.max(TX_RESP_STATUS_OFF + 8) });
    }
    let ra_tid = payload[33];
    Ok(TxResp {
        frame_count,
        failure_rts: payload[2],
        failure_frame: payload[3],
        initial_rate: le32(payload, 4),
        status: le16(payload, TX_RESP_STATUS_OFF) as u32 & TX_STATUS_MSK,
        ssn: (le32(payload, TX_RESP_STATUS_OFF + 4 * frame_count as usize) & 0xfff) as u16,
        ra: ra_tid >> 4,
        tid: ra_tid & 0x0f,
    })
}
