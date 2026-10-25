// SPDX-License-Identifier: GPL-2.0-only
//! What the join needs to know about an access point, read from one Beacon or Probe Response
//! (IEEE Std 802.11-2020 9.3.3.3 / 9.3.3.11 and the elements of 9.4.2). Only legacy rates are
//! kept: this driver associates as an 11a/g station.

use crate::dot11::*;
use crate::rates;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bss<'a> {
    pub bssid: Mac,
    pub ssid: &'a [u8],
    /// TSF timestamp field.
    pub tsf: u64,
    pub beacon_interval: u16,
    pub capability: u16,
    /// DS Parameter Set channel, else the HT Operation primary channel, else `None`.
    pub channel: Option<u8>,
    /// Bit `i` set when rate index `i` ([`rates`]) is in the BSS rate set / the BSSBasicRateSet.
    pub supported_rates: u16,
    pub basic_rates: u16,
    /// `(dtim_count, dtim_period)` from the TIM element; `None` for a probe response.
    pub dtim: Option<(u8, u8)>,
    /// ERP element: protection required / long preambles required.
    pub use_protection: bool,
    pub barker_preamble: bool,
    /// The whole RSN element (id, length, body), if any.
    pub rsne: Option<&'a [u8]>,
    pub has_ht: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BssRefusal {
    /// Not a beacon or probe response; carries frame control byte 0.
    NotBeacon(u8),
    /// Shorter than the fixed fields.
    Short { have: usize, need: usize },
    /// An element overruns the frame.
    ElementOverrun,
    /// The SSID element is longer than 32 bytes or missing.
    BadSsid,
}

/// Reduce a Beacon or Probe Response (`mpdu` from the 802.11 header, `pad` as for
/// [`crate::dot11::parse_mgmt`]).
pub fn parse_beacon(mpdu: &[u8], pad: bool) -> Result<Bss<'_>, BssRefusal> {
    let m = parse_mgmt(mpdu, pad).map_err(|e| match e {
        RxRefusal::Short { have, need } => BssRefusal::Short { have, need },
        _ => BssRefusal::NotBeacon(mpdu.first().copied().unwrap_or(0)),
    })?;
    if m.subtype != SUBTYPE_BEACON && m.subtype != SUBTYPE_PROBE_RESP {
        return Err(BssRefusal::NotBeacon(mpdu[0]));
    }
    if m.body.len() < 12 {
        return Err(BssRefusal::Short { have: m.body.len(), need: 12 });
    }
    let b = m.body;
    let mut out = Bss {
        bssid: m.bssid,
        ssid: &[],
        tsf: u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        beacon_interval: u16::from_le_bytes([b[8], b[9]]),
        capability: u16::from_le_bytes([b[10], b[11]]),
        channel: None,
        supported_rates: 0,
        basic_rates: 0,
        dtim: None,
        use_protection: false,
        barker_preamble: false,
        rsne: None,
        has_ht: false,
    };
    let mut ies = &b[12..];
    let mut have_ssid = false;
    let mut ht_primary = None;
    while ies.len() >= 2 {
        let (id, len) = (ies[0], ies[1] as usize);
        let Some(body) = ies.get(2..2 + len) else { return Err(BssRefusal::ElementOverrun) };
        match id {
            EID_SSID => {
                if len > 32 {
                    return Err(BssRefusal::BadSsid);
                }
                out.ssid = body;
                have_ssid = true;
            }
            EID_SUPP_RATES | EID_EXT_RATES => {
                for &r in body {
                    if let Some(i) = rates::index_of(r) {
                        out.supported_rates |= 1 << i;
                        if rates::is_basic(r) {
                            out.basic_rates |= 1 << i;
                        }
                    }
                }
            }
            EID_DS_PARAMS if len == 1 => out.channel = Some(body[0]),
            EID_TIM if len >= 2 => out.dtim = Some((body[0], body[1])),
            EID_ERP if len >= 1 => {
                out.use_protection = body[0] & 0x02 != 0;
                out.barker_preamble = body[0] & 0x04 != 0;
            }
            EID_HT_CAPS => out.has_ht = true,
            EID_HT_OPERATION if len >= 1 => ht_primary = Some(body[0]),
            EID_RSN => out.rsne = Some(&ies[..2 + len]),
            _ => {}
        }
        ies = &ies[2 + len..];
    }
    if !have_ssid {
        return Err(BssRefusal::BadSsid);
    }
    if out.channel.is_none() {
        out.channel = ht_primary;
    }
    Ok(out)
}

/// The 802.11 rate octets to offer in an association request: every rate of the BSS this station
/// supports (all twelve legacy rates), in ascending order, basic flag clear. Returns the count.
pub fn offered_rates(bss_supported: u16, out: &mut [u8; 12]) -> usize {
    let mut n = 0;
    for i in 0..rates::RATE_COUNT {
        if bss_supported & (1 << i) != 0 {
            out[n] = rates::UNITS_500K[i];
            n += 1;
        }
    }
    n
}
