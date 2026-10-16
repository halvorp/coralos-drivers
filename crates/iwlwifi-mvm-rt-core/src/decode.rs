// SPDX-License-Identifier: GPL-2.0-only
//! Decoders for the RT image's replies and notifications: the ADD_STA status, the MCC_UPDATE response (v3 layout),
//! `SCAN_COMPLETE_UMAC`, `SCAN_ITERATION_COMPLETE_UMAC`, and the `REPLY_RX_MPDU_CMD` descriptor (version-1 tail, the
//! layout of every family below AX210). A length that does not fit is a named refusal, never a read past the payload
//! (Linux casts `pkt->data` and trusts the firmware; the MCC response is the one it length-checks, mvm/nvm.c).

use crate::ids::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    /// The payload is shorter than the fixed part. `need` is the fixed size.
    Short { have: usize, need: usize },
    /// `payload_len != struct_size(resp, channels, n_channels)`. // mvm/nvm.c (iwl_mvm_update_mcc)
    McCLengthMismatch { have: usize, want: usize },
    /// "FW lied about packet len": `len + desc_size > pkt_len`. // mvm/rxmq.c:2158-2162
    RxLenLie { mpdu_len: usize, desc_size: usize, pkt_len: usize },
}

fn le32(p: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([p[o], p[o + 1], p[o + 2], p[o + 3]])
}
fn le16(p: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([p[o], p[o + 1]])
}

/// `iwl_mvm_add_sta_rsp.status` (the whole response is one `__le32`). // mvm/utils.c (iwl_mvm_send_cmd_status);
/// fw/api/sta.h (struct iwl_mvm_add_sta_rsp)
pub fn decode_add_sta_status(payload: &[u8]) -> Result<u32, DecodeError> {
    if payload.len() < 4 {
        return Err(DecodeError::Short { have: payload.len(), need: 4 });
    }
    Ok(le32(payload, 0))
}

/// Whether an ADD_STA status is success: `status & IWL_ADD_STA_STATUS_MASK == ADD_STA_SUCCESS`.
/// // mvm/sta.c:1730-1733
pub fn add_sta_ok(status: u32) -> bool {
    status & IWL_ADD_STA_STATUS_MASK == ADD_STA_SUCCESS
}

/// `struct iwl_mcc_update_resp_v3` header and the channel map. `channels` is the raw payload tail: `n_channels`
/// little-endian dwords, the low 16 bits of each the NVM channel flags. // fw/api/nvm-reg.h:319-328
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MccResp<'a> {
    pub status: u32,
    pub mcc: u16,
    pub cap: u8,
    pub source_id: u8,
    pub time: u16,
    pub geo_info: u16,
    pub n_channels: u32,
    channels: &'a [u8],
}

impl MccResp<'_> {
    /// The flags of channel `idx` in NVM channel order (`iwl_parse_nvm_mcc_info` indexes the same table).
    pub fn channel_flags(&self, idx: usize) -> Option<u16> {
        let o = idx.checked_mul(4)?;
        Some(le16(self.channels.get(o..o + 4)?, 0))
    }
}

/// `iwl_mvm_update_mcc`'s v3 branch: length-check `MCC_UPDATE_RESP_V3_HDR_LEN + 4 * n_channels` exactly, and map a zero
/// MCC (the firmware's world-domain quirk) to "00". // mvm/nvm.c (iwl_mvm_update_mcc); fw/api/nvm-reg.h:319-328
pub fn decode_mcc_resp_v3(payload: &[u8]) -> Result<MccResp<'_>, DecodeError> {
    if payload.len() < MCC_UPDATE_RESP_V3_HDR_LEN {
        return Err(DecodeError::Short { have: payload.len(), need: MCC_UPDATE_RESP_V3_HDR_LEN });
    }
    let n = le32(payload, 12);
    let want = (n as usize).checked_mul(4).and_then(|c| c.checked_add(MCC_UPDATE_RESP_V3_HDR_LEN)).unwrap_or(usize::MAX);
    if payload.len() != want {
        return Err(DecodeError::McCLengthMismatch { have: payload.len(), want });
    }
    let mut mcc = le16(payload, 4);
    if mcc == 0 {
        mcc = 0x3030; // W/A for a FW/NVM issue - returns 0x00 for the world domain
    }
    Ok(MccResp { status: le32(payload, 0), mcc, cap: payload[6], source_id: payload[7], time: le16(payload, 8), geo_info: le16(payload, 10), n_channels: n, channels: &payload[MCC_UPDATE_RESP_V3_HDR_LEN..] })
}

/// `struct iwl_umac_scan_complete`. // fw/api/scan.h:1169-1177
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanComplete {
    pub uid: u32,
    pub last_schedule: u8,
    pub last_iter: u8,
    /// `IWL_SCAN_OFFLOAD_COMPLETED` (1) or `IWL_SCAN_OFFLOAD_ABORTED` (2). // fw/api/scan.h:184-185
    pub status: u8,
    pub ebs_status: u8,
    pub time_from_last_iter: u32,
}

