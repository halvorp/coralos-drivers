// SPDX-License-Identifier: GPL-2.0-only
//! The extended-NVM decode. Expected values are written out from `iwl-nvm-parse.c`, `iwl-nvm-parse.h`, `iwl-drv.h`,
//! `iwl-csr.h`, `cfg/9000.c` (Linux v7.0-rc6).
use iwlwifi_nvm_parse_core::*;

#[test]
fn constants_pinned_by_value() {
    assert_eq!(MAC_ADDR_FROM_CSR, 0x380); // cfg/9000.c:43
    assert_eq!((CSR_MAC_ADDR0_OTP, CSR_MAC_ADDR1_OTP, CSR_MAC_ADDR0_STRAP, CSR_MAC_ADDR1_STRAP), (0x380, 0x384, 0x388, 0x38c)); // iwl-csr.h:655-658
    assert_eq!((NVM_VERSION_EXT_NVM, N_HW_ADDRS_FAMILY_8000, RADIO_CFG_FAMILY_EXT_NVM, SKU_FAMILY_8000), (0, 3, 0, 2)); // iwl-nvm-parse.c:52-57
    assert_eq!((NVM_CHANNELS_EXTENDED, NVM_LAR_OFFSET_OLD, NVM_LAR_OFFSET, NVM_LAR_ENABLED), (0, 0x4C7, 0x507, 0x7)); // :60-63
    assert_eq!(NVM_LAR_NEW_VERSION, 0xE39); // :1490
    assert_eq!(MAC_ADDRESS_OVERRIDE_EXT_NVM, 1); // :49
    assert_eq!(N_HW_ADDR_MASK, 0xF); // :117
    assert_eq!((NVM_SKU_CAP_BAND_24GHZ, NVM_SKU_CAP_BAND_52GHZ, NVM_SKU_CAP_11N_ENABLE, NVM_SKU_CAP_11AC_ENABLE, NVM_SKU_CAP_MIMO_DISABLE), (1, 2, 4, 8, 0x20)); // :68-72
    assert_eq!(
        (NVM_CHANNEL_VALID, NVM_CHANNEL_IBSS, NVM_CHANNEL_ALLOW_20MHZ_ACTIVITY, NVM_CHANNEL_ACTIVE, NVM_CHANNEL_RADAR, NVM_CHANNEL_INDOOR_ONLY),
        (1, 2, 4, 8, 0x10, 0x20) // iwl-nvm-parse.h:74-79
    );
    assert_eq!((NVM_CHANNEL_40MHZ, NVM_CHANNEL_80MHZ, NVM_CHANNEL_160MHZ), (0x200, 0x400, 0x800)); // :83-85
    assert_eq!(NUM_2GHZ_CHANNELS, 14); // iwl-nvm-parse.c:113
}

#[test]
fn the_channel_table_is_the_extended_one() {
    assert_eq!(EXT_NVM_CHANNELS.len(), 14 + 37); // NUM_2GHZ_CHANNELS + NUM_5GHZ_CHANNELS (:113-114)
    assert_eq!(&EXT_NVM_CHANNELS[..14], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]);
    assert_eq!(&EXT_NVM_CHANNELS[14..22], &[36, 40, 44, 48, 52, 56, 60, 64]);
    assert_eq!(&EXT_NVM_CHANNELS[22..], &[68, 72, 76, 80, 84, 88, 92, 96, 100, 104, 108, 112, 116, 120, 124, 128, 132, 136, 140, 144, 149, 153, 157, 161, 165, 169, 173, 177, 181]);
}

#[test]
fn the_extended_radio_word_splits_into_its_fields() {
    // iwl-drv.h:22-27: flavor [3:0], dash [7:4], step [11:8], type [23:12], tx_ant [27:24], rx_ant [31:28]
    let r = RadioCfg::from_word(0x3312_2100 | 0x5);
    assert_eq!((r.flavor, r.dash, r.step, r.ty, r.valid_tx_ant, r.valid_rx_ant), (5, 0, 1, 0x122, 3, 3));
    let r = RadioCfg::from_word(0x1 << 24 | 0x2 << 28);
    assert_eq!((r.valid_tx_ant, r.valid_rx_ant), (1, 2));
}

