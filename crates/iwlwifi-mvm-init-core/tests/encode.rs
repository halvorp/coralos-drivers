// SPDX-License-Identifier: GPL-2.0-only
//! Encoder vectors, hand-written Linux `__packed` little-endian layouts.
//! Ported from Linux `fw/api/config.h:22-24,84-87`, `fw/api/nvm-reg.h:108-115`, `mvm/fw.c:36-44,
//! 534-577`, `mvm/mvm.h:1789-1800`, `mvm/nvm.c:75-80`. Copyright Intel Corporation.

use iwlwifi_mvm_init_core::encode::*;

#[test]
fn tx_ant_cfg_is_one_le32() {
    let mut b = [0xEEu8; 6];
    assert_eq!(encode_tx_ant_cfg(0x3, &mut b), Ok(4));
    assert_eq!(b, [0x03, 0x00, 0x00, 0x00, 0xEE, 0xEE]); // trailing bytes untouched
    let mut b = [0u8; 4];
    assert_eq!(encode_tx_ant_cfg(0xA5, &mut b), Ok(4)); // valid is a u8 widened to le32 (fw.c:38)
    assert_eq!(b, [0xA5, 0, 0, 0]);
}

#[test]
fn tx_ant_cfg_refuses_short_buffer() {
    let mut b = [0u8; 3];
    assert_eq!(encode_tx_ant_cfg(1, &mut b), Err(EncodeError::BufferTooSmall { need: 4, have: 3 }));
}

#[test]
fn phy_cfg_v1_layout_is_phy_cfg_then_flow_then_event() {
    // config.h:84-87 + :34-37: phy_cfg, calib_control.flow_trigger, calib_control.event_trigger.
    let c = PhyCfgV1 { phy_cfg: 0x8011_0015, flow_trigger: 0x1122_3344, event_trigger: 0x5566_7788 };
    let mut b = [0u8; 12];
    assert_eq!(encode_phy_cfg_v1(&c, &mut b), Ok(12));
    assert_eq!(
        b,
        [0x15, 0x00, 0x11, 0x80, 0x44, 0x33, 0x22, 0x11, 0x88, 0x77, 0x66, 0x55]
    );
}

#[test]
fn phy_cfg_v1_refuses_short_buffer() {
    let c = PhyCfgV1 { phy_cfg: 0, flow_trigger: 0, event_trigger: 0 };
    let mut b = [0u8; 11];
    assert_eq!(encode_phy_cfg_v1(&c, &mut b), Err(EncodeError::BufferTooSmall { need: 12, have: 11 }));
}

#[test]
fn phy_config_masks_chains_like_get_phy_config() {
    // mvm/mvm.h:1791-1799: fw & (~(TX|RX) | tx << 16 | rx << 20)
    assert_eq!(phy_config(0xFFFF_FFFF, 1, 3), 0xFF31_FFFF);
    assert_eq!(phy_config(0x0033_0015, 1, 1), 0x0011_0015);
    assert_eq!(phy_config(0x0033_0015, 3, 3), 0x0033_0015);
    // Chains the firmware does not advertise stay off; non-chain bits pass through.
    assert_eq!(phy_config(0x8000_0015, 0xf, 0xf), 0x8000_0015);
    // No masking of the antenna value (mvm.h:1796-1797): 0x10 << 16 lands in the RX field.
    assert_eq!(phy_config(0xFFFF_FFFF, 0x10, 0), 0xFF10_FFFF);
    // TX and RX are placed in their own fields, not swapped.
    assert_eq!(phy_config(0xFFFF_FFFF, 0xf, 0), 0xFF0F_FFFF);
    assert_eq!(phy_config(0xFFFF_FFFF, 0, 0xf), 0xFFF0_FFFF);
}

#[test]
fn phy_cfg_new_ors_extra_flags_after_masking() {
    // mvm/fw.c:556 then :559-560 (|= extra), :562-565 triggers (event/flow not swapped).
    let c = PhyCfgV1::new(0x0033_0015, 1, 1, 0x8000_0000, 0xAAAA_0001, 0xBBBB_0002);
    assert_eq!(c.phy_cfg, 0x8011_0015);
    assert_eq!(c.flow_trigger, 0xAAAA_0001);
    assert_eq!(c.event_trigger, 0xBBBB_0002);
    // extra flags are NOT masked by the firmware value:
    let c = PhyCfgV1::new(0, 3, 3, 0x0000_0100, 0, 0);
    assert_eq!(c.phy_cfg, 0x0000_0100);
}

#[test]
fn nvm_read_layout() {
    // nvm-reg.h:108-115: op_code, target, le16 type, le16 offset, le16 length. nvm.c:75-80.
    let mut b = [0xEEu8; 9];
    assert_eq!(encode_nvm_read(12, 0x0800, 0x0800, &mut b), Ok(8));
    assert_eq!(b, [0x00, 0x00, 0x0c, 0x00, 0x00, 0x08, 0x00, 0x08, 0xEE]);
    let mut b = [0u8; 8];
    assert_eq!(encode_nvm_read(0x0102, 0x0304, 0x0506, &mut b), Ok(8));
    assert_eq!(b, [0x00, 0x00, 0x02, 0x01, 0x04, 0x03, 0x06, 0x05]);
}

#[test]
fn nvm_read_refuses_short_buffer() {
    let mut b = [0u8; 7];
    assert_eq!(encode_nvm_read(0, 0, 2048, &mut b), Err(EncodeError::BufferTooSmall { need: 8, have: 7 }));
}
