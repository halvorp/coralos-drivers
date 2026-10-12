// SPDX-License-Identifier: GPL-2.0-only
//! Sink capabilities: the DPCD receiver-capability block, the eDP 1.4 link-rate table, the source (SKL) rate table, the
//! common rates and lane counts, and the link-parameter reduction Linux applies after a failed training.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dp.c (v7.0-rc6): `intel_dp_set_dpcd_sink_rates`
//! (:209-277), `intel_dp_set_max_sink_lane_count` (:304-325), `intel_dp_rate_limit_len` (:330-341),
//! `intel_dp_set_source_rates` (:580-662), `intersect_rates` (:664-685), `intel_dp_rate_index` (:688-697),
//! `intel_dp_set_common_rates` (:799-818), `intel_dp_rate_select` / `intel_dp_compute_rate` (:1582-1610),
//! `intel_edp_set_sink_rates` (:4523-4574), `intel_edp_init_dpcd` (:4577-4640); drm_dp_helper.c `drm_dp_read_dpcd_caps`
//! (:1240-1259), `drm_dp_read_extended_dpcd_caps` (:1188-1225), `drm_dp_link_rate_to_bw_code` / `drm_dp_bw_code_to_link_rate`
//! (:530-560) and drm_dp_helper.h `drm_dp_*_supported` inlines (:147-205); intel_dp_link_training.c `reduce_link_rate`,
//! `reduce_lane_count`, `reduce_link_params_in_rate_lane_order` (:1238-1295).
//! Copyright (c) 2008 Intel Corporation.
//!
//! NOT PORTED (named): the 128b/132b UHBR rate list (a KBL source's `skl_rates` stop at 5.4 Gbps, so UHBR rates
//! intersect away), `drm_dp_read_desc` (OUI / branch ID, :4588) and the DPCD quirk table it feeds,
//! `intel_dp_get_colorimetry_status`, PSR / DSC / backlight-AUX capability reads, BDB 263 `edp_data_rate_override`
//! (`intel_bios_encoder_reject_edp_rate`), `QUIRK_EDP_LIMIT_RATE_HBR2`, forced rate/lane debugfs.
//!
//! DEVIATION: `intel_edp_set_sink_rates()` ignores the return of the 16-byte `DP_SUPPORTED_LINK_RATES` read (:4533) and so
//! parses stack garbage on failure; here a failed read is an all-zero table (no rates), which falls back to the
//! DPCD `MAX_LINK_RATE` path exactly as an eDP < 1.4 panel does.
use crate::dpcd::{DpcdFail, DpcdOk, DpcdRequest};
use crate::dpdefs::*;

/// `DP_MAX_SUPPORTED_RATES` — drm_dp.h:181. Capacity of the sink and common rate arrays.
pub const MAX_RATES: usize = DP_MAX_SUPPORTED_RATES;

/// A short sorted list of link rates (units: kHz of LS_Clk, i.e. 162000 = 1.62 Gbps, Linux's `port_clock`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rates {
    len: usize,
    v: [u32; MAX_RATES],
}

