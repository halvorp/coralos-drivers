// SPDX-License-Identifier: GPL-2.0-only
//! Display power wells as the BIOS left them (read-only).
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display_power_well.c (`hsw_power_well_enabled`,
//! `hsw_power_well_regs`, `gen9_wait_for_power_well_fuses`), intel_display_power_map.c (`skl_power_wells_*`,
//! `SKL_PW_2_POWER_DOMAINS`) and intel_display_regs.h (HSW_PWR_WELL_CTL*, SKL_FUSE_STATUS, DC_STATE_EN).
//! Copyright (c) Intel Corporation.
//!
//! Which registers are safe to read depends on which well powers them: Linux wraps every display register read in
//! `intel_display_power_get_if_enabled(domain)` and treats an unpowered domain as "nothing there". The readout does the
//! same through [`PowerState::pipe_powered`] and friends, because a read inside a gated well returns garbage.

use crate::regs::*;
use crate::types::{Ddi, Pipe, Transcoder};

/// The Skylake-class wells with a request/state pair in HSW_PWR_WELL_CTL (intel_display_power_map.c:355-401).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Well {
    Pw1,
    Pw2,
    MiscIo,
    DdiAE,
    DdiB,
    DdiC,
    DdiD,
}

impl Well {
    pub const ALL: [Well; 7] = [Well::Pw1, Well::Pw2, Well::MiscIo, Well::DdiAE, Well::DdiB, Well::DdiC, Well::DdiD];
    pub const fn name(self) -> &'static str {
        match self {
            Well::Pw1 => "PW1",
            Well::Pw2 => "PW2",
            Well::MiscIo => "MISC_IO",
            Well::DdiAE => "DDI_A_E",
            Well::DdiB => "DDI_B",
            Well::DdiC => "DDI_C",
            Well::DdiD => "DDI_D",
        }
    }
    /// `.hsw.idx` of the well (intel_display_power_map.c:361, 375, 389, 398-401).
    pub const fn idx(self) -> u32 {
        match self {
            Well::Pw1 => SKL_PW_CTL_IDX_PW_1,
            Well::Pw2 => SKL_PW_CTL_IDX_PW_2,
            Well::MiscIo => SKL_PW_CTL_IDX_MISC_IO,
            Well::DdiAE => SKL_PW_CTL_IDX_DDI_A_E,
            Well::DdiB => SKL_PW_CTL_IDX_DDI_B,
            Well::DdiC => SKL_PW_CTL_IDX_DDI_C,
            Well::DdiD => SKL_PW_CTL_IDX_DDI_D,
        }
    }
    /// The well whose power gates a DDI's I/O.
    pub const fn for_ddi(d: Ddi) -> Well {
        match d {
            Ddi::A | Ddi::E => Well::DdiAE,
            Ddi::B => Well::DdiB,
            Ddi::C => Well::DdiC,
            Ddi::D => Well::DdiD,
        }
    }
}

/// `HSW_PWR_WELL_CTL_REQ(pw_idx)` = `0x2 << (pw_idx * 2)` (intel_display_regs.h:2129).
pub const fn well_req(idx: u32) -> u32 {
    0x2 << (idx * 2)
}
/// `HSW_PWR_WELL_CTL_STATE(pw_idx)` = `0x1 << (pw_idx * 2)` (intel_display_regs.h:2130).
pub const fn well_state(idx: u32) -> u32 {
    0x1 << (idx * 2)
}
/// `SKL_FUSE_PG_DIST_STATUS(pg)` = `1 << (27 - pg)` (intel_display_regs.h:2225).
pub const fn fuse_pg_dist_status(pg: u32) -> u32 {
    1 << (27 - pg)
}

/// One well's request and state bits in the BIOS and driver registers, and Linux's verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WellStatus {
    pub well: Well,
    /// HSW_PWR_WELL_CTL1 (the BIOS's request register): request bit / state bit.
    pub bios_req: bool,
    pub bios_state: bool,
    /// HSW_PWR_WELL_CTL2 (the driver's request register): request bit / state bit.
    pub drv_req: bool,
    pub drv_state: bool,
    /// `hsw_power_well_enabled`: request and state both set in the driver register; for PW1 and MISC_IO on gen 9
    /// the BIOS register's bits count too (the DMC-bug workaround, intel_display_power_well.c:634-636).
    pub enabled: bool,
}

