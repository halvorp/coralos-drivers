// SPDX-License-Identifier: GPL-2.0-only
//! The Gen9 forcewake protocol: `fw_domains_get_with_fallback`, `fw_domains_put`, the `wait_ack_*` family, the
//! `WaRsForcewakeAddDelayForAck` fallback toggle, and the per-domain reference count of
//! `__intel_uncore_forcewake_get` / `__intel_uncore_forcewake_put` (intel_uncore.c:134-354, :663-680, :764-787).
//! Everything goes through `coralos_hal::Mmio`; the only registers written are [`crate::WRITE_ALLOWLIST`].
use crate::domain::{Domain, DomainId, GEN9_DOMAINS};
use crate::ranges::read_fw_domains;
use coralos_hal::Mmio;
use i915_common_core::reg::{masked_bit_disable, masked_bit_enable};
use i915_common_core::{I915Error, Reg};
use i915_devinfo_core::PlatformDescriptor;
use i915_gt_regs_core::{FORCEWAKE_KERNEL, FORCEWAKE_KERNEL_FALLBACK};

/// `FORCEWAKE_ACK_TIMEOUT_MS` (intel_uncore.c:42): how long an ack may take.
pub const FORCEWAKE_ACK_TIMEOUT_MS: u64 = 50; // intel_uncore.c:42

/// Iteration cap on one wait, for a host whose `Mmio` has no clock (`deadline_expired` is inert by default). Far above what 50 ms of
/// BAR0 reads can be, so on silicon the deadline fires first.
pub const MAX_POLLS: u32 = 1_000_000;

/// The fallback loop's pass limit: `while (!ack_detected && pass++ < 10)` (intel_uncore.c:248).
const FALLBACK_PASSES: u32 = 10;

enum Poll {
    Met,
    TimedOut { got: u32, polls: u32 },
}

/// The Gen9 uncore: which domains exist, how many references each holds, which are awake.
#[derive(Debug)]
pub struct Uncore {
    domains: &'static [Domain],
    mask: u32,
    wake_count: [u32; 3],
    active: u32,
    max_polls: u32,
}

impl Uncore {
    /// The Gen9 domains: RENDER, GT, MEDIA (intel_uncore.c:2210-2219).
    pub const fn gen9() -> Uncore {
        Uncore { domains: &GEN9_DOMAINS, mask: crate::domain::FORCEWAKE_ALL, wake_count: [0; 3], active: 0, max_polls: MAX_POLLS }
    }

    /// The uncore for `desc`, or the NAMED refusal if it is not a Gen9 part (a Cherryview id gets `UnsupportedGeneration`, never
    /// the Gen9 register set).
    pub fn for_platform(desc: &PlatformDescriptor) -> Result<Uncore, I915Error> {
        desc.require_gen9()?;
        Ok(Uncore::gen9())
    }

    /// An uncore over an arbitrary domain table. Exists so a test can drive the Valleyview / Cherryview domains against a Gen9
    /// script (the negative control); a device never calls it.
    pub fn with_domains(domains: &'static [Domain]) -> Uncore {
        let mask = domains.iter().fold(0, |m, d| m | d.mask);
        Uncore { domains, mask, wake_count: [0; 3], active: 0, max_polls: MAX_POLLS }
    }

    /// Lower the per-wait iteration cap (for hosts and tests; the deadline is the real bound).
    pub fn set_poll_limit(&mut self, polls: u32) {
        self.max_polls = polls.max(1);
    }

    /// `uncore->fw_domains`.
    pub const fn domains(&self) -> u32 {
        self.mask
    }
    /// `uncore->fw_domains_active`.
    pub const fn active(&self) -> u32 {
        self.active
    }
    /// `domain->wake_count`.
    pub fn wake_count(&self, id: DomainId) -> u32 {
        self.wake_count[id as usize]
    }

