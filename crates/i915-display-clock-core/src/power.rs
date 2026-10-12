// SPDX-License-Identifier: GPL-2.0-only
//! Refcounted display power domains over the Skylake / Kaby Lake wells, and the request/state handshake that turns one on.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_display_power.c (`__intel_display_power_get_domain`,
//! `__intel_display_power_put_domain`, `skl_display_core_init`, `skl_display_core_uninit`) and intel_display_power_well.c
//! (`intel_power_well_get/put/enable/disable`, `hsw_power_well_enable/disable`, `hsw_wait_for_power_well_enable/disable`,
//! `gen9_wait_for_power_well_fuses`, `hsw_power_well_requesters`) at v7.0-rc6. Copyright (c) Intel Corporation.
//!
//! [`PowerWells`] is the software state Linux keeps in `struct i915_power_well` (`count`, `hw_enabled`) and
//! `domain_use_count`. A `get` / `put` returns a [`Transaction`]; the state change is committed ONLY when the whole plan was
//! built without refusal, and the executor reports back with [`PowerWells::rollback_applied`] / nothing if it all ran.
//!
//! Deliberate departures from Linux, each a strictness and none a loosening:
//!   * a timed-out ENABLE handshake aborts (Linux logs `power well enable timeout` and continues, intel_display_power_well.c:314-317);
//!   * a well may not turn on before the well it sits behind (see [`crate::power_map::requires`]);
//!   * a well may not turn off while one that sits behind it is on.
use crate::error::{ClockError, Handshake};
use crate::plan::{OnTimeout, PollPlan, Transaction, WritePlan};
use crate::power_map::{power_gate, requires, Domain, WELL_ORDER};
use crate::regs::*;
use i915_kbl_display_core::power::{fuse_pg_dist_status, well_req, well_state, PowerState, Well};

/// `hsw_wait_for_power_well_enable`'s `timeout = power_well->desc->enable_timeout ? : 1` (intel_display_power_well.c:299): no
/// SKL well sets `enable_timeout`, so 1 ms.
pub const WELL_ENABLE_TIMEOUT_MS: u32 = 1; // intel_display_power_well.c:299
/// `gen9_wait_for_power_well_fuses`: "Timeout 5us for PG#0, for other PGs 1us" - rounded up to the 1 ms the call takes
/// (intel_display_power_well.c:376-379).
pub const FUSE_WAIT_TIMEOUT_MS: u32 = 1; // intel_display_power_well.c:379
/// `hsw_wait_for_power_well_disable`: `reqs ? 0 : 1` ms (intel_display_power_well.c:357-359).
pub const WELL_DISABLE_TIMEOUT_MS: u32 = 1; // intel_display_power_well.c:359

/// KEPT FIXED (D10): the SKL power wells the hardware has (and the per-well/per-domain counters indexed by them).
const NWELLS: usize = 7;

/// Software state of the SKL wells and domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PowerWells {
    /// `power_well->count`, indexed by `Well as usize`.
    count: [u32; NWELLS],
    /// `power_well->hw_enabled`.
    hw_enabled: [bool; NWELLS],
    /// `domain_use_count[domain]`.
    domain_use: [u32; Domain::COUNT],
    /// Request bits other agents hold (BIOS / KVMR / DEBUG, `hsw_power_well_requesters` bits 1, 4, 8): with any set, the
    /// disable wait is 0 ms instead of 1 (intel_display_power_well.c:355-359).
    other_requesters: [u8; NWELLS],
}

impl PowerWells {
    /// All wells off, nothing referenced.
    pub const fn new() -> PowerWells {
        PowerWells { count: [0; NWELLS], hw_enabled: [false; NWELLS], domain_use: [0; Domain::COUNT], other_requesters: [0; NWELLS] }
    }

    /// The software state implied by a hardware readout (`intel_power_well_sync_hw`: `hw_enabled = is_enabled()`,
    /// intel_display_power_well.c:132-137). Counts start at zero: nothing in this crate holds a reference yet.
    pub fn from_readout(state: &PowerState) -> PowerWells {
        let mut w = PowerWells::new();
        for well in Well::ALL {
            w.hw_enabled[well as usize] = state.well(well).enabled;
            // bios/kvmr/debug request bits: the BIOS register's bit is the one the readout carries.
            if state.well(well).bios_req {
                w.other_requesters[well as usize] |= 1;
            }
        }
        w
    }