/// `hsw_power_well_enabled` (intel_display_power_well.c:616-639) for a Gen9 (non-Broxton) well.
pub fn decode_well(well: Well, ctl_bios: u32, ctl_driver: u32) -> WellStatus {
    let idx = well.idx();
    let mask = well_req(idx) | well_state(idx); // intel_display_power_well.c:622-623
    let mut val = ctl_driver; // intel_display_power_well.c:626
    if matches!(well, Well::Pw1 | Well::MiscIo) {
        val |= ctl_bios; // intel_display_power_well.c:634-636
    }
    WellStatus {
        well,
        bios_req: ctl_bios & well_req(idx) != 0,
        bios_state: ctl_bios & well_state(idx) != 0,
        drv_req: ctl_driver & well_req(idx) != 0,
        drv_state: ctl_driver & well_state(idx) != 0,
        enabled: val & mask == mask, // intel_display_power_well.c:638
    }
}

/// DC_STATE_EN decoded (intel_display_regs.h:2820-2829): the DMC's deep display sleep states the firmware allows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DcState {
    /// No DC state allowed (`DC_STATE_EN` low bits 0).
    Off,
    UpToDc5,
    UpToDc6,
    /// The low two bits are 3 (undefined).
    Reserved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DcStateEn {
    pub raw: u32,
    pub state: DcState,
    pub dc9: bool,
}

pub fn decode_dc_state(raw: u32) -> DcStateEn {
    let state = match raw & DC_STATE_EN_UPTO_DC5_DC6_MASK {
        0 => DcState::Off,
        DC_STATE_EN_UPTO_DC5 => DcState::UpToDc5,
        DC_STATE_EN_UPTO_DC6 => DcState::UpToDc6,
        _ => DcState::Reserved,
    };
    DcStateEn { raw, state, dc9: raw & DC_STATE_EN_DC9 != 0 }
}

/// All wells plus the fuse and DC-state words.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerState {
    pub wells: [WellStatus; 7],
    /// SKL_FUSE_STATUS (intel_display_regs.h:2223): bit 31 = the power-gate fuses are downloaded; bits 27, 26, 25 =
    /// power gates 0, 1, 2 distributed.
    pub fuse_status: u32,
    pub dc_state: DcStateEn,
}

impl PowerState {
    pub fn well(&self, w: Well) -> &WellStatus {
        &self.wells[w as usize]
    }
    pub fn fuse_download_done(&self) -> bool {
        self.fuse_status & SKL_FUSE_DOWNLOAD_STATUS != 0
    }
    /// A pipe's register block is powered: pipe A, its panel fitter and TRANSCODER_EDP sit in the always-on domain
    /// (PW1); pipes B and C, their panel fitters and TRANSCODER_A/B/C are in PW2's domain list
    /// (`SKL_PW_2_POWER_DOMAINS`, intel_display_power_map.c:306-326); every domain is also in the always-on well
    /// (`i9xx_pwdoms_always_on = I915_PW_DOMAINS_ALL`, intel_display_power_map.c:50), so a domain is on iff the wells
    /// that list it are.
    pub fn pipe_powered(&self, p: Pipe) -> bool {
        match p {
            Pipe::A => true,
            Pipe::B | Pipe::C => self.well(Well::Pw2).enabled,
        }
    }
    pub fn transcoder_powered(&self, t: Transcoder) -> bool {
        match t {
            Transcoder::Edp => true,
            Transcoder::A | Transcoder::B | Transcoder::C => self.well(Well::Pw2).enabled,
        }
    }
    /// `encoder->power_domain` of a DDI is POWER_DOMAIN_PORT_DDI_LANES_x: port A's is in the always-on domain, B-E are
    /// in PW2's list (intel_display_power_map.c:314-317).
    pub fn ddi_lanes_powered(&self, d: Ddi) -> bool {
        match d {
            Ddi::A => true,
            _ => self.well(Well::Pw2).enabled,
        }
    }
}
