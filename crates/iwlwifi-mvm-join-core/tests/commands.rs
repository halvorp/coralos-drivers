// SPDX-License-Identifier: GPL-2.0-only
//! Expected bytes are written out from the Linux structs (fw/api/{phy-ctxt,binding,time-event,sta,mac,filter}.h)
//! as literals, never derived from this crate's tables.

use iwlwifi_mvm_join_core::ctxt::*;
use iwlwifi_mvm_join_core::ids::*;
use iwlwifi_mvm_join_core::EncodeError;

fn le32(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}

fn put(v: &mut [u8], off: usize, bytes: &[u8]) {
    v[off..off + bytes.len()].copy_from_slice(bytes);
}

#[test]
fn constants_are_the_linux_literals() {
    assert_eq!(REMOVE_STA, 0x19); // fw/api/commands.h:145
    assert_eq!(TX_CMD, 0x1c); // fw/api/commands.h:153
    assert_eq!(TIME_EVENT_CMD, 0x29);
    assert_eq!(TIME_EVENT_NOTIFICATION, 0x2a);
    assert_eq!(BINDING_CONTEXT_CMD, 0x2b);
    assert_eq!(MCAST_FILTER_CMD, 0xd0);
    assert_eq!((FW_CTXT_ID_POS, FW_CTXT_COLOR_POS), (0, 8));
    assert_eq!(FW_CTXT_INVALID, 0xffff_ffff);
    assert_eq!((FW_CTXT_ACTION_ADD, FW_CTXT_ACTION_MODIFY, FW_CTXT_ACTION_REMOVE), (1, 2, 3));
    assert_eq!((PHY_BAND_5, PHY_BAND_24, IWL_PHY_CHANNEL_MODE20), (0, 1, 0));
    assert_eq!((PHY_RX_CHAIN_VALID_POS, PHY_RX_CHAIN_CNT_POS, PHY_RX_CHAIN_MIMO_CNT_POS), (1, 10, 12));
    assert_eq!(MAX_MACS_IN_BINDING, 3);
    assert_eq!(TE_BSS_STA_AGGRESSIVE_ASSOC, 0);
    assert_eq!(TE_V2_FRAG_NONE, 0);
    assert_eq!((TE_V2_NOTIF_HOST_EVENT_START, TE_V2_NOTIF_HOST_EVENT_END, TE_V2_START_IMMEDIATELY), (1, 2, 0x800));
    assert_eq!((STA_MODE_ADD, STA_MODE_MODIFY), (0, 1));
    assert_eq!((STA_MODIFY_TID_DISABLE_TX, STA_MODIFY_QUEUES), (2, 0x80));
    assert_eq!(IWL_STA_LINK, 0);
    assert_eq!((STA_FLG_FAT_EN_MSK, STA_FLG_MIMO_EN_MSK, STA_FLG_RTS_MIMO_PROT), (0x0c00_0000, 0x3000_0000, 0x0002_0000));
    assert_eq!(AP_STA_ID, 0);
    assert_eq!((MAC_PROT_FLG_TGG_PROTECT, MAC_FLG_SHORT_SLOT, MAC_FLG_SHORT_PREAMBLE), (8, 0x10, 0x20));
    assert_eq!(LISTEN_INTERVAL, 10);
    assert_eq!(MAC_CTX_CMD_LEN, 148);
    assert_eq!(ADD_STA_CMD_LEN, 48);
    assert_eq!(TIME_EVENT_CMD_LEN, 4 * 3 + 4 * 5 + 1 + 1 + 2);
    assert_eq!(TIME_EVENT_RESP_LEN, 16);
    assert_eq!(BINDING_CMD_V1_LEN, 4 * (2 + 3 + 1));
    assert_eq!(PHY_CTX_CMD_V1_UHB_LEN, 4 * 4 + 8 + 16);
    assert_eq!(PHY_CTX_CMD_V1_LEN, PHY_CTX_CMD_V1_UHB_LEN - 4);
}

