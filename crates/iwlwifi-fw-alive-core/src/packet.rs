// SPDX-License-Identifier: GPL-2.0-only
//! The RX packet every notification and command response arrives in, ported from Linux
//! `drivers/net/wireless/intel/iwlwifi/iwl-trans.h:63-102` (`struct iwl_rx_packet`,
//! `iwl_rx_packet_len`, `iwl_rx_packet_payload_len`) and
//! `drivers/net/wireless/intel/iwlwifi/fw/api/cmdhdr.h:22, :65-98` (`struct iwl_cmd_header`).
//!
//! Copyright (C) 2005-2014, 2018-2025 Intel Corporation; Copyright (C) 2013-2015 Intel Mobile
//! Communications GmbH; Copyright (C) 2016-2017 Intel Deutschland GmbH.

pub const FH_RSCSR_FRAME_SIZE_MSK: u32 = 0x0000_3FFF; // iwl-trans.h:63
pub const FH_RSCSR_FRAME_INVALID: u32 = 0x5555_0000; // iwl-trans.h:64
pub const FH_RSCSR_RXQ_POS: u32 = 16; // iwl-trans.h:68
pub const FH_RSCSR_RXQ_MASK: u32 = 0x003F_0000; // iwl-trans.h:69
/// Set in `sequence` on everything the uCode originates. // cmdhdr.h:22
pub const SEQ_RX_FRAME: u16 = 0x8000; // cmdhdr.h:22
/// `len_n_flags` + `struct iwl_cmd_header` (cmd, group_id, le16 sequence). // iwl-trans.h:89-90; cmdhdr.h:65-98
pub const RX_PACKET_HDR_LEN: usize = 4 + 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RxPacket<'a> {
    pub len_n_flags: u32,
    pub cmd: u8,
    pub group_id: u8,
    pub sequence: u16,
    /// `pkt->data`, `iwl_rx_packet_payload_len` bytes. // iwl-trans.h:99-102
    pub payload: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketError {
    /// Fewer bytes than the 8-byte header.
    TooShort { len: usize },
    /// The FH left its 0x5555xxxx fill: the buffer was never written. // iwl-trans.h:64
    FrameInvalid { len_n_flags: u32 },
    /// The frame size is below the 4-byte command header it counts. // iwl-trans.h:99-102
    SizeBelowHeader { size: u32 },
    /// The frame size claims more bytes than the buffer holds.
    SizeOverrun { size: u32, avail: usize },
}

/// Parse one receive buffer that starts at its `len_n_flags`.
pub fn parse_rx_packet(rb: &[u8]) -> Result<RxPacket<'_>, PacketError> {
    if rb.len() < RX_PACKET_HDR_LEN {
        return Err(PacketError::TooShort { len: rb.len() });
    }
    let len_n_flags = u32::from_le_bytes([rb[0], rb[1], rb[2], rb[3]]);
    if len_n_flags & 0xFFFF_0000 == FH_RSCSR_FRAME_INVALID {
        return Err(PacketError::FrameInvalid { len_n_flags });
    }
    let size = len_n_flags & FH_RSCSR_FRAME_SIZE_MSK; // iwl-trans.h:96
    if size < 4 {
        return Err(PacketError::SizeBelowHeader { size });
    }
    let avail = rb.len() - 4;
    if size as usize > avail {
        return Err(PacketError::SizeOverrun { size, avail });
    }
    Ok(RxPacket {
        len_n_flags,
        cmd: rb[4],      // cmdhdr.h:69
        group_id: rb[5], // cmdhdr.h:73
        sequence: u16::from_le_bytes([rb[6], rb[7]]), // cmdhdr.h:97
        payload: &rb[RX_PACKET_HDR_LEN..4 + size as usize],
    })
}

impl RxPacket<'_> {
    /// uCode-originated (a notification), not a command response. // cmdhdr.h:22
    pub fn is_notification(&self) -> bool {
        self.sequence & SEQ_RX_FRAME != 0
    }

    /// The RX queue the FH delivered on. // iwl-trans.h:68-69
    pub fn rx_queue(&self) -> u8 {
        ((self.len_n_flags & FH_RSCSR_RXQ_MASK) >> FH_RSCSR_RXQ_POS) as u8
    }
}
