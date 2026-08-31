// SPDX-License-Identifier: GPL-2.0-only
//! Linked List Item layout and LLP word helpers, ported from Linux
//! `drivers/dma/dw/regs.h`.
//!
//! Original copyright holders: Atmel Corporation, ST Microelectronics, Intel
//! Corporation, Haavard Skinnemoen, and Viresh Kumar.

/// Byte offsets of every word in Linux's seven-word `struct dw_lli` (`regs.h:369-382`).
pub mod offset {
    pub const SAR: usize = 0x00; // regs.h:371
    pub const DAR: usize = 0x04; // regs.h:372
    pub const LLP: usize = 0x08; // regs.h:373
    pub const CTL_LO: usize = 0x0c; // regs.h:374
    pub const CTL_HI: usize = 0x10; // regs.h:376
    pub const SSTAT: usize = 0x14; // regs.h:380
    pub const DSTAT: usize = 0x18; // regs.h:381
}

/// Names of all seven Linux `struct dw_lli` words, in memory order.
pub const LLI_FIELD_NAMES: [&str; 7] = [
    "sar",   // regs.h:371
    "dar",   // regs.h:372
    "llp",   // regs.h:373
    "ctllo", // regs.h:374
    "ctlhi", // regs.h:376
    "sstat", // regs.h:380
    "dstat", // regs.h:381
];

/// Size in bytes of Linux's seven `__le32` LLI words (`regs.h:369-382`).
pub const LLI_SIZE: usize = 0x1c; // regs.h:369-382

/// The exact hardware descriptor prefix from Linux's `struct dw_lli`.
///
/// Values are stored as little-endian byte arrays, so the representation is
/// portable without unsafe code or host-endian assumptions.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lli {
    pub sar: [u8; 4],
    pub dar: [u8; 4],
    pub llp: [u8; 4],
    pub ctl_lo: [u8; 4],
    pub ctl_hi: [u8; 4],
    pub sstat: [u8; 4],
    pub dstat: [u8; 4],
}

impl Lli {
    /// Construct the seven Linux LLI words in wire order (`regs.h:371-381`).
    pub const fn new(
        sar: u32,
        dar: u32,
        llp: u32,
        ctl_lo: u32,
        ctl_hi: u32,
        sstat: u32,
        dstat: u32,
    ) -> Self {
        Self {
            sar: sar.to_le_bytes(),
            dar: dar.to_le_bytes(),
            llp: llp.to_le_bytes(),
            ctl_lo: ctl_lo.to_le_bytes(),
            ctl_hi: ctl_hi.to_le_bytes(),
            sstat: sstat.to_le_bytes(),
            dstat: dstat.to_le_bytes(),
        }
    }

    /// Return the seven descriptor words as host-endian values.
    pub const fn words(self) -> [u32; 7] {
        [
            u32::from_le_bytes(self.sar),
            u32::from_le_bytes(self.dar),
            u32::from_le_bytes(self.llp),
            u32::from_le_bytes(self.ctl_lo),
            u32::from_le_bytes(self.ctl_hi),
            u32::from_le_bytes(self.sstat),
            u32::from_le_bytes(self.dstat),
        ]
    }

    /// Return all 28 hardware bytes in Linux's field order.
    pub const fn bytes(self) -> [u8; LLI_SIZE] {
        let fields = [
            self.sar,
            self.dar,
            self.llp,
            self.ctl_lo,
            self.ctl_hi,
            self.sstat,
            self.dstat,
        ];
        let mut out = [0; LLI_SIZE];
        let mut field = 0;
        while field < fields.len() {
            let mut byte = 0;
            while byte < 4 {
                out[field * 4 + byte] = fields[field][byte];
                byte += 1;
            }
            field += 1;
        }
        out
    }
}

/// LLP list-master-select bits (`DWC_LLP_LMS`, `regs.h:144`).
pub const fn list_master(llp: u32) -> u8 {
    (llp & 3) as u8
}

/// LLP descriptor location with the two master bits cleared (`DWC_LLP_LOC`, `regs.h:145`).
pub const fn location(llp: u32) -> u32 {
    llp & !3
}

/// Compose an LLP word exactly as `desc->txd.phys | lms` in `core.c:590` and `core.c:684`.
///
/// Refuses unaligned descriptor addresses instead of silently clearing address bits, and names
/// both Linux bounds for the two-bit list-master selector.
pub const fn encode_pointer(address: u32, master: u8) -> Result<u32, LlpError> {
    if address & 3 != 0 {
        return Err(LlpError::AddressNotFourByteAligned {
            address,
            required_alignment: 4,
        });
    }
    if master > 3 {
        return Err(LlpError::MasterOutOfRange {
            master,
            maximum_master: 3,
        });
    }
    Ok(address | master as u32)
}

/// A named refusal from LLP construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlpError {
    AddressNotFourByteAligned {
        address: u32,
        required_alignment: u32,
    },
    MasterOutOfRange {
        master: u8,
        maximum_master: u8,
    },
}