pub fn decode_scan_complete(payload: &[u8]) -> Result<ScanComplete, DecodeError> {
    if payload.len() < UMAC_SCAN_COMPLETE_LEN {
        return Err(DecodeError::Short { have: payload.len(), need: UMAC_SCAN_COMPLETE_LEN });
    }
    Ok(ScanComplete { uid: le32(payload, 0), last_schedule: payload[4], last_iter: payload[5], status: payload[6], ebs_status: payload[7], time_from_last_iter: le32(payload, 8) })
}

/// `struct iwl_umac_scan_iter_complete_notif` header. // fw/api/scan.h:1284-1292
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScanIterComplete {
    pub uid: u32,
    pub scanned_channels: u8,
    pub status: u8,
    pub bt_status: u8,
    pub last_channel: u8,
    pub start_tsf: u64,
}

pub fn decode_scan_iter_complete(payload: &[u8]) -> Result<ScanIterComplete, DecodeError> {
    if payload.len() < UMAC_SCAN_ITER_COMPLETE_HDR_LEN {
        return Err(DecodeError::Short { have: payload.len(), need: UMAC_SCAN_ITER_COMPLETE_HDR_LEN });
    }
    let mut tsf = [0u8; 8];
    tsf.copy_from_slice(&payload[8..16]);
    Ok(ScanIterComplete { uid: le32(payload, 0), scanned_channels: payload[4], status: payload[5], bt_status: payload[6], last_channel: payload[7], start_tsf: u64::from_le_bytes(tsf) })
}

/// What the RX_MPDU descriptor says about one received frame. // fw/api/rx.h:651-737; mvm/rxmq.c:2100-2240
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxMpdu<'a> {
    pub crc_ok: bool,
    /// The channel the card was on when it received the frame (`desc->v1.channel`). // mvm/rxmq.c:2143
    pub channel: u8,
    /// `max(-energy_a, -energy_b)` in dBm, an absent chain (energy 0) counting as -128. // mvm/rxmq.c:297-320
    pub rssi: i16,
    /// The 802.11 header length in bytes (`mac_flags2 & HDR_LEN_MASK`, 2-byte words). // fw/api/rx.h:184-186
    pub hdr_len: usize,
    /// The MPDU as it arrived: header, (2 pad bytes when the device inserted them), body.
    pub mpdu: &'a [u8],
    pub padded: bool,
}

/// Decode a `REPLY_RX_MPDU_CMD` payload for a family below AX210 (the version-1 descriptor tail). The MPDU is
/// `mpdu_len` bytes after the descriptor, less the `mic_crc_len` the hardware left on (mvm/rxmq.c:99-114).
pub fn decode_rx_mpdu_v1(payload: &[u8]) -> Result<RxMpdu<'_>, DecodeError> {
    let desc = IWL_RX_DESC_SIZE_V1;
    if payload.len() < desc {
        return Err(DecodeError::Short { have: payload.len(), need: desc }); // "Bad REPLY_RX_MPDU_CMD size", rxmq.c:2119-2123
    }
    let mpdu_len = le16(payload, 0) as usize;
    let mac_flags1 = payload[2];
    let mac_flags2 = payload[3];
    if mpdu_len + desc > payload.len() {
        return Err(DecodeError::RxLenLie { mpdu_len, desc_size: desc, pkt_len: payload.len() }); // rxmq.c:2158-2162
    }
    let status = le32(payload, 12);
    let mic_crc_len = (((mac_flags1 & IWL_RX_MPDU_MFLG1_MIC_CRC_LEN_MASK) >> 4) << 1) as usize; // u8_get_bits(…) << 1
    let mut len = mpdu_len;
    if len > mic_crc_len {
        len -= mic_crc_len; // rxmq.c:113-114
    }
    let energy_a = payload[32] as i16;
    let energy_b = payload[33] as i16;
    let a = if energy_a != 0 { -energy_a } else { -128 }; // S8_MIN
    let b = if energy_b != 0 { -energy_b } else { -128 };
    Ok(RxMpdu {
        crc_ok: status & (IWL_RX_MPDU_STATUS_CRC_OK | IWL_RX_MPDU_STATUS_OVERRUN_OK) == (IWL_RX_MPDU_STATUS_CRC_OK | IWL_RX_MPDU_STATUS_OVERRUN_OK),
        channel: payload[34],
        rssi: a.max(b),
        hdr_len: ((mac_flags2 & 0x1f) as usize) * 2,
        mpdu: &payload[desc..desc + len],
        padded: mac_flags2 & IWL_RX_MPDU_MFLG2_PAD != 0,
    })
}
