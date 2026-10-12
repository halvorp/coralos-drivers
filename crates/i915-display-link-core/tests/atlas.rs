// SPDX-License-Identifier: GPL-2.0-only
//! End to end on the Pixelbook Go's real VBT: VBT facts -> buffer table / PPS / backlight, then the whole AUX -> DPCD ->
//! capability -> link-training stack against a scripted eDP 1.4 panel. The VBT is the file i915-vbt-core's tests parse
//! (`I915_ATLAS_VBT` or coral_handoffs/atlas/sources/atlas-vbt.bin); the tests SKIP when it is absent.
//! The panel's DPCD is the representative eDP 1.4 literal documented in tests/caps.rs (the real capture is pending).
#![allow(unused_imports)]
mod common;
use common::*;
use i915_devinfo_core::lookup;
use i915_display_link_core::aux::*;
use i915_display_link_core::backlight::*;
use i915_display_link_core::buf_trans::*;
use i915_display_link_core::caps::*;
use i915_display_link_core::ddi::*;
use i915_display_link_core::dpcd::*;
use i915_display_link_core::dpdefs::*;
use i915_display_link_core::plan::Effect;
use i915_display_link_core::pps::*;
use i915_display_link_core::regs::*;
use i915_display_link_core::train::*;
use i915_display_link_core::vbt_link::*;
use i915_kbl_display_core::regs::{DDI_A_4_LANES, DDI_BUF_CTL_ENABLE};
use i915_kbl_display_core::Ddi;
use i915_vbt_core::child::AuxCh;
use i915_vbt_core::general::parse_general_definitions;
use i915_vbt_core::panel::{select_panel_type, vbt_panel_type, PanelTypeSources};
use i915_vbt_core::Vbt;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

fn load() -> Option<Vec<u8>> {
    let p = std::env::var("I915_ATLAS_VBT").unwrap_or_else(|_| "/home/peter/Projects/coral_handoffs/atlas/sources/atlas-vbt.bin".into());
    std::fs::read(p).ok()
}