fn sections() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut sw = vec![0u8; 16];
    sw[0..4].copy_from_slice(&0x0e43u32.to_le_bytes());
    sw[6..10].copy_from_slice(&0x0000_0013u32.to_le_bytes()); // n_hw_addrs word has junk above the mask
    let mut reg = vec![0u8; 0xB00];
    reg[0x507 * 2..0x507 * 2 + 2].copy_from_slice(&0x0007u16.to_le_bytes());
    reg[0x4c7 * 2..0x4c7 * 2 + 2].copy_from_slice(&0x0000u16.to_le_bytes());
    let mut phy = vec![0u8; 16];
    phy[0..4].copy_from_slice(&0x3312_2100u32.to_le_bytes());
    phy[4..8].copy_from_slice(&0x0000_000fu32.to_le_bytes());
    (sw, reg, phy)
}

#[test]
fn parse_ext_reads_the_words_where_linux_does() {
    let (sw, reg, phy) = sections();
    let d = parse_ext(&sw, &reg, &phy).unwrap();
    assert_eq!(d.nvm_version, 0x0e43);
    assert_eq!(d.n_hw_addrs, 3, "0x13 & 0xF");
    assert_eq!(d.sku, 0xf);
    assert!(d.band_24ghz() && d.band_52ghz() && !d.mimo_disabled());
    assert_eq!((d.radio.valid_tx_ant, d.radio.valid_rx_ant), (3, 3));
    assert!(d.lar_enabled);
}

#[test]
fn the_lar_word_is_at_the_old_offset_before_nvm_version_0xe39() {
    let (mut sw, reg, phy) = sections();
    sw[0..4].copy_from_slice(&0x0e38u32.to_le_bytes());
    let d = parse_ext(&sw, &reg, &phy).unwrap();
    assert!(!d.lar_enabled, "version 0xe38 reads 0x4c7, which is 0 here"); // iwl-nvm-parse.c:1489-1494
    let mut reg2 = reg.clone();
    reg2[0x4c7 * 2] = 0x07;
    assert!(parse_ext(&sw, &reg2, &phy).unwrap().lar_enabled);
    sw[0..4].copy_from_slice(&0x0e39u32.to_le_bytes());
    assert!(parse_ext(&sw, &reg, &phy).unwrap().lar_enabled, "0xe39 is the first version at the new offset");
}

#[test]
fn lar_is_any_of_the_three_low_bits_of_its_word() {
    // data->lar_enabled = !!(lar_config & NVM_LAR_ENABLED), NVM_LAR_ENABLED = 0x7 (iwl-nvm-parse.c:63, :1493-1495)
    let (sw, mut reg, phy) = sections();
    for (word, want) in [(0x01u8, true), (0x02, true), (0x04, true), (0x08, false), (0x00, false), (0xf8, false)] {
        reg[0x507 * 2] = word;
        assert_eq!(parse_ext(&sw, &reg, &phy).unwrap().lar_enabled, want, "word {word:#x}");
    }
}

#[test]
fn short_sections_are_named_not_read_past() {
    let (sw, reg, phy) = sections();
    assert_eq!(parse_ext(&sw[..3], &reg, &phy), Err(NvmError::Short { section: "SW", need: 4, have: 3 }));
    assert_eq!(parse_ext(&sw, &reg, &phy[..6]), Err(NvmError::Short { section: "PHY_SKU", need: 8, have: 6 }));
    assert!(matches!(parse_ext(&sw, &reg[..0x507 * 2], &phy), Err(NvmError::Short { section: "REGULATORY", .. })));
}