#[test]
fn id_and_color() {
    assert_eq!(iwlwifi_mvm_join_core::ctxt::id_and_color(2, 3), 0x302);
    assert_eq!(iwlwifi_mvm_join_core::ctxt::id_and_color(0, 0), 0);
    assert_eq!(iwlwifi_mvm_join_core::ctxt::id_and_color(0xff, 0xff), 0xffff);
}

fn phy(ch: u8, five: bool) -> PhyCtxt {
    PhyCtxt { id: 0, color: 0, channel: ch, band_5ghz: five, valid_tx_ant: 3, valid_rx_ant: 3, chains_static: 1, chains_dynamic: 1 }
}

#[test]
fn phy_context_with_the_8_byte_channel_info() {
    let mut b = [0xeeu8; 64];
    let n = phy_context(1, &phy(36, true), true, &mut b).unwrap();
    assert_eq!(n, 40);
    let mut w = [0u8; 40];
    put(&mut w, 0, &le32(0)); // id_and_color
    put(&mut w, 4, &le32(1)); // action ADD
    put(&mut w, 16, &le32(36)); // ci.channel
    w[20] = 0; // band 5 GHz
    w[21] = 0; // width 20 MHz
    put(&mut w, 24, &le32(3)); // txchain_info
    put(&mut w, 28, &le32((3 << 1) | (1 << 10) | (1 << 12))); // rxchain_info = 0x1406
    assert_eq!(&b[..40], &w[..]);
    assert_eq!(u32::from_le_bytes(b[28..32].try_into().unwrap()), 0x1406);
    // 2.4 GHz channel 6, MODIFY.
    phy_context(2, &phy(6, false), true, &mut b).unwrap();
    assert_eq!(b[20], 1);
    assert_eq!(b[16], 6);
    assert_eq!(b[4], 2);
}

#[test]
fn phy_context_with_the_4_byte_channel_info() {
    let mut b = [0u8; 64];
    let n = phy_context(1, &phy(11, false), false, &mut b).unwrap();
    assert_eq!(n, 36);
    assert_eq!(&b[16..20], &[1, 11, 0, 0]); // band 2.4 GHz, channel, width, ctrl_pos
    assert_eq!(u32::from_le_bytes(b[20..24].try_into().unwrap()), 3); // txchain
    assert_eq!(u32::from_le_bytes(b[24..28].try_into().unwrap()), 0x1406); // rxchain
    assert_eq!(&b[28..36], &[0u8; 8]);
}

#[test]
fn phy_context_buffer_and_ids() {
    let mut small = [0u8; 39];
    assert_eq!(phy_context(1, &phy(1, false), true, &mut small), Err(EncodeError::BufferTooSmall { need: 40, have: 39 }));
    let mut b = [0u8; 40];
    let c = PhyCtxt { id: 2, color: 3, ..phy(36, true) };
    phy_context(3, &c, true, &mut b).unwrap();
    assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 0x302);
}

#[test]
fn binding_v1_bytes() {
    let mut b = [0u8; 32];
    assert_eq!(binding_v1(1, 2, 3, 0, 0, &mut b), Ok(24));
    let mut w = [0u8; 24];
    put(&mut w, 0, &le32(0x302)); // id_and_color of the PHY
    put(&mut w, 4, &le32(1));
    put(&mut w, 8, &le32(0)); // macs[0] = FW_CMD_ID_AND_COLOR(0, 0)
    put(&mut w, 12, &le32(0xffff_ffff));
    put(&mut w, 16, &le32(0xffff_ffff));
    put(&mut w, 20, &le32(0x302)); // phy
    assert_eq!(&b[..24], &w[..]);
    binding_v1(3, 0, 0, 1, 2, &mut b).unwrap();
    assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()), 0x201);
}

