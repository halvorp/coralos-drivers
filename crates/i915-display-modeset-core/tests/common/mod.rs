// SPDX-License-Identifier: GPL-2.0-only
//! Test harness: a register file that applies the effects of a `Body`, and a fixture that builds a real `SubPlans` from the clock and
//! link crates for the Pixelbook Go's 1080p60 eDP (pipe A -> transcoder EDP -> DDI A, 4 lanes, HBR, 24 bpp).
#![allow(dead_code)]
use i915_common_core::Reg;
use i915_display_clock_core::cdclk::{init_target, plan_set_cdclk, CdclkHw};
use i915_display_clock_core::dpll::{dp_hw_state, plan_ddi_clock_enable, plan_dpll0_edp, DpllSnapshot};
use i915_display_clock_core::{Domain, PowerWells, Step, Transaction};
use i915_display_link_core::backlight::BacklightState;
use i915_display_link_core::buf_trans::{get_buf_trans, Sku};
use i915_display_link_core::ddi::{DdiConfig, DdiLink};
use i915_display_link_core::plan::Effect;
use i915_display_modeset_core::enable::{EnableInputs, SubPlans};
use i915_display_modeset_core::plan::Body;
use i915_kbl_display_core::mode_write::SyncFlags;
use i915_kbl_display_core::skl_plane::*;
use i915_kbl_display_core::skl_wm::{adjust_latencies, latencies_from_pcode, DramFacts};
use i915_kbl_display_core::timing::ModeTimings;
use i915_kbl_display_core::types::{Ddi, Pipe};
use i915_vbt_core::backlight::Backlight;
use std::collections::BTreeMap;

/// A register file. Reads of an unset register return 0.
#[derive(Clone, Default)]
pub struct Sim {
    pub regs: BTreeMap<u32, u32>,
    pub writes: Vec<(u32, u32)>,
}