    fn each<'a>(&'a self, mask: u32) -> impl Iterator<Item = &'a Domain> + 'a {
        self.domains.iter().filter(move |d| d.mask & mask != 0)
    }

    fn check_domains(&self, want: u32) -> Result<(), I915Error> {
        let bad = want & !self.mask;
        if bad != 0 {
            return Err(I915Error::NoSuchDomain { domain_bit: bad & bad.wrapping_neg() });
        }
        Ok(())
    }

    // ---- the ack waits ---------------------------------------------------------------------------------------------

    /// `__wait_for_ack`: poll `(fw_ack(d) & ack) == value`, bounded by the 50 ms deadline and `max_polls`. A read of all-ones is
    /// `MmioUnreliable` at once (ALL_ONES_IS_NOT_AN_ACK).
    fn poll<M: Mmio>(&self, m: &mut M, d: &Domain, ack: u32, value: u32) -> Result<Poll, I915Error> {
        m.arm_deadline(FORCEWAKE_ACK_TIMEOUT_MS);
        let mut polls = 0u32;
        loop {
            let got = m.r32(d.ack.offset());
            polls += 1;
            if got == u32::MAX {
                return Err(I915Error::MmioUnreliable { domain: d.mask, reg: d.ack });
            }
            if got & ack == value {
                return Ok(Poll::Met);
            }
            if m.deadline_expired() || polls >= self.max_polls {
                return Ok(Poll::TimedOut { got, polls });
            }
            m.relax();
        }
    }

    fn timeout(d: &Domain, ack: u32, value: u32, got: u32, polls: u32) -> I915Error {
        I915Error::AckTimeout { domain: d.mask, reg: d.ack, mask: ack, want: value, got, polls }
    }

    /// `fw_domain_wait_ack_clear` / `fw_domain_wait_ack_set`, with the failure returned instead of logged
    /// (intel_uncore.c:188-206, :276-285).
    fn wait_final<M: Mmio>(&self, m: &mut M, d: &Domain, value: u32) -> Result<(), I915Error> {
        match self.poll(m, d, FORCEWAKE_KERNEL, value)? {
            Poll::Met => Ok(()),
            Poll::TimedOut { got, polls } => Err(Self::timeout(d, FORCEWAKE_KERNEL, value, got, polls)),
        }
    }

    /// `fw_domain_wait_ack_with_fallback` (intel_uncore.c:213-258): kick the GT state machine by toggling the FALLBACK bit until the
    /// KERNEL ack shows. Returns whether the ack was detected.
    fn ack_with_fallback<M: Mmio>(&self, m: &mut M, d: &Domain, set: bool) -> Result<bool, I915Error> {
        let value = if set { FORCEWAKE_KERNEL } else { 0 }; // intel_uncore.c:218
        let mut pass = 1u32; // :236
        loop {
            self.poll(m, d, FORCEWAKE_KERNEL_FALLBACK, 0)?; // :238 (result ignored, as in Linux)
            m.w32(d.set.offset(), masked_bit_enable(FORCEWAKE_KERNEL_FALLBACK)); // :240 fw_set
            // udelay(10 * pass) (:242), UDELAY_BY_READS
            for _ in 0..10 * pass {
                let _ = m.r32(d.ack.offset());
            }
            self.poll(m, d, FORCEWAKE_KERNEL_FALLBACK, FORCEWAKE_KERNEL_FALLBACK)?; // :243
            let detected = m.r32(d.ack.offset()) & FORCEWAKE_KERNEL == value; // :245
            m.w32(d.set.offset(), masked_bit_disable(FORCEWAKE_KERNEL_FALLBACK)); // :247 fw_clear
            let again = !detected && pass < FALLBACK_PASSES; // :248 `!ack_detected && pass++ < 10`
            pass += 1;
            if !again {
                return Ok(detected);
            }
        }
    }

    /// `fw_domain_wait_ack_clear_fallback` (intel_uncore.c:260-268).
    fn wait_clear_fallback<M: Mmio>(&self, m: &mut M, d: &Domain) -> Result<(), I915Error> {
        if let Poll::Met = self.poll(m, d, FORCEWAKE_KERNEL, 0)? {
            return Ok(()); // :263
        }
        if !self.ack_with_fallback(m, d, false)? {
            return self.wait_final(m, d, 0); // :266-267
        }
        Ok(())
    }

    /// `fw_domain_wait_ack_set_fallback` (intel_uncore.c:287-295).
    fn wait_set_fallback<M: Mmio>(&self, m: &mut M, d: &Domain) -> Result<(), I915Error> {
        if let Poll::Met = self.poll(m, d, FORCEWAKE_KERNEL, FORCEWAKE_KERNEL)? {
            return Ok(()); // :290
        }
        if !self.ack_with_fallback(m, d, true)? {
            return self.wait_final(m, d, FORCEWAKE_KERNEL); // :293-294
        }
        Ok(())
    }

    // ---- get / put -------------------------------------------------------------------------------------------------

    /// `fw_domains_get_with_fallback` (intel_uncore.c:322-340): for each domain wait for a clear ack and set KERNEL; then for each
    /// wait for the set ack.
    fn hw_get<M: Mmio>(&mut self, m: &mut M, need: u32) -> Result<(), I915Error> {
        for d in self.each(need) {
            self.wait_clear_fallback(m, d)?; // :332
            m.w32(d.set.offset(), masked_bit_enable(FORCEWAKE_KERNEL)); // :333 fw_domain_get = fw_set(d, KERNEL), :270-274
        }
        for d in self.each(need) {
            self.wait_set_fallback(m, d)?; // :337
        }
        self.active |= need; // :339
        Ok(())
    }

    /// `intel_uncore_forcewake_get` (intel_uncore.c:663-680): take a reference on each domain in `want`; the hardware is
    /// only asked for those that had none. On error the references taken by this call are dropped and nothing further is written.
    pub fn forcewake_get<M: Mmio>(&mut self, m: &mut M, want: u32) -> Result<(), I915Error> {
        self.check_domains(want)?;
        let mut need = 0;
        for d in self.domains {
            if d.mask & want != 0 {
                let c = &mut self.wake_count[d.id as usize];
                if *c == 0 {
                    need |= d.mask; // :671-676: only a first reference reaches the hardware
                }
                *c += 1;
            }
        }
        if need != 0 {
            if let Err(e) = self.hw_get(m, need) {
                for d in self.domains {
                    if d.mask & want != 0 {
                        self.wake_count[d.id as usize] -= 1;
                    }
                }
                return Err(e);
            }
        }
        Ok(())
    }

    /// `intel_uncore_forcewake_put` (non-delayed; intel_uncore.c:764-787): drop a reference on each domain in `want`; the last
    /// reference clears the KERNEL bit (`fw_domains_put`, :342-354). A put without a get is `Unbalanced` and changes nothing
    /// (Linux: `GEM_BUG_ON(!domain->wake_count)`, :774).
    pub fn forcewake_put<M: Mmio>(&mut self, m: &mut M, want: u32) -> Result<(), I915Error> {
        self.check_domains(want)?;
        for d in self.each(want) {
            if self.wake_count[d.id as usize] == 0 {
                return Err(I915Error::Unbalanced { domain_bit: d.mask });
            }
        }
        let mut release = 0;
        for d in self.domains {
            if d.mask & want != 0 {
                let c = &mut self.wake_count[d.id as usize];
                *c -= 1;
                if *c == 0 {
                    release |= d.mask; // :776-785
                }
            }
        }
        for d in self.each(release) {
            m.w32(d.set.offset(), masked_bit_disable(FORCEWAKE_KERNEL)); // :351 fw_domain_put = fw_clear(d, KERNEL), :297-301
        }
        self.active &= !release; // :353
        Ok(())
    }

    /// Wait for each domain in `mask` to show a CLEAR ack: the release has landed. Linux does this at the start of the next get
    /// (`fw_domain_wait_ack_clear`); the proof of life asks for it up front so "released" is observed, not assumed.
    pub fn wait_released<M: Mmio>(&self, m: &mut M, mask: u32) -> Result<(), I915Error> {
        self.check_domains(mask)?;
        for d in self.each(mask) {
            self.wait_final(m, d, 0)?;
        }
        Ok(())
    }

    /// A 32-bit read of `reg` with the forcewake it needs (`fwtable_read32`, intel_uncore.c:1912-1922): the domains the Gen9 table
    /// names for the offset must already be held (DELAYED_PUT: no auto-forcewake timer here), else `ForcewakeNotHeld`.
    pub fn read32<M: Mmio>(&self, m: &mut M, reg: Reg) -> Result<u32, I915Error> {
        let need = read_fw_domains(reg.offset()) & self.mask;
        let held = self.domains.iter().filter(|d| self.wake_count[d.id as usize] > 0).fold(0, |a, d| a | d.mask);
        if need & !held != 0 {
            return Err(I915Error::ForcewakeNotHeld { offset: reg.offset(), domains: need & !held });
        }
        Ok(m.r32(reg.offset()))
    }
}