#[test]
fn time_event_protect_bytes() {
    let mut b = [0u8; 40];
    assert_eq!(time_event_protect(0, 0, 200, 50, &mut b), Ok(36));
    let mut w = [0u8; 36];
    put(&mut w, 4, &le32(1)); // action ADD
    put(&mut w, 8, &le32(0)); // id TE_BSS_STA_AGGRESSIVE_ASSOC
    put(&mut w, 16, &le32(50)); // max_delay
    put(&mut w, 24, &le32(1)); // interval
    put(&mut w, 28, &le32(200)); // duration
    w[32] = 1; // repeat
    w[33] = 0; // max_frags NONE
    put(&mut w, 34, &[0x03, 0x08]); // policy = START | END | START_IMMEDIATELY = 0x0803
    assert_eq!(&b[..36], &w[..]);
}

#[test]
fn time_event_remove_bytes_and_response() {
    let mut b = [0u8; 36];
    time_event_remove(1, 2, 0x1234, &mut b).unwrap();
    assert_eq!(u32::from_le_bytes(b[0..4].try_into().unwrap()), 0x201);
    assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()), 3);
    assert_eq!(u32::from_le_bytes(b[8..12].try_into().unwrap()), 0x1234);

    let mut r = [0u8; 16];
    put(&mut r, 0, &le32(1));
    put(&mut r, 8, &le32(77));
    assert_eq!(time_event_resp(&r), Ok(77));
    put(&mut r, 0, &le32(0));
    assert_eq!(time_event_resp(&r), Err(TimeEventRefusal::NotScheduled(0)));
    put(&mut r, 0, &le32(2)); // bit 0 clear
    assert_eq!(time_event_resp(&r), Err(TimeEventRefusal::NotScheduled(2)));
    assert_eq!(time_event_resp(&r[..12]), Err(TimeEventRefusal::Length(12)));
}

const NODE: [u8; 6] = [0x12, 0x22, 0x33, 0x44, 0x55, 0x66];
const BSSID: [u8; 6] = [0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x01];

fn mac(basic: u16) -> MacCtxt {
    MacCtxt {
        id: 0,
        color: 0,
        node_addr: NODE,
        bssid: BSSID,
        basic_rates: basic,
        short_preamble: false,
        short_slot: false,
        use_cts_prot: false,
        beacon_int: 100,
        dtim_period: 3,
        assoc: None,
    }
}

/// The hand-assembled MAC context for `mac(0x0150)` (6M, 12M, 24M basic), ADD or MODIFY.
fn mac_golden(action: u32) -> [u8; 148] {
    let mut w = [0u8; 148];
    put(&mut w, 4, &le32(action));
    put(&mut w, 8, &le32(5)); // FW_MAC_TYPE_BSS_STA
    put(&mut w, 12, &le32(0)); // TSF A
    put(&mut w, 16, &NODE);
    put(&mut w, 24, &BSSID);
    put(&mut w, 32, &le32(0xf)); // cck ack rates: all four (no CCK rate is basic)
    put(&mut w, 36, &le32(0x15)); // ofdm: 6M | 12M | 24M
    put(&mut w, 52, &le32(0x44)); // ACCEPT_GRP | IN_BEACON
    // ac[AC_VO]: cw 3/7 aifsn 2 fifos BIT(3) txop 1504us; AC_VI; AC_BE; AC_BK; the CAB entry is zero
    put(&mut w, 60, &[3, 0, 7, 0, 2, 8, 0xe0, 0x05]);
    put(&mut w, 68, &[7, 0, 15, 0, 2, 4, 0xc0, 0x0b]);
    put(&mut w, 76, &[15, 0, 0xff, 0x03, 3, 2, 0, 0]);
    put(&mut w, 84, &[15, 0, 0xff, 0x03, 7, 1, 0, 0]);
    put(&mut w, 116, &le32(100)); // bi
    put(&mut w, 124, &le32(300)); // dtim_interval = bi * dtim_period
    put(&mut w, 132, &le32(10)); // listen_interval
    w
}

#[test]
fn mac_context_before_association() {
    let mut b = [0u8; 160];
    assert_eq!(mac_context_sta(2, &mac(0x0150), &mut b), Ok(148));
    assert_eq!(&b[..148], &mac_golden(2)[..]);
    mac_context_sta(1, &mac(0x0150), &mut b).unwrap();
    assert_eq!(&b[..148], &mac_golden(1)[..]);
}