impl Sim {
    pub fn get(&self, r: u32) -> u32 {
        self.regs.get(&r).copied().unwrap_or(0)
    }
    pub fn put(&mut self, r: u32, v: u32) {
        self.regs.insert(r, v);
        self.writes.push((r, v));
    }
    pub fn effect(&mut self, e: &Effect) {
        match e {
            Effect::Write { reg, value } => self.put(reg.offset(), *value),
            Effect::Rmw { reg, clear, set } => {
                let v = (self.get(reg.offset()) & !clear) | set;
                self.put(reg.offset(), v)
            }
            Effect::ClearIfSet { reg, bits } => {
                let v = self.get(reg.offset());
                if v & bits != 0 {
                    self.put(reg.offset(), v & !bits)
                }
            }
            Effect::PostingRead { .. } | Effect::Poll(_) | Effect::DelayUs(_) => {}
        }
    }
    pub fn body(&mut self, b: &Body) {
        match b {
            Body::Empty(_) | Body::Interactive(_) => {}
            Body::Link(p) => p.iter().for_each(|e| self.effect(e)),
            Body::Clock(p) => {
                for s in p.iter() {
                    match s {
                        Step::Write { reg, value } => self.put(reg.offset(), *value),
                        Step::Rmw { reg, clear, set } => {
                            let v = (self.get(reg.offset()) & !clear) | set;
                            self.put(reg.offset(), v)
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}

pub fn cea_1080p60() -> ModeTimings {
    ModeTimings {
        hdisplay: 1920,
        htotal: 2200,
        hblank_start: 1920,
        hblank_end: 2200,
        hsync_start: 2008,
        hsync_end: 2052,
        vdisplay: 1080,
        vtotal: 1125,
        vblank_start: 1080,
        vblank_end: 1125,
        vsync_start: 1084,
        vsync_end: 1089,
        interlaced: false,
    }
}

pub fn plane_cfg() -> PlaneConfig {
    PlaneConfig {
        pipe: Pipe::A,
        plane_id: 0,
        fb: PlaneFb {
            format: WriteFormat::Xrgb8888,
            tiling: WriteTiling::Linear,
            stride_bytes: 7680,
            surf: 0x0040_0000,
            x: 0,
            y: 0,
            uv_stride_bytes: 0,
            uv_surf: 0,
            uv_x: 0,
            uv_y: 0,
        },
        src_w: 1920,
        src_h: 1080,
        dst_x: 0,
        dst_y: 0,
        dst_w: 1920,
        dst_h: 1080,
        rotation: WriteRotation::R0,
        blend: PixelBlend::None,
        yuv: YuvMatrix { bt709: false, full_range: false },
        plane_alpha: 0xff,
        pipe_gamma_enable: false,
        pipe_csc_enable: false,
    }
}

pub fn vbt_bl() -> Backlight {
    Backlight { present: true, entry_type: 2, control_type: Some(2), controller: 0, pwm_freq_hz: 200, active_low_pwm: false, min_brightness: 0, brightness_precision_bits: 0, hdr_dpcd_refresh_timeout: 30 }
}

/// The pre-transaction register snapshot: a firmware that left everything idle.
pub fn pre_state() -> BTreeMap<u32, u32> {
    BTreeMap::new()
}

pub struct Fixture {
    pub sub: SubPlans,
    pub pre: BTreeMap<u32, u32>,
}

pub fn fixture() -> Fixture {
    let pre = pre_state();
    let mut wells = PowerWells::new();
    wells.core_init().unwrap(); // PW1 / PW2 / MISC_IO are up from driver init, long before a modeset
    let mut power_crtc = wells.get(Domain::PipeA).unwrap();
    power_crtc.then(&wells.get(Domain::TranscoderEdp).unwrap()).unwrap();
    let power_aux = wells.get(Domain::AuxA).unwrap();
    let power_ddi_io = wells.get(Domain::DdiIoA).unwrap();
    let cdclk = plan_set_cdclk(&CdclkHw { vco_khz: 0, cdclk_ctl: 0, cdclk_khz: 0, voltage_level: 0 }, &init_target(0), false).unwrap();
    let hw = dp_hw_state(270_000).unwrap();
    let dpll = plan_dpll0_edp(&hw, 8_100_000, &DpllSnapshot { ctrl1: 0, cfgcr1: 0, cfgcr2: 0, ctl: 0 }).unwrap();
    let ddi_clock = plan_ddi_clock_enable(Ddi::A, 0, 0).unwrap();
    let cfg = DdiConfig {
        port: Ddi::A,
        lane_count: 4,
        lane_reversal: false,
        ddi_a_4_lanes: true,
        max_lanes: 4,
        vbt_dp_boost: 0,
        trans: get_buf_trans(Sku::KblY, true, false, Ddi::A),
        enhanced_framing: false,
    };
    let mut link = DdiLink::new(cfg);
    let ddi_buf_trans = link.prepare_dp_buffers();
    let signal_levels = link.set_signal_levels(&[0; 4]).unwrap();
    let stop_link_train = link.stop_link_train();
    let ddi_buf_disable = link.clone().buf_disable();
    let mut bl = BacklightState::setup(&vbt_bl(), 0, 0, 0).unwrap();
    let backlight_enable = bl.enable_plan(7500);
    let backlight_disable = bl.disable_plan();
    Fixture {
        sub: SubPlans {
            cdclk: Some(cdclk),
            power_crtc,
            power_aux,
            dpll,
            ddi_clock,
            power_ddi_io,
            ddi_buf_trans,
            signal_levels,
            stop_link_train,
            ddi_buf_disable,
            backlight_enable,
            backlight_disable,
        },
        pre,
    }
}

pub fn latencies() -> [u16; 8] {
    let mut l = latencies_from_pcode(0x120a_0402, 0x3c32_281e);
    adjust_latencies(&mut l, &DramFacts { symmetric_memory: false, has_16gb_dimms: false });
    l
}

pub fn inputs<'a>(f: &Fixture, snap: &'a dyn Fn(u32) -> u32) -> EnableInputs<'a> {
    EnableInputs {
        mode: cea_1080p60(),
        sync: SyncFlags { phsync: true, pvsync: true },
        pixel_clock_khz: 148_500,
        port_clock: 270_000,
        lanes: 4,
        pipe_bpp: 24,
        panel_native: (1920, 1080),
        plane: plane_cfg(),
        latencies: latencies(),
        ipc_enabled: false,
        cdclk_khz: 337_500,
        snap,
        sub: f.sub.clone(),
    }
}

pub fn regs_of(b: &Body) -> Vec<Reg> {
    b.written_regs()
}

pub fn tx_of(f: &Fixture) -> Transaction {
    f.sub.dpll.clone()
}
