// SPDX-License-Identifier: GPL-2.0-only
//! Legacy rates: the firmware's rate index, the PLCP code, the 802.11 rate unit (500 kbit/s), the
//! `rate_n_flags` word of rates version 1, and the ACK-rate bitmaps of the MAC context.
//! Ported from `fw/rs.c` (`fw_rate_idx_to_plcp`), `fw/api/rs.h` (indices, PLCP codes, version-1
//! flags), `mvm/mac-ctxt.c` (`iwl_mvm_ack_rates`, `iwl_mvm_mac_ctxt_get_lowest_rate`) and
//! `mvm/utils.c` (`iwl_mvm_v3_rate_to_fw`, version 1 branch).

use crate::ids::*;

/// `IWL_RATE_COUNT_LEGACY`: 4 CCK + 8 OFDM.
pub const RATE_COUNT: usize = 12; // fw/api/rs.h:401
pub const FIRST_CCK: usize = 0; // fw/api/rs.h:368
pub const LAST_CCK: usize = 3; // fw/api/rs.h:372
pub const FIRST_OFDM: usize = 4; // fw/api/rs.h:374
pub const RATE_1M: usize = 0; // fw/api/rs.h:367
pub const RATE_2M: usize = 1; // fw/api/rs.h:369
pub const RATE_5M: usize = 2; // fw/api/rs.h:370
pub const RATE_11M: usize = 3; // fw/api/rs.h:371
pub const RATE_6M: usize = 4; // fw/api/rs.h:373
pub const RATE_9M: usize = 5; // fw/api/rs.h:378
pub const RATE_12M: usize = 6; // fw/api/rs.h:379
pub const RATE_18M: usize = 7; // fw/api/rs.h:381
pub const RATE_24M: usize = 8; // fw/api/rs.h:383
pub const RATE_36M: usize = 9; // fw/api/rs.h:385
pub const RATE_48M: usize = 10; // fw/api/rs.h:387
pub const RATE_54M: usize = 11; // fw/api/rs.h:389

/// `fw_rate_idx_to_plcp`, indexed by rate index. // fw/rs.c:11-24
pub const PLCP: [u8; RATE_COUNT] = [10, 20, 55, 110, 13, 15, 5, 7, 9, 11, 1, 3];

/// The rate of each index in 802.11 units of 500 kbit/s (IEEE 802.11-2020 9.4.2.3).
pub const UNITS_500K: [u8; RATE_COUNT] = [2, 4, 11, 22, 12, 18, 24, 36, 48, 72, 96, 108];

/// The rate index of an 802.11 supported-rates octet (bit 7, the basic-rate flag, ignored).
pub fn index_of(octet: u8) -> Option<usize> {
    let r = octet & 0x7f;
    UNITS_500K.iter().position(|&u| u == r)
}

/// Whether the octet carries the BSSBasicRateSet flag.
pub fn is_basic(octet: u8) -> bool {
    octet & 0x80 != 0
}

pub fn is_cck(idx: usize) -> bool {
    idx <= LAST_CCK
}

/// `rate_n_flags` of rates version 1 for a legacy rate: PLCP code, the CCK flag for 1/2/5.5/11 Mbit/s,
/// and the antenna selection (`ant` bit 0 = A, bit 1 = B). // iwl_mvm_v3_rate_to_fw, mvm/utils.c
pub fn rate_n_flags_v1(idx: usize, ant: u8) -> u32 {
    let idx = idx.min(RATE_COUNT - 1);
    let mut f = PLCP[idx] as u32 & RATE_LEGACY_RATE_MSK_V1;
    if is_cck(idx) {
        f |= RATE_MCS_CCK_MSK_V1;
    }
    f | (((ant & 3) as u32) << RATE_MCS_ANT_POS)
}

/// The ACK-rate bitmaps of `struct iwl_mac_ctx_cmd`: bit `i` of the CCK map is rate index `i`, bit
/// `i` of the OFDM map is rate index `i + FIRST_OFDM`. `basic` has bit `idx` set for each basic
/// rate in the BSS rate set. Every mandatory rate below the lowest basic one is added so the
/// firmware can pick a valid response rate. // iwl_mvm_ack_rates, mvm/mac-ctxt.c:325-405
pub fn ack_rates(basic: u16) -> (u32, u32) {
    let (mut cck, mut ofdm) = (0u32, 0u32);
    let (mut lowest_cck, mut lowest_ofdm) = (100usize, 100usize);
    for i in 0..RATE_COUNT {
        if basic & (1 << i) == 0 {
            continue;
        }
        if i >= FIRST_OFDM {
            ofdm |= 1 << (i - FIRST_OFDM);
            lowest_ofdm = lowest_ofdm.min(i);
        } else {
            cck |= 1 << i;
            lowest_cck = lowest_cck.min(i);
        }
    }
    if RATE_24M < lowest_ofdm {
        ofdm |= 1 << (RATE_24M - FIRST_OFDM);
    }
    if RATE_12M < lowest_ofdm {
        ofdm |= 1 << (RATE_12M - FIRST_OFDM);
    }
    ofdm |= 1 << (RATE_6M - FIRST_OFDM);
    if RATE_11M < lowest_cck {
        cck |= 1 << RATE_11M;
    }
    if RATE_5M < lowest_cck {
        cck |= 1 << RATE_5M;
    }
    if RATE_2M < lowest_cck {
        cck |= 1 << RATE_2M;
    }
    cck |= 1 << RATE_1M;
    (cck, ofdm)
}

/// The rate management frames go out at: the lowest basic rate of the band. 2.4 GHz prefers CCK
/// (1 Mbit/s when no CCK rate is basic and no OFDM one either); 5 GHz is OFDM only.
/// // iwl_mvm_mac_ctxt_get_lowest_rate, mvm/mac-ctxt.c:872-917
pub fn lowest_rate(basic: u16, is_2ghz: bool) -> usize {
    let (mut lowest_cck, mut lowest_ofdm) = (RATE_COUNT, RATE_COUNT);
    for i in 0..RATE_COUNT {
        if basic & (1 << i) == 0 {
            continue;
        }
        if i >= FIRST_OFDM {
            lowest_ofdm = lowest_ofdm.min(i);
        } else {
            lowest_cck = lowest_cck.min(i);
        }
    }
    if is_2ghz {
        if lowest_cck != RATE_COUNT {
            lowest_cck
        } else if lowest_ofdm != RATE_COUNT {
            lowest_ofdm
        } else {
            RATE_1M
        }
    } else if lowest_ofdm != RATE_COUNT {
        lowest_ofdm
    } else {
        RATE_6M
    }
}