#[test]
fn mac_context_after_association() {
    let mut c = mac(0x0150);
    c.assoc = Some(MacAssoc { aid: 5, dtim_count: 2, sync_tsf: 1_000_000, sync_device_ts: 5000 });
    c.short_slot = true;
    c.short_preamble = true;
    let mut b = [0u8; 148];
    mac_context_sta(2, &c, &mut b).unwrap();
    let mut w = mac_golden(2);
    put(&mut w, 44, &le32(0x20)); // cck_short_preamble
    put(&mut w, 48, &le32(0x10)); // short_slot
    put(&mut w, 52, &le32(0x04)); // IN_BEACON dropped once the DTIM period is known
    put(&mut w, 100, &le32(1)); // is_assoc
    put(&mut w, 104, &le32(5000 + 204_800)); // dtim_time: 2 intervals * 100 TU * 1024 us after the beacon
    put(&mut w, 108, &(1_000_000u64 + 204_800).to_le_bytes()); // dtim_tsf
    put(&mut w, 136, &le32(5)); // assoc_id
    put(&mut w, 140, &le32(5000)); // assoc_beacon_arrive_time
    assert_eq!(&b[..], &w[..]);
}

#[test]
fn mac_context_without_a_dtim_period_is_not_associated() {
    // vif->bss_conf.dtim_period == 0: is_assoc 0, beacons still pass, the AID is still sent.
    let mut c = mac(0x0150);
    c.dtim_period = 0;
    c.assoc = Some(MacAssoc { aid: 9, dtim_count: 0, sync_tsf: 1, sync_device_ts: 1 });
    let mut b = [0u8; 148];
    mac_context_sta(2, &c, &mut b).unwrap();
    assert_eq!(u32::from_le_bytes(b[100..104].try_into().unwrap()), 0);
    assert_eq!(u32::from_le_bytes(b[52..56].try_into().unwrap()), 0x44);
    assert_eq!(u32::from_le_bytes(b[136..140].try_into().unwrap()), 9);
    assert_eq!(u32::from_le_bytes(b[124..128].try_into().unwrap()), 0);
}

#[test]
fn mac_context_cts_protection_and_rates() {
    let mut c = mac(0x0001); // 1M basic
    c.use_cts_prot = true;
    let mut b = [0u8; 148];
    mac_context_sta(1, &c, &mut b).unwrap();
    assert_eq!(u32::from_le_bytes(b[40..44].try_into().unwrap()), 8);
    assert_eq!(u32::from_le_bytes(b[32..36].try_into().unwrap()), 0x1); // only 1M may ack
    assert_eq!(u32::from_le_bytes(b[36..40].try_into().unwrap()), 0x15);
}

#[test]
fn mac_context_agrees_with_the_rt_core_unassociated_add() {
    // The RT bring-up already sends an ADD with no basic rates; the join MODIFY must be the same
    // command wherever the fields mean the same thing.
    use iwlwifi_mvm_rt_core::encode::encode_mac_ctxt_sta_add;
    let mut a = [0u8; 148];
    encode_mac_ctxt_sta_add(0, NODE, &mut a).unwrap();
    let mut c = mac(0);
    c.bssid = [0; 6];
    c.beacon_int = 0;
    c.dtim_period = 0;
    let mut b = [0u8; 148];
    mac_context_sta(1, &c, &mut b).unwrap();
    for (name, range) in [("id", 0..8), ("type/tsf", 8..16), ("node", 16..22), ("bssid", 24..30), ("rates", 32..40), ("prot/preamble/slot", 40..52), ("filter", 52..56), ("listen", 132..140)] {
        assert_eq!(a[range.clone()], b[range], "{name}");
    }
}

