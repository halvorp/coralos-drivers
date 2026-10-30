// SPDX-License-Identifier: GPL-2.0-only
//! The GT's software frequency request, executed from the service's poll. `i915_gt_core::rps::Waitboost` DECIDES (request RP0 while
//! submitted work is in flight, RPn after `BOOST_IDLE_MS` with nothing in flight); this file reads what the firmware left, executes the
//! decisions through the executor (write allow-list + forcewake) and logs. Linux reaches RP0 through its up/down threshold interrupts
//! (gt/intel_rps.c); no GPU interrupt is serviced by this device yet, so the policy is the interrupt-less subset, driven by submissions
//! and the one idle deadline the policy asks for.
//!
//! Markers (all `[I915-RPS ...]` / `[I915-GT-FREQ ...]`):
//!  * `[I915-RPS ENABLE platform= rp0_mhz= rp1_mhz= rpn_mhz= was_rp_control= was_rpnswreq= was_cagf_mhz= rp_control= rpnswreq=
//!    confirm=OK|MISMATCH cagf_mhz_after=]` once at bring-up: the firmware's P-state (BEFORE any write), the two registers read back
//!    after the write, and the first frequency sample after the PCU was given the software request.
//!  * `[I915-RPS REFUSED why=...]`: the platform's path is not ported, the caps are unusable, or a write was refused. RPS then stays
//!    off for the life of the task - a refused write is never retried on every poll.
//!  * `[I915-GT-FREQ cagf_mhz= req_mhz= rpstat1= busy= boosts= decays=]`: a sample of the actual frequency, at most every
//!    `FREQ_LOG_MS` while work is in flight - never at idle.
use super::service::Hw;
use super::UapiService;
use crate::exec::dma::CpuMem;
use crate::exec::{ExecError, Executor};
use crate::log::{emit, Log};
use coralos_hal::{Dma, Mmio};
use i915_common_core::Reg;
use i915_devinfo_core::rps::RpsLayout;
use i915_devinfo_core::PlatformDescriptor;
use i915_gt_core::rps::{Waitboost, Why};
use i915_gt_regs_core::{GEN6_RPNSWREQ, GEN6_RPSTAT1, GEN6_RP_CONTROL};

/// Read one register with the forcewake its read needs held around it.
fn read_awake<B: Mmio>(ex: &mut Executor<B>, reg: Reg) -> Result<u32, ExecError> {
    let d = ex.domains_for([reg]);
    ex.awake(d, |ex| ex.rd(reg))
}

