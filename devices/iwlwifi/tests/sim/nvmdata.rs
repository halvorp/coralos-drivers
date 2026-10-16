// SPDX-License-Identifier: GPL-2.0-only
//! The OTP of the simulated card: the extended-NVM sections a 9260 holds, laid out per
//! `iwl-nvm-parse.c:25-62` (word offsets) and `iwl-nvm-parse.h:73-88` (channel flags). The words are written as
//! LITERALS here, not through `iwlwifi-nvm-parse-core`'s constants.

/// The NVM section numbers, `fw/api/nvm-reg.h:89-96`, and the 9000 family's HW section (cfg/9000.c: `nvm_hw_section_num` 10).
pub const SW: usize = 1;
pub const REGULATORY: usize = 3;
pub const CALIBRATION: usize = 4;
pub const PRODUCTION: usize = 5;
pub const HW: usize = 10;
pub const PHY_SKU: usize = 12;

#[derive(Clone)]
pub struct NvmSpec {
    pub version: u32,
    pub tx_ant: u8,
    pub rx_ant: u8,
    pub sku: u32,
    pub lar: bool,
    /// Flags per channel in `iwl_ext_nvm_channels` order (51 entries).
    pub channel_flags: Vec<u16>,
}

impl NvmSpec {
    /// A Pixelbook-Go-shaped part: 2x2, 2.4 + 5 GHz, 11n + 11ac, LAR on, channels 1-11 and 36-165 usable.
    pub fn atlas() -> NvmSpec {
        let mut f = vec![0u16; 51];
        for c in f.iter_mut().take(11) {
            *c = 0x0109; // VALID | ACTIVE | 20MHZ
        }
        // 5 GHz: index 14 = ch 36 ... ; U-NII-1 active, U-NII-2/2e radar, U-NII-3 active.
        let radar = 0x0111; // VALID | RADAR | 20MHZ
        let active = 0x0109;
        for (i, c) in f.iter_mut().enumerate().skip(14) {
            let ch = [36u16, 40, 44, 48, 52, 56, 60, 64, 68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144, 149, 153, 157, 161, 165, 169, 173, 177, 181][i - 14];
            *c = match ch {
                36..=48 | 149..=165 => active,
                52..=64 | 100..=144 => radar,
                _ => 0,
            };
        }
        NvmSpec { version: 0x0e43, tx_ant: 3, rx_ant: 3, sku: 0xf, lar: true, channel_flags: f }
    }

    /// The sections as the card returns them, indexed by section type; `None` answers "not valid".
    pub fn sections(&self) -> Vec<Option<Vec<u8>>> {
        let mut v: Vec<Option<Vec<u8>>> = vec![None; 13];
        // SW (1): version le32 at word 0; n_hw_addrs le32 at word 3 (byte 6).
        let mut sw = vec![0u8; 0x200];
        sw[0..4].copy_from_slice(&self.version.to_le_bytes());
        sw[6..10].copy_from_slice(&1u32.to_le_bytes());
        v[SW] = Some(sw);
        // REGULATORY (3): channel flags from word 0; the LAR word at 0x507 (new offset, version >= 0xe39). Exactly two
        // 2 KiB reads long, so the walk meets the NOT_VALID_ADDRESS that ends a section on a chunk boundary.
        let mut reg = vec![0u8; 4096];
        for (i, f) in self.channel_flags.iter().enumerate() {
            reg[2 * i..2 * i + 2].copy_from_slice(&f.to_le_bytes());
        }
        let lar_word = if self.version < 0xe39 { 0x4c7 } else { 0x507 };
        let lar: u16 = if self.lar { 0x7 } else { 0 };
        reg[2 * lar_word..2 * lar_word + 2].copy_from_slice(&lar.to_le_bytes());
        v[REGULATORY] = Some(reg);
        v[CALIBRATION] = Some((0..0x120u32).map(|i| (i * 7) as u8).collect());
        v[PRODUCTION] = Some(vec![0x5a; 0x40]);
        // HW (10): the section the family names; contents unused by the ext-NVM path (the address comes from the CSR straps).
        v[HW] = Some(vec![0xa5; 0x60]);
        // PHY_SKU (12): radio cfg le32 at word 0 (flavor|dash<<4|step<<8|type<<12|tx_ant<<24|rx_ant<<28); sku le32 at word 2.
        let radio: u32 = 0x0 | (1 << 4) | (2 << 8) | (0x112 << 12) | ((self.tx_ant as u32) << 24) | ((self.rx_ant as u32) << 28);
        let mut phy = vec![0u8; 0x20];
        phy[0..4].copy_from_slice(&radio.to_le_bytes());
        phy[4..8].copy_from_slice(&self.sku.to_le_bytes());
        v[PHY_SKU] = Some(phy);
        v
    }
}