#[test]
fn add_sta_for_the_ap() {
    let mut b = [0u8; 48];
    assert_eq!(add_sta_ap(false, 0, 0, 0, &BSSID, 1 << 5, 0, &mut b), Ok(48));
    let mut w = [0u8; 48];
    w[0] = 0; // STA_MODE_ADD
    put(&mut w, 2, &[0xff, 0xff]); // tid_disable_tx
    put(&mut w, 8, &BSSID);
    w[16] = 0; // sta_id
    put(&mut w, 24, &le32(0x3c02_0000)); // station_flags_msk = FAT_EN | MIMO_EN | RTS_MIMO_PROT
    w[35] = 0; // IWL_STA_LINK
    put(&mut w, 40, &le32(0x20)); // tfd_queue_msk
    assert_eq!(&b[..], &w[..]);

    add_sta_ap(true, 0, 0, 0, &BSSID, 1 << 5, 5, &mut b).unwrap();
    assert_eq!(b[0], 1); // STA_MODE_MODIFY
    assert_eq!(u16::from_le_bytes([b[36], b[37]]), 5); // assoc_id
    assert_eq!(b[17], 0); // modify_mask: the queue list is unchanged
}

#[test]
fn add_sta_agrees_with_the_rt_core_aux_layout() {
    // Same struct, two writers: fields at the offsets the aux encoder pins must be where this one puts them.
    use iwlwifi_mvm_rt_core::encode::encode_add_sta_aux;
    let mut a = [0u8; 48];
    encode_add_sta_aux(1, 1, &mut a).unwrap();
    let mut b = [0u8; 48];
    add_sta_ap(false, 1, 4, 0, &[0; 6], 1 << 1, 0, &mut b).unwrap();
    assert_eq!(a[2..4], b[2..4]); // tid_disable_tx
    assert_eq!(a[4..8], b[4..8]); // mac_id_n_color
    assert_eq!(a[16], b[16]); // sta_id
    assert_eq!(a[40..44], b[40..44]); // tfd_queue_msk
}

#[test]
fn remove_sta_and_mcast_filter() {
    let mut b = [0u8; 16];
    assert_eq!(remove_sta(0, &mut b), Ok(4));
    assert_eq!(&b[..4], &[0, 0, 0, 0]);
    remove_sta(7, &mut b).unwrap();
    assert_eq!(b[0], 7);
    assert_eq!(mcast_filter_pass_all(&BSSID, &mut b), Ok(12));
    assert_eq!(&b[..12], &[1, 0, 0, 1, 0x02, 0xaa, 0xbb, 0xcc, 0xdd, 0x01, 0, 0]);
}

#[test]
fn every_encoder_refuses_a_short_buffer_by_name() {
    let mut s = [0u8; 3];
    assert!(matches!(binding_v1(1, 0, 0, 0, 0, &mut s), Err(EncodeError::BufferTooSmall { need: 24, have: 3 })));
    assert!(matches!(time_event_protect(0, 0, 1, 1, &mut s), Err(EncodeError::BufferTooSmall { need: 36, have: 3 })));
    assert!(matches!(time_event_remove(0, 0, 1, &mut s), Err(EncodeError::BufferTooSmall { need: 36, have: 3 })));
    assert!(matches!(mac_context_sta(1, &mac(0), &mut s), Err(EncodeError::BufferTooSmall { need: 148, have: 3 })));
    assert!(matches!(add_sta_ap(false, 0, 0, 0, &BSSID, 0, 0, &mut s), Err(EncodeError::BufferTooSmall { need: 48, have: 3 })));
    assert!(matches!(remove_sta(0, &mut s[..3]), Err(EncodeError::BufferTooSmall { need: 4, have: 3 })));
    assert!(matches!(mcast_filter_pass_all(&BSSID, &mut s), Err(EncodeError::BufferTooSmall { need: 12, have: 3 })));
}

#[test]
fn scd_queue_disable_bytes() {
    let mut b = [0xeeu8; 16];
    assert_eq!(scd_queue_disable(5, 0, 8, &mut b), Ok(12));
    assert_eq!(&b[..12], &[0, 0, 8, 5, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(matches!(scd_queue_disable(5, 0, 8, &mut b[..11]), Err(EncodeError::BufferTooSmall { need: 12, have: 11 })));
}