impl UapiService {
    /// Put the GT's frequency under the software request, at RPn, and say what the firmware had left. Called once, after bring-up.
    pub fn enable_rps<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>, desc: &PlatformDescriptor, rp_state_cap: u32) {
        let Some(layout) = RpsLayout::for_platform(desc) else {
            emit(hw.log, format_args!("[I915-RPS REFUSED why=PLATFORM-NOT-PORTED platform={}] - the GT keeps the firmware's clock", desc.name()));
            return;
        };
        let Some(mut wb) = Waitboost::new(layout, rp_state_cap) else {
            emit(hw.log, format_args!("[I915-RPS REFUSED why=CAPS-UNUSABLE rp_state_cap={:#x}] - the GT keeps the firmware's clock", rp_state_cap));
            return;
        };
        // what the firmware left, read BEFORE the first write (the "before" of the measurement)
        let was = (read_awake(hw.ex, GEN6_RP_CONTROL), read_awake(hw.ex, GEN6_RPNSWREQ), read_awake(hw.ex, GEN6_RPSTAT1));
        let (Ok(was_ctl), Ok(was_req), Ok(was_stat)) = was else {
            emit(hw.log, format_args!("[I915-RPS REFUSED why=READ-REFUSED] - the GT keeps the firmware's clock"));
            return;
        };
        let plan = wb.enable_plan(hw.now_ms);
        if let Err(e) = hw.ex.run_gt_effects(&plan) {
            emit(hw.log, format_args!("[I915-RPS REFUSED why={} step=ENABLE] - the GT keeps the firmware's clock", e.name()));
            return;
        }
        // confirm: the two registers read back as written; a refused read ends RPS by name, a different value is reported as MISMATCH
        // (the request stays: the PCU owns how it grants it, and the frequency samples show the outcome)
        let (Ok(ctl), Ok(req), Ok(stat)) = (read_awake(hw.ex, GEN6_RP_CONTROL), read_awake(hw.ex, GEN6_RPNSWREQ), read_awake(hw.ex, GEN6_RPSTAT1)) else {
            emit(hw.log, format_args!("[I915-RPS REFUSED why=READ-REFUSED step=CONFIRM] - the request was written but cannot be confirmed; RPS off"));
            return;
        };
        let want = match plan {
            [i915_gt_core::Effect::Write(a), i915_gt_core::Effect::Write(b)] => (a.value, b.value),
            _ => (u32::MAX, u32::MAX),
        };
        let confirm = if (ctl, req) == want { "OK" } else { "MISMATCH" };
        let c = wb.caps();
        emit(
            hw.log,
            format_args!(
                "[I915-RPS ENABLE platform={} rp0_mhz={} rp1_mhz={} rpn_mhz={} was_rp_control={:#x} was_rpnswreq={:#x} was_cagf_mhz={} rp_control={:#x} rpnswreq={:#x} confirm={} cagf_mhz_after={}] policy: RP0 while work is in flight, RPn after {} ms idle",
                desc.name(),
                layout.mhz(c.rp0),
                layout.mhz(c.rp1),
                layout.mhz(c.rpn),
                was_ctl,
                was_req,
                layout.mhz(layout.cagf(was_stat)),
                ctl,
                req,
                confirm,
                layout.mhz(layout.cagf(stat)),
                i915_gt_core::rps::BOOST_IDLE_MS
            ),
        );
        self.rps = Some(wb);
    }

    /// After every poll: boost or decay the request, and sample the actual frequency while busy.
    pub(crate) fn rps_tick<B: Mmio, D: Dma, M: CpuMem, L: Log>(&mut self, hw: &mut Hw<'_, B, D, M, L>) {
        let busy = self.rig.next_stall_check(hw.now_ms).is_some();
        let Some(wb) = self.rps.as_mut() else { return };
        if let Some((e, why)) = wb.tick(hw.now_ms, busy) {
            if let Err(err) = hw.ex.run_gt_effects(&[e]) {
                let step = match why {
                    Why::Boost => "BOOST",
                    Why::Idle => "IDLE",
                };
                emit(hw.log, format_args!("[I915-RPS REFUSED why={} step={}] - RPS off for the life of the task", err.name(), step));
                self.rps = None;
                return;
            }
        }
        if wb.sample_due(hw.now_ms, busy) {
            let layout = wb.layout();
            let req = wb.requested().map_or(0, |v| layout.mhz(v));
            let (boosts, decays) = (wb.boosts, wb.decays);
            match read_awake(hw.ex, GEN6_RPSTAT1) {
                Ok(v) => emit(
                    hw.log,
                    format_args!("[I915-GT-FREQ cagf_mhz={} req_mhz={} rpstat1={:#x} busy={} boosts={} decays={}]", layout.mhz(layout.cagf(v)), req, v, busy as u8, boosts, decays),
                ),
                Err(e) => emit(hw.log, format_args!("[I915-GT-FREQ read refused why={}]", e.name())),
            }
        }
    }

    /// The policy's one deadline (the decay to RPn), if a boost is standing.
    pub(crate) fn rps_deadline(&self) -> Option<u64> {
        self.rps.as_ref().and_then(|w| w.next_deadline())
    }

    /// The request the policy last made (request units), for the tests.
    pub fn rps_requested(&self) -> Option<u32> {
        self.rps.as_ref().and_then(|w| w.requested())
    }
}