impl Rates {
    pub const fn new() -> Rates {
        Rates { len: 0, v: [0; MAX_RATES] }
    }
    /// Build from a slice (at most [`MAX_RATES`]; extras are dropped).
    pub fn from_slice(s: &[u32]) -> Rates {
        let mut r = Rates::new();
        for &x in s.iter().take(MAX_RATES) {
            r.v[r.len] = x;
            r.len += 1;
        }
        r
    }
    pub fn as_slice(&self) -> &[u32] {
        &self.v[..self.len]
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    /// `intel_dp_rate_index()` — intel_dp.c:688-697: index of `rate`, or `None` (-1).
    pub fn index_of(&self, rate: u32) -> Option<usize> {
        self.as_slice().iter().position(|&r| r == rate)
    }
    fn push(&mut self, x: u32) -> bool {
        if self.len == MAX_RATES {
            return false;
        }
        self.v[self.len] = x;
        self.len += 1;
        true
    }
}

impl Default for Rates {
    fn default() -> Rates {
        Rates::new()
    }
}

/// `drm_dp_bw_code_to_link_rate()` — drm_dp_helper.c:546-560.
pub const fn bw_code_to_link_rate(link_bw: u8) -> u32 {
    match link_bw {
        DP_LINK_BW_10 => 1_000_000,
        DP_LINK_BW_13_5 => 1_350_000,
        DP_LINK_BW_20 => 2_000_000,
        // "Spec says link_rate = link_bw * 0.27Gbps"
        _ => link_bw as u32 * 27_000,
    }
}

/// `drm_dp_link_rate_to_bw_code()` — drm_dp_helper.c:530-544.
pub const fn link_rate_to_bw_code(link_rate: u32) -> u8 {
    match link_rate {
        1_000_000 => DP_LINK_BW_10,
        1_350_000 => DP_LINK_BW_13_5,
        2_000_000 => DP_LINK_BW_20,
        // "Spec says link_bw = link_rate / 0.27Gbps"
        _ => (link_rate / 27_000) as u8,
    }
}

/// `drm_dp_is_uhbr_rate()` — drm_dp_helper.h: 10, 13.5 and 20 Gbps (UHBR) are the rates >= 1 Gbps.
pub const fn is_uhbr_rate(link_rate: u32) -> bool {
    link_rate >= 1_000_000
}

// ---------------------------------------------------------------------------------------------------------------
// Pure accessors over the 15-byte receiver-capability block (drm_dp_helper.h:147-205)
// ---------------------------------------------------------------------------------------------------------------
/// `drm_dp_max_lane_count()` — drm_dp_helper.h:147-150.
pub const fn dp_max_lane_count(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> u8 {
    dpcd[DP_MAX_LANE_COUNT as usize] & DP_MAX_LANE_COUNT_MASK
}
/// `drm_dp_enhanced_frame_cap()` — drm_dp_helper.h:152-157.
pub const fn dp_enhanced_frame_cap(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> bool {
    dpcd[DP_DPCD_REV as usize] >= 0x11 && dpcd[DP_MAX_LANE_COUNT as usize] & DP_ENHANCED_FRAME_CAP != 0
}
/// `drm_dp_tps3_supported()` — drm_dp_helper.h:173-178.
pub const fn dp_tps3_supported(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> bool {
    dpcd[DP_DPCD_REV as usize] >= 0x12 && dpcd[DP_MAX_LANE_COUNT as usize] & DP_TPS3_SUPPORTED != 0
}
/// `drm_dp_tps4_supported()` — drm_dp_helper.h:187-192.
pub const fn dp_tps4_supported(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> bool {
    dpcd[DP_DPCD_REV as usize] >= 0x14 && dpcd[DP_MAX_DOWNSPREAD as usize] & DP_TPS4_SUPPORTED != 0
}
/// `drm_dp_is_branch()` — drm_dp_helper.h:201-205.
pub const fn dp_is_branch(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> bool {
    dpcd[DP_DOWNSTREAMPORT_PRESENT as usize] & DP_DWN_STRM_PORT_PRESENT != 0
}

/// Everything `intel_edp_init_dpcd()` learns about the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SinkCaps {
    /// `intel_dp->dpcd` (base block, replaced by the extended block when that is newer).
    pub dpcd: [u8; DP_RECEIVER_CAP_SIZE],
    /// `intel_dp->edp_dpcd` (0x700..0x704); all zero if the read failed.
    pub edp_dpcd: [u8; EDP_DISPLAY_CTL_CAP_SIZE],
    /// `intel_dp->use_max_params = edp_dpcd[0] < DP_EDP_14` (:4611). Stays false if the eDP block read failed.
    pub use_max_params: bool,
    /// `intel_dp->sink_rates` after `intel_edp_set_sink_rates()`.
    pub sink_rates: Rates,
    /// `intel_dp->use_rate_select`: program `DP_LINK_RATE_SET` instead of `DP_LINK_BW_SET` (eDP 1.4 rate table present).
    pub use_rate_select: bool,
    /// `intel_dp->max_sink_lane_count` after `intel_dp_set_max_sink_lane_count()`.
    pub max_sink_lane_count: u8,
    /// `intel_dp_set_default_sink_rates()` was used ("Invalid DPCD with no link rates, using defaults", :290-293).
    pub rates_defaulted: bool,
    /// `intel_dp_set_default_max_sink_lane_count()` was used ("Invalid DPCD max lane count", :319-323).
    pub lane_count_defaulted: bool,
}

/// `intel_dp_set_dpcd_sink_rates()` (8b/10b part) — intel_dp.c:209-277.
fn dpcd_sink_rates(dpcd: &[u8; DP_RECEIVER_CAP_SIZE]) -> Rates {
    const DP_RATES: [u32; 4] = [162_000, 270_000, 540_000, 810_000]; // intel_dp.c:211-213
    let max_rate = bw_code_to_link_rate(dpcd[DP_MAX_LINK_RATE as usize]);
    let mut r = Rates::new();
    for &x in DP_RATES.iter() {
        if x > max_rate {
            break;
        }
        r.push(x);
    }
    r
}

/// `intel_edp_set_sink_rates()` — intel_dp.c:4523-4574 (without the rate-override reject).
fn edp_sink_rates(raw: &[u8; 16]) -> Rates {
    let mut r = Rates::new();
    for i in 0..MAX_RATES {
        // "Value read multiplied by 200kHz gives the per-lane link rate in kHz ... rate = le16 * 200 / 10" — :4545
        let rate = u16::from_le_bytes([raw[2 * i], raw[2 * i + 1]]) as u32 * 200 / 10;
        if rate == 0 {
            break;
        }
        r.push(rate);
    }
    r
}

/// Build [`SinkCaps`] from the raw blocks, applying the rules of `intel_edp_init_dpcd()` and its callees. `rates_raw` is
/// the 16 bytes at `DP_SUPPORTED_LINK_RATES` (all zero if not read / the read failed).
pub fn sink_caps_from_blocks(
    dpcd: [u8; DP_RECEIVER_CAP_SIZE],
    edp_dpcd: Option<[u8; EDP_DISPLAY_CTL_CAP_SIZE]>,
    rates_raw: &[u8; 16],
) -> SinkCaps {
    let use_max_params = match edp_dpcd {
        Some(e) => e[0] < DP_EDP_14,
        None => false,
    };
    let edp_dpcd = edp_dpcd.unwrap_or([0; EDP_DISPLAY_CTL_CAP_SIZE]);

    let mut sink_rates = Rates::new();
    let mut use_rate_select = false;
    let mut rates_defaulted = false;
    if edp_dpcd[0] >= DP_EDP_14 {
        sink_rates = edp_sink_rates(rates_raw);
    }
    // "Use DP_LINK_RATE_SET if DP_SUPPORTED_LINK_RATES are available, default to DP_MAX_LINK_RATE and DP_LINK_BW_SET otherwise."
    if !sink_rates.is_empty() {
        use_rate_select = true;
    } else {
        sink_rates = dpcd_sink_rates(&dpcd);
        if sink_rates.is_empty() {
            sink_rates = Rates::from_slice(&[162_000]); // intel_dp_set_default_sink_rates, :202-206
            rates_defaulted = true;
        }
    }

    let lanes = dp_max_lane_count(&dpcd);
    let (max_sink_lane_count, lane_count_defaulted) = match lanes {
        1 | 2 | 4 => (lanes, false),
        _ => (1, true), // :316-325
    };

    SinkCaps { dpcd, edp_dpcd, use_max_params, sink_rates, use_rate_select, max_sink_lane_count, rates_defaulted, lane_count_defaulted }
}

/// Why capability discovery failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapsError {
    /// `drm_dp_dpcd_read_data(DP_DPCD_REV, 15)` failed (drm_dp_helper.c:1245).
    BaseReadFailed(DpcdFail),
    /// `dpcd[DP_DPCD_REV] == 0` — "-EIO" (drm_dp_helper.c:1248-1249): the sink did not answer with a revision.
    DpcdRevZero,
    /// The extended-capability read at `DP_DP13_DPCD_REV` failed (drm_dp_helper.c:1205-1208).
    ExtendedReadFailed(DpcdFail),
    /// The ALPM-capability read failed, which makes `intel_edp_init_dpcd()` return false (:4621-4624).
    AlpmReadFailed(DpcdFail),
    /// A DPCD read returned fewer bytes than asked.
    ShortRead { what: &'static str, got: usize, want: usize },
    /// The probe was stepped with a reply of the wrong shape (no request outstanding / reply missing).
    OutOfSequence,
}

impl CapsError {
    pub const fn name(&self) -> &'static str {
        match self {
            CapsError::BaseReadFailed(_) => "CAPS-BASE-READ-FAILED",
            CapsError::DpcdRevZero => "CAPS-DPCD-REV-ZERO",
            CapsError::ExtendedReadFailed(_) => "CAPS-EXT-READ-FAILED",
            CapsError::AlpmReadFailed(_) => "CAPS-ALPM-READ-FAILED",
            CapsError::ShortRead { .. } => "CAPS-SHORT-READ",
            CapsError::OutOfSequence => "CAPS-OUT-OF-SEQUENCE",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Phase {
    Base,
    Ext,
    Edp,
    Alpm,
    Rates,
    Finished,
}

/// What the probe wants next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeStep {
    Request(DpcdRequest),
    Done(Result<SinkCaps, CapsError>),
}

/// `intel_edp_init_dpcd()`'s read sequence as a machine: [`CapsProbe::step`] with `None`, perform the
/// [`DpcdRequest`], `step` again with `Some(result)`.
#[derive(Clone, Copy, Debug)]
pub struct CapsProbe {
    phase: Phase,
    dpcd: [u8; DP_RECEIVER_CAP_SIZE],
    edp: Option<[u8; EDP_DISPLAY_CTL_CAP_SIZE]>,
}

impl Default for CapsProbe {
    fn default() -> CapsProbe {
        CapsProbe::new()
    }
}

impl CapsProbe {
    pub const fn new() -> CapsProbe {
        CapsProbe { phase: Phase::Base, dpcd: [0; DP_RECEIVER_CAP_SIZE], edp: None }
    }

    fn finish(&mut self, r: Result<SinkCaps, CapsError>) -> ProbeStep {
        self.phase = Phase::Finished;
        ProbeStep::Done(r)
    }

    pub fn step(&mut self, reply: Option<Result<DpcdOk, DpcdFail>>) -> ProbeStep {
        match (self.phase, reply) {
            (Phase::Base, None) => ProbeStep::Request(DpcdRequest::read(DP_DPCD_REV, DP_RECEIVER_CAP_SIZE)),
            (Phase::Base, Some(Err(e))) => self.finish(Err(CapsError::BaseReadFailed(e))),
            (Phase::Base, Some(Ok(ok))) => {
                if ok.len != DP_RECEIVER_CAP_SIZE {
                    return self.finish(Err(CapsError::ShortRead { what: "dpcd", got: ok.len, want: DP_RECEIVER_CAP_SIZE }));
                }
                self.dpcd.copy_from_slice(&ok.data[..DP_RECEIVER_CAP_SIZE]);
                if self.dpcd[DP_DPCD_REV as usize] == 0 {
                    return self.finish(Err(CapsError::DpcdRevZero));
                }
                // drm_dp_read_extended_dpcd_caps(): only if the sink says an extended block exists (:1200-1203).
                if self.dpcd[DP_TRAINING_AUX_RD_INTERVAL as usize] & DP_EXTENDED_RECEIVER_CAP_FIELD_PRESENT != 0 {
                    self.phase = Phase::Ext;
                    ProbeStep::Request(DpcdRequest::read(DP_DP13_DPCD_REV, DP_RECEIVER_CAP_SIZE))
                } else {
                    self.phase = Phase::Edp;
                    ProbeStep::Request(DpcdRequest::read(DP_EDP_DPCD_REV, EDP_DISPLAY_CTL_CAP_SIZE))
                }
            }
            (Phase::Ext, Some(Err(e))) => self.finish(Err(CapsError::ExtendedReadFailed(e))),
            (Phase::Ext, Some(Ok(ok))) => {
                if ok.len != DP_RECEIVER_CAP_SIZE {
                    return self.finish(Err(CapsError::ShortRead { what: "dpcd-ext", got: ok.len, want: DP_RECEIVER_CAP_SIZE }));
                }
                let mut ext = [0u8; DP_RECEIVER_CAP_SIZE];
                ext.copy_from_slice(&ok.data[..DP_RECEIVER_CAP_SIZE]);
                // "if (dpcd[DP_DPCD_REV] > dpcd_ext[DP_DPCD_REV]) ... return 0" — keep the base block (:1210-1215).
                // Identical blocks need no copy (:1217-1218); otherwise the extended block REPLACES the base (:1222).
                if self.dpcd[DP_DPCD_REV as usize] <= ext[DP_DPCD_REV as usize] {
                    self.dpcd = ext;
                }
                self.phase = Phase::Edp;
                ProbeStep::Request(DpcdRequest::read(DP_EDP_DPCD_REV, EDP_DISPLAY_CTL_CAP_SIZE))
            }
            (Phase::Edp, Some(r)) => {
                // Only a complete read counts (":4605 ... == sizeof(intel_dp->edp_dpcd)"); anything else leaves edp_dpcd zero.
                if let Ok(ok) = r {
                    if ok.len == EDP_DISPLAY_CTL_CAP_SIZE {
                        let mut e = [0u8; EDP_DISPLAY_CTL_CAP_SIZE];
                        e.copy_from_slice(&ok.data[..EDP_DISPLAY_CTL_CAP_SIZE]);
                        self.edp = Some(e);
                    }
                }
                self.phase = Phase::Alpm;
                ProbeStep::Request(DpcdRequest::read(DP_RECEIVER_ALPM_CAP, 1))
            }
            (Phase::Alpm, Some(Err(e))) => self.finish(Err(CapsError::AlpmReadFailed(e))),
            (Phase::Alpm, Some(Ok(_))) => {
                let edp0 = self.edp.map(|e| e[0]).unwrap_or(0);
                if edp0 >= DP_EDP_14 {
                    self.phase = Phase::Rates;
                    ProbeStep::Request(DpcdRequest::read(DP_SUPPORTED_LINK_RATES, 16))
                } else {
                    let caps = sink_caps_from_blocks(self.dpcd, self.edp, &[0; 16]);
                    self.finish(Ok(caps))
                }
            }
            (Phase::Rates, Some(r)) => {
                let mut raw = [0u8; 16];
                if let Ok(ok) = r {
                    if ok.len == 16 {
                        raw.copy_from_slice(&ok.data[..16]);
                    }
                }
                let caps = sink_caps_from_blocks(self.dpcd, self.edp, &raw);
                self.finish(Ok(caps))
            }
            _ => self.finish(Err(CapsError::OutOfSequence)),
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Source rates, common rates, rate select
// ---------------------------------------------------------------------------------------------------------------
/// `skl_rates[]` — intel_dp.c:598-600 (display version 9 and not BXT/GLK, :639-641). Increasing order.
pub const SKL_SOURCE_RATES: [u32; 6] = [162_000, 216_000, 270_000, 324_000, 432_000, 540_000];

/// `intel_dp_rate_limit_len()` — intel_dp.c:330-341: how many of the (ascending) `rates` are <= `max_rate`.
pub fn rate_limit_len(rates: &[u32], max_rate: u32) -> usize {
    let len = rates.len();
    for i in 0..len {
        if rates[len - i - 1] <= max_rate {
            return len - i;
        }
    }
    0
}

/// `vbt_max_link_rate()` — intel_dp.c:559-577 for an eDP port: the child device's cap (0 = none) and the eDP block's
/// `max_link_rate` (BDB 244+; `None` = absent). The smaller wins when both are set.
pub fn vbt_max_link_rate(child_max: u32, edp_max: Option<u32>) -> u32 {
    let edp_max_rate = edp_max.unwrap_or(0);
    if child_max != 0 && edp_max_rate != 0 {
        if child_max < edp_max_rate {
            child_max
        } else {
            edp_max_rate
        }
    } else if edp_max_rate != 0 {
        edp_max_rate
    } else {
        child_max
    }
}

/// `intel_dp_set_source_rates()` for Kaby Lake — intel_dp.c:580-662: `skl_rates`, cut to the VBT's maximum when it has one
/// (`max_rate` starts 0 for display ver 9, so the VBT value alone sets it, :643-651).
pub fn source_rates(vbt_max_rate: u32) -> Rates {
    let mut n = SKL_SOURCE_RATES.len();
    if vbt_max_rate != 0 {
        n = rate_limit_len(&SKL_SOURCE_RATES, vbt_max_rate);
    }
    Rates::from_slice(&SKL_SOURCE_RATES[..n])
}

/// `intersect_rates()` + the "Paranoia" fallback of `intel_dp_set_common_rates()` — intel_dp.c:664-685, :799-818.
pub fn common_rates(source: &Rates, sink: &Rates) -> Rates {
    let (s, k) = (source.as_slice(), sink.as_slice());
    let (mut i, mut j) = (0, 0);
    let mut out = Rates::new();
    while i < s.len() && j < k.len() {
        if s[i] == k[j] {
            if !out.push(s[i]) {
                break; // WARN_ON(k >= DP_MAX_SUPPORTED_RATES) returns k — :672-673
            }
            i += 1;
            j += 1;
        } else if s[i] < k[j] {
            i += 1;
        } else {
            j += 1;
        }
    }
    if out.is_empty() {
        out.push(162_000); // "there should always be something in common" — :811-815
    }
    out
}

/// The `link_bw` / `rate_select` pair `intel_dp_compute_rate()` writes (intel_dp.c:1594-1610).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RateSel {
    pub link_bw: u8,
    pub rate_select: u8,
}

/// `intel_dp_compute_rate()` for a non-g4x part. With `use_rate_select` the rate is looked up in the SINK table
/// (`intel_dp_rate_select()`, :1582-1592: not found is `drm_WARN_ON` and index 0 — here a named `Err`).
pub fn compute_rate(caps: &SinkCaps, port_clock: u32) -> Result<RateSel, RateSelectError> {
    if caps.use_rate_select {
        match caps.sink_rates.index_of(port_clock) {
            Some(i) => Ok(RateSel { link_bw: 0, rate_select: i as u8 }),
            None => Err(RateSelectError::RateNotInSinkTable { port_clock }),
        }
    } else {
        Ok(RateSel { link_bw: link_rate_to_bw_code(port_clock), rate_select: 0 })
    }
}

/// Refusals of the rate selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RateSelectError {
    /// `drm_WARN_ON(i < 0)` in `intel_dp_rate_select()` (:1587): the port clock is not one of the sink's rates.
    RateNotInSinkTable { port_clock: u32 },
}

impl RateSelectError {
    pub const fn name(&self) -> &'static str {
        match self {
            RateSelectError::RateNotInSinkTable { .. } => "RATE-NOT-IN-SINK-TABLE",
        }
    }
}

/// `intel_ddi_max_lanes()` for DDI-A, display ver < 11 — intel_ddi.c:4951-4980: 4 lanes if `DDI_A_4_LANES` is set in
/// DDI_BUF_CTL(A), else 2 ("Both A and E share 2 lanes").
pub const fn ddi_a_max_lanes(ddi_buf_ctl_a: u32) -> u8 {
    if ddi_buf_ctl_a & i915_kbl_display_core::regs::DDI_A_4_LANES != 0 {
        4
    } else {
        2
    }
}

/// `intel_dp_max_common_lane_count()` for a non-Type-C port — intel_dp.c:379-397. `vbt_max_lanes` 0 = unspecified
/// (intel_dp_max_source_lane_count, :368-377).
pub fn max_common_lane_count(port_max_lanes: u8, vbt_max_lanes: u8, sink_max: u8) -> u8 {
    let mut source_max = port_max_lanes;
    if vbt_max_lanes != 0 && vbt_max_lanes < source_max {
        source_max = vbt_max_lanes;
    }
    if source_max < sink_max {
        source_max
    } else {
        sink_max
    }
}

/// `reduce_link_rate()` — intel_dp_link_training.c:1238-1260 (no forced rate): the next lower COMMON rate, or `None`
/// (-1) at the bottom or when the step would cross between UHBR and non-UHBR.
pub fn reduce_link_rate(common: &Rates, current_rate: u32) -> Option<u32> {
    match common.index_of(current_rate) {
        Some(i) if i > 0 => {
            let new_rate = common.as_slice()[i - 1];
            if is_uhbr_rate(current_rate) != is_uhbr_rate(new_rate) {
                None
            } else {
                Some(new_rate)
            }
        }
        _ => None,
    }
}

/// `reduce_lane_count()` — intel_dp_link_training.c:1262-1271 (no forced count): halve, `None` at one lane.
pub fn reduce_lane_count(current_lane_count: u8) -> Option<u8> {
    if current_lane_count == 1 {
        None
    } else {
        Some(current_lane_count >> 1)
    }
}

/// `reduce_link_params_in_rate_lane_order()` — intel_dp_link_training.c:1273-1294: drop the rate first; when it is already
/// lowest, halve the lane count and go back to the MAXIMUM common rate. `None` = nothing left to try.
pub fn reduce_link_params(common: &Rates, port_clock: u32, lane_count: u8) -> Option<(u32, u8)> {
    let mut lanes = Some(lane_count);
    let rate = match reduce_link_rate(common, port_clock) {
        Some(r) => r,
        None => {
            lanes = reduce_lane_count(lane_count);
            *common.as_slice().last()? // intel_dp_max_common_rate()
        }
    };
    Some((rate, lanes?))
}
