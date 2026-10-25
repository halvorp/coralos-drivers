// SPDX-License-Identifier: GPL-2.0-only
//! IEEE Std 802.11-2020 frames of the join: authentication (open system), association request and
//! response, deauthentication, To-DS data with LLC/SNAP, the CCMP header, and the parser for
//! received management and From-DS data frames. Layouts: 802.11-2020 9.2.4 (frame control),
//! 9.3.3 (management), 9.3.2 (data), 9.4.1 (fixed fields), 12.5.3.2 (CCMP header), RFC 1042 (SNAP).
//! Sequencing follows OpenBSD `ieee80211_send_mgmt` (net80211/ieee80211_output.c).

use crate::{check, EncodeError};

pub const HDR_LEN: usize = 24;
pub const QOS_HDR_LEN: usize = 26;
pub const CCMP_HDR_LEN: usize = 8;
pub const LLC_SNAP_LEN: usize = 8;

// frame control byte 0: version(2) type(2) subtype(4)
pub const FC0_TYPE_MASK: u8 = 0x0c;
pub const FC0_TYPE_MGMT: u8 = 0x00;
pub const FC0_TYPE_DATA: u8 = 0x08;
pub const FC0_SUBTYPE_MASK: u8 = 0xf0;
pub const SUBTYPE_ASSOC_REQ: u8 = 0x00;
pub const SUBTYPE_ASSOC_RESP: u8 = 0x10;
pub const SUBTYPE_PROBE_RESP: u8 = 0x50;
pub const SUBTYPE_BEACON: u8 = 0x80;
pub const SUBTYPE_DISASSOC: u8 = 0xa0;
pub const SUBTYPE_AUTH: u8 = 0xb0;
pub const SUBTYPE_DEAUTH: u8 = 0xc0;
// frame control byte 1
pub const FC1_TO_DS: u8 = 0x01;
pub const FC1_FROM_DS: u8 = 0x02;
pub const FC1_DS_MASK: u8 = 0x03;
pub const FC1_PROTECTED: u8 = 0x40;
/// A QoS data subtype has bit 7 of byte 0 set (the QoS bit of the subtype).
pub const FC0_QOS: u8 = 0x80;
/// A null data subtype (no payload) has bit 6 of byte 0 set.
pub const FC0_NULL: u8 = 0x40;

pub const AUTH_ALGO_OPEN: u16 = 0;
pub const CAP_ESS: u16 = 1 << 0;
pub const CAP_PRIVACY: u16 = 1 << 4;
pub const CAP_SHORT_PREAMBLE: u16 = 1 << 5;
pub const CAP_SHORT_SLOT: u16 = 1 << 10;
pub const STATUS_SUCCESS: u16 = 0;

pub const EID_SSID: u8 = 0;
pub const EID_SUPP_RATES: u8 = 1;
pub const EID_DS_PARAMS: u8 = 3;
pub const EID_TIM: u8 = 5;
pub const EID_ERP: u8 = 42;
pub const EID_HT_CAPS: u8 = 45;
pub const EID_RSN: u8 = 48;
pub const EID_EXT_RATES: u8 = 50;
pub const EID_HT_OPERATION: u8 = 61;

pub const ETHERTYPE_EAPOL: u16 = 0x888e;

pub type Mac = [u8; 6];

fn put_hdr(out: &mut [u8], fc0: u8, fc1: u8, a1: &Mac, a2: &Mac, a3: &Mac) {
    out[0] = fc0;
    out[1] = fc1;
    // duration @2 and sequence control @22 stay zero: the firmware assigns the sequence number
    out[4..10].copy_from_slice(a1);
    out[10..16].copy_from_slice(a2);
    out[16..22].copy_from_slice(a3);
}

/// Open-system authentication request: `algorithm 0, transaction 1, status 0`. // 802.11-2020 9.3.3.12
pub fn auth_request(out: &mut [u8], bssid: &Mac, sa: &Mac) -> Result<usize, EncodeError> {
    check(out, HDR_LEN + 6)?;
    put_hdr(out, SUBTYPE_AUTH, 0, bssid, sa, bssid);
    out[24..26].copy_from_slice(&AUTH_ALGO_OPEN.to_le_bytes());
    out[26..28].copy_from_slice(&1u16.to_le_bytes());
    out[28..30].copy_from_slice(&STATUS_SUCCESS.to_le_bytes());
    Ok(HDR_LEN + 6)
}

/// Deauthentication with a reason code. // 802.11-2020 9.3.3.13
pub fn deauth(out: &mut [u8], bssid: &Mac, sa: &Mac, reason: u16) -> Result<usize, EncodeError> {
    check(out, HDR_LEN + 2)?;
    put_hdr(out, SUBTYPE_DEAUTH, 0, bssid, sa, bssid);
    out[24..26].copy_from_slice(&reason.to_le_bytes());
    Ok(HDR_LEN + 2)
}

/// What an association request carries. `rates` are the octets of the Supported Rates and
/// Extended Supported Rates elements to offer, basic-rate flag cleared.
pub struct AssocReq<'a> {
    pub sa: Mac,
    pub bssid: Mac,
    pub capability: u16,
    pub listen_interval: u16,
    pub ssid: &'a [u8],
    pub rates: &'a [u8],
    /// The whole RSNE (element id, length, body) or `None` for an open network.
    pub rsne: Option<&'a [u8]>,
}