    pub fn is_enabled(&self, well: Well) -> bool {
        self.hw_enabled[well as usize]
    }
    pub fn well_count(&self, well: Well) -> u32 {
        self.count[well as usize]
    }
    pub fn domain_count(&self, d: Domain) -> u32 {
        self.domain_use[d as usize]
    }
    /// Record the requester bits (`hsw_power_well_requesters`, bit 1 = BIOS, 4 = KVMR, 8 = DEBUG) seen on a well.
    pub fn note_requesters(&mut self, well: Well, others: u8) {
        self.other_requesters[well as usize] = others & 0b1101;
    }

    /// `skl_display_core_init`'s well part (intel_display_power.c:1465-1472): enable PW1, then MISC_IO. Not refcounted - both
    /// are `always_on` wells whose enable is DMC-owned.
    pub fn core_init(&mut self) -> Result<Transaction, ClockError> {
        let mut t = Transaction::new();
        let mut next = *self;
        for well in [Well::Pw1, Well::MiscIo] {
            if !next.hw_enabled[well as usize] {
                t.then(&next.enable_well(well)?)?;
            }
        }
        *self = next;
        Ok(t)
    }

    /// `skl_display_core_uninit`'s well part (intel_display_power.c:1484-1512): drop the driver's request on PW1 (MISC_IO stays
    /// on: "BSpec says to keep the MISC IO power well enabled here"). Refused while a well behind PW1 is on.
    pub fn core_uninit(&mut self) -> Result<Transaction, ClockError> {
        let mut next = *self;
        let t = next.disable_well(Well::Pw1)?;
        *self = next;
        Ok(t)
    }

    /// `intel_display_power_get` (`__intel_display_power_get_domain`, intel_display_power.c:519-533): reference the domain and
    /// turn on every well that lists it, in table order.
    pub fn get(&mut self, d: Domain) -> Result<Transaction, ClockError> {
        let mut next = *self;
        let mut t = Transaction::new();
        for &well in d.wells() {
            let i = well as usize;
            // intel_power_well_get: `if (!power_well->count++) intel_power_well_enable()` (intel_display_power_well.c:139-144)
            let was = next.count[i];
            next.count[i] += 1;
            if was == 0 {
                t.then(&next.enable_well(well)?)?;
            }
        }
        next.domain_use[d as usize] += 1; // intel_display_power.c:532
        *self = next;
        Ok(t)
    }

    /// `intel_display_power_put` (`__intel_display_power_put_domain`, intel_display_power.c:605-627): drop the reference and turn
    /// off every well that no domain still needs, in REVERSE table order.
    pub fn put(&mut self, d: Domain) -> Result<Transaction, ClockError> {
        if self.domain_use[d as usize] == 0 {
            return Err(ClockError::UnbalancedPut { what: "domain" }); // intel_display_power.c:614-616
        }
        let mut next = *self;
        let mut t = Transaction::new();
        next.domain_use[d as usize] -= 1; // intel_display_power.c:623
        for &well in d.wells().iter().rev() {
            let i = well as usize;
            if next.count[i] == 0 {
                return Err(ClockError::UnbalancedPut { what: "power well" }); // intel_display_power_well.c:149-151
            }
            next.count[i] -= 1; // intel_power_well_put, intel_display_power_well.c:146-155
            if next.count[i] == 0 {
                t.then(&next.disable_well(well)?)?;
            }
        }
        *self = next;
        Ok(t)
    }

    /// The executor ran a transaction's ROLLBACK because its forward plan failed. The hardware is back where it started, so the
    /// software state must be too: pass the `PowerWells` value from BEFORE the `get` / `put` (it is `Copy` for exactly this).
    pub fn rollback_applied(&mut self, before: PowerWells) {
        *self = before;
    }

