// SPDX-License-Identifier: GPL-2.0-only
//! The render P-state facts a platform row implies: where `RP_STATE_CAP` keeps RP0 / RP1 / RPn, the factor that turns its
//! value into the request unit, the request field's shift, the CAGF field of `RPSTAT1`, and the unit's MHz. Ports of
//! `__gen6_rps_get_freq_caps` (gt/intel_rps.c:1136-1167), `gen6_rps_set` (:802-824), `intel_rps_get_cagf` (:2089-2110) and
//! `intel_gpu_freq` (:1656-1670), over register VALUES - the crate performs no MMIO.
//!
//! A platform whose P-state path is not ported gets `None` from [`RpsLayout::for_platform`] and is named by its caller, never
//! given another generation's layout: Cherryview (`vlv_rps_set` / the punit side band, intel_rps.c:826-839) is not ported.
use crate::{Platform, PlatformDescriptor};
use i915_common_core::reg::field_get;
use i915_gt_regs_core::{GEN9_CAGF_MASK, GEN9_FREQUENCY_SHIFT, RP0_CAP_MASK, RP1_CAP_MASK, RPN_CAP_MASK};

/// `GT_FREQUENCY_MULTIPLIER` (gt/intel_rps.h:15).
pub const GT_FREQUENCY_MULTIPLIER: u32 = 50; // intel_rps.h:15
/// `GEN9_FREQ_SCALER` (gt/intel_rps.h:16).
pub const GEN9_FREQ_SCALER: u32 = 3; // intel_rps.h:16

/// One platform's P-state register layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpsLayout {
    /// `RP_STATE_CAP`'s RP0 / RP1 / RPn fields (non-LP order, intel_rps.c:1149-1156).
    pub rp0_mask: u32,
    pub rp1_mask: u32,
    pub rpn_mask: u32,
    /// What `RP_STATE_CAP`'s fields are multiplied by to reach the request unit (`GEN9_FREQ_SCALER` on GEN9_BC, :1159-1167).
    pub cap_scaler: u32,
    /// The `RPNSWREQ` request field's shift (`GEN9_FREQUENCY`, :811-812).
    pub swreq_shift: u32,
    /// `RPSTAT1`'s CAGF field (`GEN9_CAGF_MASK`, :2100-2101).
    pub cagf_mask: u32,
    /// MHz = round(unit * `mhz_num` / `mhz_den`) (`intel_gpu_freq`, :1660-1662).
    pub mhz_num: u32,
    pub mhz_den: u32,
}

/// The P-state limits in request units (`struct intel_rps_freq_caps`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpsCaps {
    pub rp0: u32,
    pub rp1: u32,
    pub rpn: u32,
}

/// Gen9 big core (`IS_GEN9_BC`, i915_drv.h:543): Skylake and Kaby Lake.
const GEN9_BC: RpsLayout = RpsLayout {
    rp0_mask: RP0_CAP_MASK,
    rp1_mask: RP1_CAP_MASK,
    rpn_mask: RPN_CAP_MASK,
    cap_scaler: GEN9_FREQ_SCALER,
    swreq_shift: GEN9_FREQUENCY_SHIFT,
    cagf_mask: GEN9_CAGF_MASK,
    mhz_num: GT_FREQUENCY_MULTIPLIER,
    mhz_den: GEN9_FREQ_SCALER,
};

impl RpsLayout {
    /// The layout of `desc`'s platform, or `None` when its P-state path is not ported (Cherryview) or it has no RPS.
    pub const fn for_platform(desc: &PlatformDescriptor) -> Option<RpsLayout> {
        if !desc.info.has_rps {
            return None;
        }
        match desc.info.platform {
            Platform::Skylake | Platform::Kabylake if desc.info.graphics_ver == 9 && !desc.is_gen9_lp() => Some(GEN9_BC),
            _ => None,
        }
    }

    /// `__gen6_rps_get_freq_caps` over the `RP_STATE_CAP` value (intel_rps.c:1136-1167).
    pub const fn caps(&self, rp_state_cap: u32) -> RpsCaps {
        RpsCaps {
            rp0: field_get(self.rp0_mask, rp_state_cap) * self.cap_scaler,
            rp1: field_get(self.rp1_mask, rp_state_cap) * self.cap_scaler,
            rpn: field_get(self.rpn_mask, rp_state_cap) * self.cap_scaler,
        }
    }

    /// The `RPNSWREQ` value that requests `val` (`gen6_rps_set`, intel_rps.c:810-812).
    pub const fn swreq(&self, val: u32) -> u32 {
        val << self.swreq_shift
    }

    /// The current actual frequency (request units) in an `RPSTAT1` value (`intel_rps_get_cagf`, :2100-2101).
    pub const fn cagf(&self, rpstat1: u32) -> u32 {
        field_get(self.cagf_mask, rpstat1)
    }

    /// `intel_gpu_freq`: request units to MHz, `DIV_ROUND_CLOSEST` (intel_rps.c:1660-1662).
    pub const fn mhz(&self, val: u32) -> u32 {
        (val * self.mhz_num + self.mhz_den / 2) / self.mhz_den
    }
}