const EDP14_DPCD: [u8; 15] = [0x14, 0x14, 0x84, 0x01, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const EDP14_EDP: [u8; 5] = [0x03, 0x05, 0x00, 0x00, 0x00];
const EDP14_RATES: [u8; 16] = [0xA4, 0x1F, 0x30, 0x2A, 0xBC, 0x34, 0x48, 0x3F, 0x60, 0x54, 0x78, 0x69, 0, 0, 0, 0];

/// A panel behind a DP AUX channel: it decodes the header the CPU loaded into the data registers and answers from a DPCD memory.
struct PanelSink {
    mem: HashMap<u32, u8>,
    /// Link training behaviour: CR needs `cr_after` status reads, EQ `eq_after`.
    cr_reads: usize,
    eq_reads: usize,
    cr_after: usize,
    eq_after: usize,
    pattern: u8,
    log: Vec<String>,
}

impl PanelSink {
    fn new() -> PanelSink {
        let mut mem = HashMap::new();
        for (i, b) in EDP14_DPCD.iter().enumerate() {
            mem.insert(i as u32, *b);
        }
        for (i, b) in EDP14_EDP.iter().enumerate() {
            mem.insert(0x700 + i as u32, *b);
        }
        for (i, b) in EDP14_RATES.iter().enumerate() {
            mem.insert(0x10 + i as u32, *b);
        }
        PanelSink { mem, cr_reads: 0, eq_reads: 0, cr_after: 3, eq_after: 2, pattern: 0, log: vec![] }
    }

    fn handle(&mut self, sent: &[u8]) -> (u32, Vec<u8>) {
        let req = sent[0] >> 4;
        let addr = ((sent[0] as u32 & 0xf) << 16) | ((sent[1] as u32) << 8) | sent[2] as u32;
        let size = sent[3] as usize + 1;
        if req == DP_AUX_NATIVE_READ {
            let mut out = vec![0x00]; // native ACK
            if addr == DP_LANE0_1_STATUS {
                out.extend_from_slice(&self.status_and_count()[..size]);
            } else {
                for i in 0..size {
                    out.push(self.mem.get(&(addr + i as u32)).copied().unwrap_or(0));
                }
            }
            (DP_AUX_CH_CTL_DONE, out)
        } else {
            let data = &sent[4..];
            self.log.push(format!("W {addr:#x} {data:02x?}"));
            if addr == DP_TRAINING_PATTERN_SET {
                self.pattern = data[0] & 0x0f;
            }
            for (i, b) in data.iter().enumerate() {
                self.mem.insert(addr + i as u32, *b);
            }
            (DP_AUX_CH_CTL_DONE, vec![0x00])
        }
    }

    /// The 6 status bytes for the current phase; counts the read.
    fn status_and_count(&mut self) -> [u8; 6] {
        if self.pattern == DP_TRAINING_PATTERN_1 {
            self.cr_reads += 1;
            if self.cr_reads >= self.cr_after {
                [0x11, 0x11, 0, 0, 0, 0]
            } else {
                // not locked yet: ask for one more step of voltage swing each time
                let v = self.cr_reads as u8; // 1, 2, ...
                [0, 0, 0, 0, v | (v << 4), v | (v << 4)]
            }
        } else {
            self.eq_reads += 1;
            if self.eq_reads >= self.eq_after {
                [0x77, 0x77, 0x01, 0, 0, 0]
            } else {
                [0x11, 0x11, 0, 0, 0x2a, 0x2a] // CR holds, EQ pending, ask (v2, p1)
            }
        }
    }
}

#[test]
fn atlas_vbt_feeds_the_link_planners() {
    let Some(buf) = load() else {
        eprintln!("SKIP: no Atlas VBT on this host");
        return;
    };
    let vbt = Vbt::parse(&buf).unwrap();
    let gd = parse_general_definitions(&vbt).unwrap();
    let c0 = &gd.children[0];
    let pt = select_panel_type(PanelTypeSources { vbt: vbt_panel_type(&vbt, Some(c0)).unwrap(), ..Default::default() }, false).unwrap();
    assert_eq!(pt, 2);
    let v = edp_inputs(&vbt, pt).unwrap();
    assert_eq!(v.aux_ch, AuxCh::A); // child aux channel 0x40
    assert_eq!(v.dp_boost, 0); // the VBT asks for no DP I_boost
    assert!(!v.lane_reversal);
    assert!(v.edp_low_vswing); // "edp_vswing_preemph bit 0"
    assert_eq!(v.pps, PpsDelays { power_up: 2000, backlight_on: 10, backlight_off: 2000, power_down: 500, power_cycle: 5000 });
    let bl = v.backlight.unwrap();
    assert_eq!((bl.present, bl.pwm_freq_hz), (true, 200));

    // buffer translation: Pixelbook Go = KBL ULX = Y; eDP + low vswing = skl_y_trans_edp, ten rows on DDI A
    let sku = Sku::from_descriptor(&lookup(0x591c).unwrap()).unwrap();
    assert_eq!(sku, Sku::KblY);
    let cfg = DdiConfig::edp_ddi_a(sku, &v, DDI_A_4_LANES, 4, false);
    assert_eq!(cfg.trans.entries.as_ptr(), SKL_Y_TRANS_EDP.as_ptr());
    assert_eq!(cfg.trans.n_entries, 10);
    assert!(cfg.ddi_a_4_lanes && cfg.max_lanes == 4);
    // a strap that leaves DDI A at two lanes
    let two = DdiConfig::edp_ddi_a(sku, &v, 0, 2, false);
    assert!(!two.ddi_a_4_lanes && two.max_lanes == 2);

    // panel power sequencer + backlight from the same VBT
    let t = PpsTiming::compute(PpsDelays::default(), v.pps);
    assert_eq!(t.panel_power_up_delay_ms, 200);
    assert_eq!(t.panel_power_cycle_delay_ms, 500);
    let s = BacklightState::setup(&bl, 0, 0, 0).unwrap();
    assert_eq!(s.pwm_level_max, 7500);
}

#[test]
fn a_vbt_without_an_edp_child_on_ddi_a_is_refused_by_name() {
    let Some(mut buf) = load() else {
        eprintln!("SKIP: no Atlas VBT on this host");
        return;
    };
    // turn the eDP child (device type 0x1806 at its handle 0x0008) into a plain DP one: the INTERNAL_CONNECTOR bit (1 << 12) off.
    // Find the 16-bit handle/type pair "08 00 06 18" in the child table and clear bit 12 of the type.
    let pos = buf.windows(4).position(|w| w == [0x08, 0x00, 0x06, 0x18]).expect("eDP child in the VBT");
    buf[pos + 3] &= !0x10;
    let vbt = Vbt::parse(&buf).unwrap();
    let e = edp_inputs(&vbt, 2).unwrap_err();
    assert_eq!(e, VbtLinkError::NoEdpChildOnDdiA);
    assert_eq!(e.name(), "VBT-LINK-NO-EDP-CHILD-ON-DDI-A");
}

/// Drive a DPCD request through the real AUX machine on a scripted panel; returns the result.
fn dpcd(hw: &mut AuxHw, req: DpcdRequest) -> Result<DpcdOk, DpcdFail> {
    run_dpcd(hw, &mut DpcdXfer::new(AuxCh::A, req))
}

#[test]
fn discover_and_train_the_panel_through_aux_dpcd_and_the_trainer() {
    let Some(buf) = load() else {
        eprintln!("SKIP: no Atlas VBT on this host");
        return;
    };
    let vbt = Vbt::parse(&buf).unwrap();
    let v = edp_inputs(&vbt, 2).unwrap();
    let panel = Rc::new(RefCell::new(PanelSink::new()));
    let p2 = panel.clone();
    let mut hw = AuxHw::new(v.aux_ch, move |_, sent| p2.borrow_mut().handle(sent));

    // 1. capabilities, every byte through AuxXfer -> DpcdXfer -> CapsProbe
    let mut probe = CapsProbe::new();
    let mut reply = None;
    let caps = loop {
        match probe.step(reply.take()) {
            ProbeStep::Done(r) => break r.unwrap(),
            ProbeStep::Request(req) => reply = Some(dpcd(&mut hw, req)),
        }
    };
    assert_eq!(caps.sink_rates.as_slice(), &[162_000, 216_000, 270_000, 324_000, 432_000, 540_000]);
    assert!(caps.use_rate_select);

    // 2. link parameters: the best the source, the VBT and the sink agree on
    let vbt_max = vbt_max_link_rate(v.child_max_link_rate, v.edp_max_link_rate);
    let src = source_rates(vbt_max);
    let common = common_rates(&src, &caps.sink_rates);
    let rate = *common.as_slice().last().unwrap();
    assert_eq!(rate, 540_000);
    let sku = Sku::from_descriptor(&lookup(0x591c).unwrap()).unwrap();
    let buf_ctl_a = DDI_A_4_LANES;
    let cfg = DdiConfig::edp_ddi_a(sku, &v, buf_ctl_a, 4, dp_enhanced_frame_cap(&caps.dpcd));
    let lanes = max_common_lane_count(cfg.max_lanes, v.child_max_lanes, caps.max_sink_lane_count);
    assert_eq!(lanes, 4);
    assert!(cfg.enhanced_framing); // the literal sink advertises enhanced framing

    // 3. train, DPCD traffic through the AUX machine, DDI effects collected
    let mut t = LinkTrainer::new(LinkParams { port_clock: rate, lane_count: lanes, enhanced_framing: cfg.enhanced_framing, msa_timing_par_ignore: false }, caps, DdiLink::new(cfg)).unwrap();
    let mut mmio: Vec<Effect> = vec![];
    let mut reply = TrainReply::Start;
    let outcome = loop {
        match t.step(reply) {
            TrainAction::Finished(o) => break o,
            TrainAction::Mmio(p) => {
                mmio.extend(p);
                reply = TrainReply::Done;
            }
            TrainAction::DelayUs(_) => reply = TrainReply::Done,
            TrainAction::Dpcd(req) => reply = TrainReply::Dpcd(dpcd(&mut hw, req)),
        }
    };
    let ok = outcome.expect("the scripted panel trains");
    assert_eq!((ok.cr_reads, ok.eq_reads), (3, 2));
    // the panel saw the eDP 1.4 rate-select programming and a clean pattern shutdown
    let log = panel.borrow().log.clone();
    assert!(log.contains(&"W 0x101 [84]".to_string()), "{log:?}"); // 4 lanes + enhanced framing
    assert!(log.contains(&"W 0x115 [05]".to_string()), "{log:?}"); // rate index 5 = 5.4 Gbps
    assert_eq!(log.last().unwrap(), "W 0x102 [00]");
    // DDI-A is up: buffer enabled, the 10-row eDP table was used for the signal levels
    assert!(t.ddi().dp & DDI_BUF_CTL_ENABLE != 0);
    let writes: Vec<u32> = i915_display_link_core::plan::written_regs(&mmio).iter().map(|r| r.offset()).collect();
    assert!(writes.iter().all(|o| [0x64000, 0x64040, 0x6C00C].contains(o)), "{writes:x?}");
}