    /// `hsw_power_well_enable` (intel_display_power_well.c:382-423) + the `hw_enabled` bookkeeping of `intel_power_well_enable`.
    fn enable_well(&mut self, well: Well) -> Result<Transaction, ClockError> {
        if let Some(needs) = requires(well) {
            if !self.hw_enabled[needs as usize] {
                return Err(ClockError::WellOrder { well, needs });
            }
        }
        let idx = well.idx();
        let mut fwd = WritePlan::new();
        let gate = power_gate(well);
        // "For PW1 we have to wait both for the PW0/PG0 fuse state before enabling the power well and PW1/PG1's own fuse state
        // after the enabling." (intel_display_power_well.c:397-410)
        if gate == Some(1) {
            fwd.poll(fuse_poll(SKL_PG0))?;
        }
        fwd.rmw(HSW_PWR_WELL_CTL2, 0, well_req(idx))?; // intel_display_power_well.c:408
        fwd.poll(PollPlan::wait_set(
            Handshake::PowerWellEnable(well),
            i915_common_core::Reg::new(HSW_PWR_WELL_CTL2),
            well_state(idx),
            WELL_ENABLE_TIMEOUT_MS,
            OnTimeout::Fail,
        ))?; // hsw_wait_for_power_well_enable, intel_display_power_well.c:293-320
        if let Some(pg) = gate {
            fwd.poll(fuse_poll(pg))?; // intel_display_power_well.c:412-418
        }
        // Rollback: withdraw our request. Tolerated wait, because a well can legitimately stay up for another requester.
        let mut rb = WritePlan::new();
        rb.rmw(HSW_PWR_WELL_CTL2, well_req(idx), 0)?;
        self.hw_enabled[well as usize] = true; // intel_power_well_enable, intel_display_power_well.c:121
        Ok(Transaction { forward: fwd, rollback: rb })
    }

    /// `hsw_power_well_disable` (intel_display_power_well.c:425-436).
    fn disable_well(&mut self, well: Well) -> Result<Transaction, ClockError> {
        for &other in WELL_ORDER.iter() {
            if requires(other) == Some(well) && self.hw_enabled[other as usize] {
                return Err(ClockError::WellStillNeeded { well, held_by: other });
            }
        }
        let idx = well.idx();
        let others = self.other_requesters[well as usize];
        let mut fwd = WritePlan::new();
        fwd.rmw(HSW_PWR_WELL_CTL2, well_req(idx), 0)?; // intel_display_power_well.c:434
        fwd.poll(PollPlan::wait_clear(
            Handshake::PowerWellDisable(well),
            i915_common_core::Reg::new(HSW_PWR_WELL_CTL2),
            well_state(idx),
            if others != 0 { 0 } else { WELL_DISABLE_TIMEOUT_MS },
            OnTimeout::Tolerate,
        ))?; // hsw_wait_for_power_well_disable, intel_display_power_well.c:338-371
        let mut rb = WritePlan::new();
        rb.rmw(HSW_PWR_WELL_CTL2, 0, well_req(idx))?; // re-request: the disable did not stick
        rb.poll(PollPlan::wait_set(
            Handshake::PowerWellEnable(well),
            i915_common_core::Reg::new(HSW_PWR_WELL_CTL2),
            well_state(idx),
            WELL_ENABLE_TIMEOUT_MS,
            OnTimeout::Fail,
        ))?;
        self.hw_enabled[well as usize] = false; // intel_power_well_disable, intel_display_power_well.c:128
        Ok(Transaction { forward: fwd, rollback: rb })
    }
}

impl Default for PowerWells {
    fn default() -> Self {
        PowerWells::new()
    }
}

/// `gen9_wait_for_power_well_fuses(pg)`: `SKL_FUSE_STATUS & SKL_FUSE_PG_DIST_STATUS(pg)` within 1 ms (intel_display_power_well.c:373-380).
/// Linux `drm_WARN_ON`s a timeout and continues; here it aborts - powering a gate whose fuses have not been distributed is not
/// a state anything downstream can be trusted in.
const fn fuse_poll(pg: u32) -> PollPlan {
    PollPlan::wait_set(
        Handshake::FuseDistribution(pg),
        i915_common_core::Reg::new(SKL_FUSE_STATUS),
        fuse_pg_dist_status(pg),
        FUSE_WAIT_TIMEOUT_MS,
        OnTimeout::Fail,
    )
}
