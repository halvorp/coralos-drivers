// SPDX-License-Identifier: GPL-2.0-only
//! The Skylake / Kaby Lake power-well TABLE: which domains each well powers, the well order, and which well sits behind which.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display_power_map.c (`SKL_PW_2_POWER_DOMAINS`, `skl_pwdoms_*`,
//! `skl_power_wells_pw_1`, `skl_power_wells_main`, `skl_power_wells`) and intel_display_power.h (`enum
//! intel_display_power_domain`) at v7.0-rc6. Copyright (c) Intel Corporation.
//!
//! The wells themselves (`Well`, request/state bit positions) are `i915-kbl-display-core::power`'s; this module adds only the
//! domain -> well mapping and the order that module (a readout) has no use for.
//!
//! LEFT OUT, deliberately: the `always-on` well (no register: every domain is in it, `i9xx_pwdoms_always_on =
//! I915_PW_DOMAINS_ALL`, intel_display_power_map.c:50) and the `DC_off` well (intel_display_power_map.c:381-385; it is the DMC
//! DC-state gate, `gen9_dc_off_power_well_*`, owned by `i915-power-core` per plan item 12). [`Domain::needs_dc_off`] reports
//! the domains whose users must take it, so that crate can add it without re-deriving the list.
use i915_kbl_display_core::power::Well;

/// `enum intel_display_power_domain` restricted to the domains a Gen9 desktop-class part (no DSI, no Type-C PHY, no combo PHY)
/// uses (intel_display_power.h:23-118), in Linux's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Domain {
    DisplayCore,
    PipeA,
    PipeB,
    PipeC,
    PanelFitterA,
    PanelFitterB,
    PanelFitterC,
    TranscoderA,
    TranscoderB,
    TranscoderC,
    TranscoderEdp,
    DdiLanesA,
    DdiLanesB,
    DdiLanesC,
    DdiLanesD,
    DdiLanesE,
    DdiIoA,
    DdiIoB,
    DdiIoC,
    DdiIoD,
    DdiIoE,
    PortCrt,
    PortOther,
    Vga,
    AudioMmio,
    AudioPlayback,
    AuxIoA,
    AuxIoB,
    AuxIoC,
    AuxIoD,
    AuxA,
    AuxB,
    AuxC,
    AuxD,
    Gmbus,
    GtIrq,
    DcOff,
    Init,
}

impl Domain {
    pub const ALL: [Domain; 38] = [
        Domain::DisplayCore,
        Domain::PipeA,
        Domain::PipeB,
        Domain::PipeC,
        Domain::PanelFitterA,
        Domain::PanelFitterB,
        Domain::PanelFitterC,
        Domain::TranscoderA,
        Domain::TranscoderB,
        Domain::TranscoderC,
        Domain::TranscoderEdp,
        Domain::DdiLanesA,
        Domain::DdiLanesB,
        Domain::DdiLanesC,
        Domain::DdiLanesD,
        Domain::DdiLanesE,
        Domain::DdiIoA,
        Domain::DdiIoB,
        Domain::DdiIoC,
        Domain::DdiIoD,
        Domain::DdiIoE,
        Domain::PortCrt,
        Domain::PortOther,
        Domain::Vga,
        Domain::AudioMmio,
        Domain::AudioPlayback,
        Domain::AuxIoA,
        Domain::AuxIoB,
        Domain::AuxIoC,
        Domain::AuxIoD,
        Domain::AuxA,
        Domain::AuxB,
        Domain::AuxC,
        Domain::AuxD,
        Domain::Gmbus,
        Domain::GtIrq,
        Domain::DcOff,
        Domain::Init,
    ];
    pub const COUNT: usize = Domain::ALL.len();