/// Association request. // 802.11-2020 9.3.3.6
pub fn assoc_request(out: &mut [u8], a: &AssocReq<'_>) -> Result<usize, EncodeError> {
    let first = a.rates.len().min(8);
    let ext = a.rates.len() - first;
    let need = HDR_LEN + 4 + 2 + a.ssid.len() + 2 + first + if ext > 0 { 2 + ext } else { 0 } + a.rsne.map_or(0, |r| r.len());
    if a.ssid.len() > 32 {
        return Err(EncodeError::OutOfRange { what: "ssid", value: a.ssid.len() as u32, max: 32 });
    }
    check(out, need)?;
    put_hdr(out, SUBTYPE_ASSOC_REQ, 0, &a.bssid, &a.sa, &a.bssid);
    out[24..26].copy_from_slice(&a.capability.to_le_bytes());
    out[26..28].copy_from_slice(&a.listen_interval.to_le_bytes());
    let mut o = 28;
    out[o] = EID_SSID;
    out[o + 1] = a.ssid.len() as u8;
    out[o + 2..o + 2 + a.ssid.len()].copy_from_slice(a.ssid);
    o += 2 + a.ssid.len();
    out[o] = EID_SUPP_RATES;
    out[o + 1] = first as u8;
    out[o + 2..o + 2 + first].copy_from_slice(&a.rates[..first]);
    o += 2 + first;
    if ext > 0 {
        out[o] = EID_EXT_RATES;
        out[o + 1] = ext as u8;
        out[o + 2..o + 2 + ext].copy_from_slice(&a.rates[first..]);
        o += 2 + ext;
    }
    if let Some(r) = a.rsne {
        out[o..o + r.len()].copy_from_slice(r);
        o += r.len();
    }
    debug_assert_eq!(o, need);
    Ok(o)
}

/// To-DS data frame header (24 bytes): `addr1 = BSSID, addr2 = SA, addr3 = DA`. // 802.11-2020 9.3.2.1
pub fn data_header_to_ds(out: &mut [u8], bssid: &Mac, sa: &Mac, da: &Mac, protected: bool) -> Result<usize, EncodeError> {
    check(out, HDR_LEN)?;
    put_hdr(out, FC0_TYPE_DATA, FC1_TO_DS | if protected { FC1_PROTECTED } else { 0 }, bssid, sa, da);
    Ok(HDR_LEN)
}

/// RFC 1042 encapsulation: `AA AA 03 00 00 00 ethertype`.
pub fn llc_snap(out: &mut [u8], ethertype: u16) -> Result<usize, EncodeError> {
    check(out, LLC_SNAP_LEN)?;
    out[..6].copy_from_slice(&[0xaa, 0xaa, 0x03, 0x00, 0x00, 0x00]);
    out[6..8].copy_from_slice(&ethertype.to_be_bytes());
    Ok(LLC_SNAP_LEN)
}

/// The CCMP header inserted after the 802.11 header: `PN0 PN1 rsvd (ExtIV | key id << 6) PN2..PN5`.
/// // iwl_mvm_set_tx_cmd_pn, mvm/tx.c:403-413; 802.11-2020 12.5.3.2
pub fn ccmp_header(out: &mut [u8], pn: u64, key_id: u8) -> Result<usize, EncodeError> {
    check(out, CCMP_HDR_LEN)?;
    out[0] = pn as u8;
    out[1] = (pn >> 8) as u8;
    out[2] = 0;
    out[3] = 0x20 | (key_id << 6);
    out[4] = (pn >> 16) as u8;
    out[5] = (pn >> 24) as u8;
    out[6] = (pn >> 32) as u8;
    out[7] = (pn >> 40) as u8;
    Ok(CCMP_HDR_LEN)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RxRefusal {
    /// Shorter than the header it claims; carries `(have, need)`.
    Short { have: usize, need: usize },
    /// Not a management frame of the subtype the caller asked for; carries frame control byte 0.
    WrongType(u8),
    /// A data frame that is not From-DS; carries frame control byte 1.
    NotFromDs(u8),
    /// A null or empty data frame: nothing to deliver.
    NoPayload,
    /// A protected frame whose CCMP header lacks the ExtIV bit (not CCMP); carries byte 3.
    NotCcmp(u8),
    /// The LLC/SNAP header is not `AA AA 03 00 00 00`.
    NotSnap,
}

/// A received management frame, header split from body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mgmt<'a> {
    pub subtype: u8,
    pub da: Mac,
    pub sa: Mac,
    pub bssid: Mac,
    pub body: &'a [u8],
}