#[test]
fn the_hardware_address_is_flipped_the_way_iwl_flip_hw_address_flips_it() {
    // dest[0..4] = bytes 3,2,1,0 of mac_addr0; dest[4..6] = bytes 1,0 of mac_addr1 (iwl-nvm-parse.c:1216-1229)
    assert_eq!(flip_hw_address(0x1222_3344, 0x0000_5566), [0x12, 0x22, 0x33, 0x44, 0x55, 0x66]);
    assert_eq!(flip_hw_address(0xaabb_ccdd, 0xffff_0102), [0xaa, 0xbb, 0xcc, 0xdd, 0x01, 0x02], "bytes 2-3 of addr1 are ignored");
}

#[test]
fn a_valid_ethernet_address_is_unicast_and_nonzero() {
    assert!(is_valid_ether_addr(&[0x02, 0, 0, 0, 0, 1]));
    assert!(!is_valid_ether_addr(&[0x01, 0, 0, 0, 0, 1]), "multicast bit");
    assert!(!is_valid_ether_addr(&[0; 6]));
    assert!(is_valid_ether_addr(&[0x00, 0x1b, 0x21, 0xaa, 0xbb, 0xcc]));
}

#[test]
fn the_strap_address_wins_when_valid_else_the_otp_one() {
    let (strap0, strap1) = (0x0a0b_0c0d, 0x0000_0e0f);
    let (otp0, otp1) = (0x1222_3344, 0x0000_5566);
    assert_eq!(mac_from_csr(strap0, strap1, otp0, otp1), Ok([0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f])); // :1237-1246
    assert_eq!(mac_from_csr(0, 0, otp0, otp1), Ok([0x12, 0x22, 0x33, 0x44, 0x55, 0x66]), "the OEM did not fuse one"); // :1248-1250
    assert_eq!(mac_from_csr(0x0100_0000, 0, otp0, otp1), Ok([0x12, 0x22, 0x33, 0x44, 0x55, 0x66]), "a multicast strap is not valid: dest[0] = byte 3 = 0x01");
    assert_eq!(mac_from_csr(0, 0, 0, 0), Err(NvmError::NoValidMac), "no valid mac address was found (:1316-1319)");
}

fn regulatory(flags: &[(usize, u16)]) -> Vec<u8> {
    let mut r = vec![0u8; 2 * 51];
    for &(i, f) in flags {
        r[2 * i..2 * i + 2].copy_from_slice(&f.to_le_bytes());
    }
    r
}

#[test]
fn channels_follow_the_nvm_order_and_drop_the_invalid_without_lar() {
    // ch 1 valid, ch 2 invalid, ch 36 (index 14) valid, ch 40 (index 15) invalid
    let reg = regulatory(&[(0, 0x109), (2, 0x109), (14, 0x111), (51 - 1, 0x1)]);
    let got: Vec<(u16, u16, bool)> = channels(&reg, true, false).map(|c| (c.number, c.flags, c.band_5ghz)).collect();
    assert_eq!(got, [(1, 0x109, false), (3, 0x109, false), (36, 0x111, true), (181, 0x1, true)]);
}

#[test]
fn with_lar_the_invalid_channels_are_kept_for_the_firmwares_regulatory_domain() {
    let reg = regulatory(&[(0, 0x109)]);
    assert_eq!(channels(&reg, true, true).count(), 51); // :388-398: dropped only `!(LAR) && !VALID`
    assert_eq!(channels(&reg, true, false).count(), 1);
}

#[test]
fn without_the_5ghz_sku_bit_the_5ghz_channels_are_dropped_before_anything_else() {
    let reg = regulatory(&[(0, 0x109), (14, 0x109)]);
    let got: Vec<u16> = channels(&reg, false, true).map(|c| c.number).collect();
    assert_eq!(got.len(), 14);
    assert!(got.iter().all(|&c| c <= 14), "{got:?}"); // :373-375
}

#[test]
fn a_short_regulatory_section_ends_the_list_at_its_last_whole_word() {
    let reg = regulatory(&[(0, 0x109), (1, 0x109)]);
    assert_eq!(channels(&reg[..4], true, true).count(), 2, "2 words = channels 1 and 2");
}