    /// The registered wells (PW1 and MISC_IO have `I915_PW_DOMAINS_NONE`, so they never appear) that must be on for this
    /// domain, in Linux's ENABLE order (`for_each_power_domain_well` walks `skl_power_wells[]` forward; the put walks it
    /// backward, intel_display_power.c:38-43, :529, :625).
    pub const fn wells(self) -> &'static [Well] {
        match self {
            // SKL_PW_2_POWER_DOMAINS (intel_display_power_map.c:306-326) -> skl_pwdoms_pw_2 (:328-330)
            Domain::PipeB
            | Domain::PipeC
            | Domain::PanelFitterB
            | Domain::PanelFitterC
            | Domain::TranscoderA
            | Domain::TranscoderB
            | Domain::TranscoderC
            | Domain::DdiLanesB
            | Domain::DdiLanesC
            | Domain::DdiLanesD
            | Domain::DdiLanesE
            | Domain::Vga
            | Domain::AudioMmio
            | Domain::AudioPlayback
            | Domain::AuxIoB
            | Domain::AuxIoC
            | Domain::AuxIoD
            | Domain::AuxB
            | Domain::AuxC
            | Domain::AuxD => &[Well::Pw2],
            // skl_pwdoms_ddi_io_a_e (:339-342), _b (:344-346), _c (:348-350), _d (:352-354)
            Domain::DdiIoA | Domain::DdiIoE => &[Well::DdiAE],
            Domain::DdiIoB => &[Well::DdiB],
            Domain::DdiIoC => &[Well::DdiC],
            Domain::DdiIoD => &[Well::DdiD],
            // POWER_DOMAIN_INIT ends every one of the five lists above (:330, :342, :346, :350, :354)
            Domain::Init => &[Well::Pw2, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD],
            // In the always-on well only: no register-backed well to turn on.
            Domain::DisplayCore
            | Domain::PipeA
            | Domain::PanelFitterA
            | Domain::TranscoderEdp
            | Domain::DdiLanesA
            | Domain::PortCrt
            | Domain::PortOther
            | Domain::AuxIoA
            | Domain::AuxA
            | Domain::Gmbus
            | Domain::GtIrq
            | Domain::DcOff => &[],
        }
    }

    /// Is this domain in `skl_pwdoms_dc_off` (intel_display_power_map.c:332-337)? Those users also need the DC_off well,
    /// which belongs to `i915-power-core`.
    pub const fn needs_dc_off(self) -> bool {
        match self {
            Domain::AuxA | Domain::GtIrq | Domain::DcOff | Domain::Init => true,
            d => matches!(d.wells(), [Well::Pw2]),
        }
    }
}

/// `skl_power_wells[]` as the registered wells appear in it (intel_display_power_map.c:407-411 after the always-on well):
/// PW_1, MISC_IO, [DC_off], PW_2, then the four DDI IO wells. This is the ENABLE order; reverse for disable.
pub const WELL_ORDER: [Well; 7] = [Well::Pw1, Well::MiscIo, Well::Pw2, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD];

/// The well that must already be on before `well` may turn on.
///
/// PW2 sits behind PW1 in the power-gate chain (`pw_idx_to_pg`: PW1 = PG1, PW2 = PG2, intel_display_power_well.c:45-50; PG2's
/// fuse distribution follows PG1's, intel_display_power_well.c:397-418) and Linux's `skl_display_core_init` enables PW1 first (intel_display_power.c:1468-1469).
/// The DDI IO wells come after PW1 in `skl_power_wells[]`, so `for_each_power_well` never reaches them with PW1 off; this crate
/// refuses that order instead of relying on the array walk. MISC_IO has no ordering constraint of its own.
pub const fn requires(well: Well) -> Option<Well> {
    match well {
        Well::Pw1 | Well::MiscIo => None,
        Well::Pw2 | Well::DdiAE | Well::DdiB | Well::DdiC | Well::DdiD => Some(Well::Pw1),
    }
}

/// `SKL_PG*` of a well with fuses (`.has_fuses`: PW_1 and PW_2 only, intel_display_power_map.c:366, :395): the gate whose
/// distribution status the enable waits on. `pw_idx_to_pg` = `pw_idx - SKL_PW_CTL_IDX_PW_1 + SKL_PG1` (intel_display_power_well.c:45-50).
pub const fn power_gate(well: Well) -> Option<u32> {
    match well {
        Well::Pw1 => Some(1), // SKL_PG1: 14 - 14 + 1
        Well::Pw2 => Some(2), // SKL_PG2: 15 - 14 + 1
        _ => None,
    }
}