/// Split a management frame. `mpdu` begins at the 802.11 header; `pad` is true when the device
/// inserted two pad bytes after the header (`IWL_RX_MPDU_MFLG2_PAD`).
pub fn parse_mgmt(mpdu: &[u8], pad: bool) -> Result<Mgmt<'_>, RxRefusal> {
    let body_off = HDR_LEN + if pad { 2 } else { 0 };
    if mpdu.len() < body_off {
        return Err(RxRefusal::Short { have: mpdu.len(), need: body_off });
    }
    if mpdu[0] & FC0_TYPE_MASK != FC0_TYPE_MGMT {
        return Err(RxRefusal::WrongType(mpdu[0]));
    }
    let mut m = Mgmt { subtype: mpdu[0] & FC0_SUBTYPE_MASK, da: [0; 6], sa: [0; 6], bssid: [0; 6], body: &mpdu[body_off..] };
    m.da.copy_from_slice(&mpdu[4..10]);
    m.sa.copy_from_slice(&mpdu[10..16]);
    m.bssid.copy_from_slice(&mpdu[16..22]);
    Ok(m)
}

/// `{ algorithm, transaction, status }` of an authentication frame body.
pub fn parse_auth(body: &[u8]) -> Result<(u16, u16, u16), RxRefusal> {
    if body.len() < 6 {
        return Err(RxRefusal::Short { have: body.len(), need: 6 });
    }
    let w = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
    Ok((w(0), w(2), w(4)))
}

/// `{ capability, status, aid }` of an association response body; the two top bits of the AID
/// field are set by the AP and masked off. // 802.11-2020 9.3.3.7
pub fn parse_assoc_resp(body: &[u8]) -> Result<(u16, u16, u16), RxRefusal> {
    if body.len() < 6 {
        return Err(RxRefusal::Short { have: body.len(), need: 6 });
    }
    let w = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
    Ok((w(0), w(2), w(4) & 0x3fff))
}

/// The reason code of a deauthentication or disassociation body.
pub fn parse_reason(body: &[u8]) -> Result<u16, RxRefusal> {
    if body.len() < 2 {
        return Err(RxRefusal::Short { have: body.len(), need: 2 });
    }
    Ok(u16::from_le_bytes([body[0], body[1]]))
}

/// A From-DS data frame reduced to an Ethernet-like view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DataIn<'a> {
    pub da: Mac,
    pub sa: Mac,
    pub bssid: Mac,
    pub ethertype: u16,
    pub payload: &'a [u8],
    pub protected: bool,
    /// The CCMP packet number and key id of a protected frame.
    pub pn: Option<(u64, u8)>,
}

/// Reduce a received From-DS data frame. `mpdu` starts at the 802.11 header; `pad` as for
/// [`parse_mgmt`]. A protected frame must have been decrypted by the firmware (the caller checks
/// the descriptor status); its 8-byte CCMP header is read for the packet number and skipped.
pub fn parse_data(mpdu: &[u8], pad: bool) -> Result<DataIn<'_>, RxRefusal> {
    if mpdu.len() < HDR_LEN {
        return Err(RxRefusal::Short { have: mpdu.len(), need: HDR_LEN });
    }
    if mpdu[0] & FC0_TYPE_MASK != FC0_TYPE_DATA {
        return Err(RxRefusal::WrongType(mpdu[0]));
    }
    if mpdu[1] & FC1_DS_MASK != FC1_FROM_DS {
        return Err(RxRefusal::NotFromDs(mpdu[1]));
    }
    if mpdu[0] & FC0_NULL != 0 {
        return Err(RxRefusal::NoPayload);
    }
    let hdr = if mpdu[0] & FC0_QOS != 0 { QOS_HDR_LEN } else { HDR_LEN };
    let mut o = hdr + if pad { 2 } else { 0 };
    let protected = mpdu[1] & FC1_PROTECTED != 0;
    let mut pn = None;
    if protected {
        if mpdu.len() < o + CCMP_HDR_LEN {
            return Err(RxRefusal::Short { have: mpdu.len(), need: o + CCMP_HDR_LEN });
        }
        let h = &mpdu[o..o + CCMP_HDR_LEN];
        if h[3] & 0x20 == 0 {
            return Err(RxRefusal::NotCcmp(h[3]));
        }
        let v = (h[0] as u64) | (h[1] as u64) << 8 | (h[4] as u64) << 16 | (h[5] as u64) << 24 | (h[6] as u64) << 32 | (h[7] as u64) << 40;
        pn = Some((v, h[3] >> 6));
        o += CCMP_HDR_LEN;
    }
    if mpdu.len() < o + LLC_SNAP_LEN {
        return Err(RxRefusal::Short { have: mpdu.len(), need: o + LLC_SNAP_LEN });
    }
    if mpdu[o..o + 6] != [0xaa, 0xaa, 0x03, 0x00, 0x00, 0x00] {
        return Err(RxRefusal::NotSnap);
    }
    let mut d = DataIn {
        da: [0; 6],
        sa: [0; 6],
        bssid: [0; 6],
        ethertype: u16::from_be_bytes([mpdu[o + 6], mpdu[o + 7]]),
        payload: &mpdu[o + LLC_SNAP_LEN..],
        protected,
        pn,
    };
    d.da.copy_from_slice(&mpdu[4..10]);
    d.bssid.copy_from_slice(&mpdu[10..16]);
    d.sa.copy_from_slice(&mpdu[16..22]);
    Ok(d)
}
