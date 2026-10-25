// SPDX-License-Identifier: GPL-2.0-only
//! The 802.11 management frames a passive scan hears: beacons and probe responses, reduced to what a scan list
//! shows — SSID, BSSID, channel, RSSI and the security the network asks of a station. The element walk is
//! IEEE 802.11-2020 9.3.3.3 (beacon) / 9.3.3.11 (probe response) / 9.4.2; the security policy is the one the brcmfmac
//! driver's `security_of` already applies for wifi-manager (userland/coral-brcmfmac/src/station.rs): no privacy bit =
//! Open; an RSN element offering PSK and CCMP = Wpa2Psk; anything else private = Unsupported (shown, never offered).

use crate::decode::RxMpdu;

pub const SSID_MAX: usize = 32;
const FC_TYPE_MASK: u8 = 0x0c;
const FC_TYPE_MGMT: u8 = 0x00;
const FC_SUBTYPE_MASK: u8 = 0xf0;
const SUBTYPE_PROBE_RESP: u8 = 0x50;
const SUBTYPE_BEACON: u8 = 0x80;
/// Management header: fc, duration, addr1..3, seq-ctrl. // 802.11-2020 9.2.3
const MGMT_HDR: usize = 24;
/// Beacon / probe response fixed fields: timestamp 8, beacon interval 2, capability 2. // 802.11-2020 9.3.3.3
const FIXED: usize = 12;
const CAP_PRIVACY: u16 = 1 << 4;
const EID_SSID: u8 = 0;
const EID_DS_PARAMS: u8 = 3;
const EID_HT_OPERATION: u8 = 61;
const EID_RSN: u8 = 48;
const RSN_OUI: [u8; 3] = [0x00, 0x0f, 0xac];
const RSN_CIPHER_CCMP: u8 = 4;
const RSN_AKM_PSK: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Security {
    Open,
    Wpa2Psk,
    Unsupported,
}

/// One network heard in a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Heard {
    pub ssid_len: u8,
    pub ssid: [u8; SSID_MAX],
    pub bssid: [u8; 6],
    pub channel: u8,
    pub rssi: i16,
    pub security: Security,
}

/// Why a received frame was not turned into a [`Heard`]. Each is a normal event on a busy channel, counted by the
/// caller; none is an error of the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The hardware flagged a CRC or FIFO-overrun error. // mvm/rxmq.c:2188-2195
    BadCrc,
    /// Not a beacon or probe response.
    NotBeaconOrProbeResp,
    /// Shorter than a management header plus the fixed fields.
    Truncated,
    /// An element claims more bytes than the frame holds.
    ElementOverrun,
    /// The SSID element is longer than 32 bytes.
    SsidTooLong,
}

fn rsn_offers(el: &[u8]) -> bool {
    // el = the element body (after id and length): version u16, group suite 4, pairwise count u16 + suites,
    // AKM count u16 + suites. A lying count is false, never a read past the element. // brcmfmac rsn_ie::offers
    let suites = |off: usize| -> Option<(usize, &[u8])> {
        let n = u16::from_le_bytes([*el.get(off)?, *el.get(off + 1)?]) as usize;
        Some((off + 2 + 4 * n, el.get(off + 2..off + 2 + 4 * n)?))
    };
    let has = |list: &[u8], t: u8| list.chunks(4).any(|s| s[..3] == RSN_OUI && s[3] == t);
    let Some((after_pw, pw)) = suites(6) else { return false };
    let Some((_, akms)) = suites(after_pw) else { return false };
    has(pw, RSN_CIPHER_CCMP) && has(akms, RSN_AKM_PSK)
}

/// Reduce one received MPDU to a scan result.
pub fn heard_from_rx(rx: &RxMpdu<'_>) -> Result<Heard, Skip> {
    if !rx.crc_ok {
        return Err(Skip::BadCrc);
    }
    let f = rx.mpdu;
    if f.len() < MGMT_HDR + FIXED {
        return Err(Skip::Truncated);
    }
    let (fc0, subtype) = (f[0], f[0] & FC_SUBTYPE_MASK);
    if fc0 & FC_TYPE_MASK != FC_TYPE_MGMT || (subtype != SUBTYPE_BEACON && subtype != SUBTYPE_PROBE_RESP) {
        return Err(Skip::NotBeaconOrProbeResp);
    }
    // The device inserts two pad bytes after the header when it is not a multiple of four bytes; a management header
    // of 24 bytes (28 with an HT-control field) never is, but honour the flag. // fw/api/rx.h:187; mvm/rxmq.c:99-105
    let hdr = if rx.hdr_len >= MGMT_HDR { rx.hdr_len } else { MGMT_HDR };
    let body_off = hdr + if rx.padded { 2 } else { 0 };
    if f.len() < body_off + FIXED {
        return Err(Skip::Truncated);
    }
    let mut bssid = [0u8; 6];
    bssid.copy_from_slice(&f[16..22]); // addr3
    let cap = u16::from_le_bytes([f[body_off + 10], f[body_off + 11]]);
    let mut ies = &f[body_off + FIXED..];
    let (mut ssid, mut ssid_len) = ([0u8; SSID_MAX], 0u8);
    let mut ds: Option<u8> = None;
    let mut ht: Option<u8> = None;
    let mut rsn_psk = false;
    while ies.len() >= 2 {
        let (id, len) = (ies[0], ies[1] as usize);
        let Some(body) = ies.get(2..2 + len) else { return Err(Skip::ElementOverrun) };
        match id {
            EID_SSID => {
                if len > SSID_MAX {
                    return Err(Skip::SsidTooLong);
                }
                ssid[..len].copy_from_slice(body);
                ssid_len = len as u8;
            }
            EID_DS_PARAMS if len == 1 => ds = Some(body[0]),
            EID_HT_OPERATION if len >= 1 => ht = Some(body[0]),
            EID_RSN => rsn_psk |= rsn_offers(body),
            _ => {}
        }
        ies = &ies[2 + len..];
    }
    let security = if cap & CAP_PRIVACY == 0 {
        Security::Open
    } else if rsn_psk {
        Security::Wpa2Psk
    } else {
        Security::Unsupported
    };
    Ok(Heard { ssid_len, ssid, bssid, channel: ds.or(ht).unwrap_or(rx.channel), rssi: rx.rssi, security })
}
