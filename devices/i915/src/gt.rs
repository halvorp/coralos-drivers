// SPDX-License-Identifier: GPL-2.0-only
//! INCREMENT 1: the GPU's proof of life. Identify the part through `i915-devinfo-core`, take the three Gen9 forcewake domains one at
//! a time (each ack checked), read the fuse / clock / P-state registers Linux reads at GT init, release the domains and observe the
//! release, and print the three marker lines. The ONLY MMIO writes are the three forcewake SET registers: every register access
//! here goes through an `i915_common_core::WriteGuard` over `i915_uncore_core::WRITE_ALLOWLIST`, and the marker reports the guard's
//! counts. Any failure stops the device with a named `[I915-FORCEWAKE FAIL ...]` line and writes nothing further.
//!
//! CHERRYVIEW (Gen8, DECISIONS.md D8) takes its own arm, [`proof_of_life`] dispatching by row: the GT wake gate
//! (`exec::power_chv::allow_gt_wake`), the two VLV domains RENDER then MEDIA (intel_uncore.c:2220-2225), both wells reported up
//! (`vlv_wait_for_gt_wells`), the fuse `CHV_FUSE_GT` decoded by `cherryview_sseu_info_init`, and the fixed Gen8 timestamp clock
//! (`gen6_read_clock_frequency`, 12.5 MHz). Gen9's `RPM_CONFIG0` / `CTC_MODE` / `GEN6_RP_STATE_CAP` are not read: on Cherryview
//! the clock is fixed and the P-states live behind the PUNIT sideband (`chv_rps_init`, gt/intel_rps.c), which this device does not
//! drive. Its writes are [`crate::exec::allow::CHV_GT_REGS`]: the two VLV SET registers and, only when the firmware left it shut,
//! the GT wake gate.
//!
//! Registers read (offsets and fields in `i915-gt-regs-core`, each cited to Linux there): `GEN8_FUSE2` 0x9120, `GEN8_EU_DISABLE0..2`
//! 0x9134/0x9138/0x913c, `RPM_CONFIG0` 0xd00, `CTC_MODE` 0xa26c, `GEN6_RP_STATE_CAP` 0x145998. Decoded by `i915-devinfo-core`
//! (`gen9_sseu_info`, `gen9_clock_from_ctc`; ports of gt/intel_sseu.c and gt/intel_gt_clock_utils.c).
use crate::identity::PciIdentity;
use crate::log::{emit, Log};
use coralos_hal::Mmio;
use i915_common_core::{I915Error, Reg, WriteGuard};
use i915_devinfo_core::clock::{gen9_clock_from_ctc, reference_ts_freq, Gen9Clock};
use i915_devinfo_core::sseu::{gen9_sseu_info, SseuInfo};
use i915_devinfo_core::{lookup, PlatformDescriptor};
use i915_gt_regs_core as r;
use i915_uncore_core::{Uncore, FORCEWAKE_GT, FORCEWAKE_MEDIA, FORCEWAKE_RENDER, WRITE_ALLOWLIST};

/// PCI_VENDOR_ID_INTEL (include/linux/pci_ids.h:2688).
const VENDOR_INTEL: u16 = 0x8086;

/// The order the domains are taken and released in: GT, RENDER, MEDIA (plan KBL-I915-PORT-PLAN §4.2).
const ORDER: [u32; 3] = [FORCEWAKE_GT, FORCEWAKE_RENDER, FORCEWAKE_MEDIA];

/// `&mut M` as an `Mmio`, so the guard can borrow the caller's window instead of owning it.
pub struct ByRef<'a, M: Mmio>(pub &'a mut M);

impl<M: Mmio> Mmio for ByRef<'_, M> {
    fn r8(&mut self, reg: u32) -> u8 { self.0.r8(reg) }
    fn r16(&mut self, reg: u32) -> u16 { self.0.r16(reg) }
    fn r32(&mut self, reg: u32) -> u32 { self.0.r32(reg) }
    fn w8(&mut self, reg: u32, v: u8) { self.0.w8(reg, v) }
    fn w16(&mut self, reg: u32, v: u16) { self.0.w16(reg, v) }
    fn w32(&mut self, reg: u32, v: u32) { self.0.w32(reg, v) }
    fn relax(&mut self) { self.0.relax() }
    fn arm_deadline(&mut self, ms: u64) { self.0.arm_deadline(ms) }
    fn deadline_expired(&mut self) -> bool { self.0.deadline_expired() }
    fn park_ms(&mut self, ms: u64) { self.0.park_ms(ms) }
}

/// What a successful proof of life learned. On Cherryview `fuse2` holds `CHV_FUSE_GT` and the Gen9-only words
/// (`eu_disable`, `rpm_config0`, `ctc_mode`, `rp_state_cap`) are 0: they were not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proof {
    pub desc: PlatformDescriptor,
    pub fuse2: u32,
    pub eu_disable: [u32; 3],
    pub rpm_config0: u32,
    pub ctc_mode: u32,
    pub rp_state_cap: u32,
    pub sseu: SseuInfo,
    /// The command-stream timestamp frequency.
    pub ts_hz: u32,
    /// MMIO writes forwarded to the device (the forcewake set / clear: 6 on a clean run).
    pub writes: u32,
}

/// The domain's name for markers.
pub fn domain_name(mask: u32) -> &'static str {
    match mask {
        FORCEWAKE_GT => "GT",
        FORCEWAKE_RENDER => "RENDER",
        FORCEWAKE_MEDIA => "MEDIA",
        // no forcewake domain: Cherryview's GUnit block (the GT wake gate, the well status, CHV_FUSE_GT)
        0 => "GUNIT",
        _ => "?",
    }
}

/// The named failure line. A forcewake ack is shown under its mask; a GUnit status (domain 0: Cherryview's `VLV_GTLC_PW_STATUS`) is
/// shown whole, so `ALLOWWAKEERR` and the well bits survive into the receipt.
fn fail<L: Log>(log: &mut L, e: I915Error) {
    match e {
        I915Error::AckTimeout { domain, reg, mask, want, got, polls } => emit(
            log,
            format_args!("[I915-FORCEWAKE FAIL domain={} why={} reg={:#x} got={:#x} want={:#x} polls={}]", domain_name(domain), e.name(), reg.offset(), if domain == 0 { got } else { got & mask }, want, polls),
        ),
        I915Error::MmioUnreliable { domain, reg } => {
            emit(log, format_args!("[I915-FORCEWAKE FAIL domain={} why={} reg={:#x} got=0xffffffff]", domain_name(domain), e.name(), reg.offset()))
        }
        I915Error::WriteRefused { offset, width } => emit(log, format_args!("[I915-FORCEWAKE FAIL why={} offset={:#x} width={}]", e.name(), offset, width)),
        I915Error::ForcewakeNotHeld { offset, domains } => {
            emit(log, format_args!("[I915-FORCEWAKE FAIL why={} offset={:#x} domains={:#x}]", e.name(), offset, domains))
        }
        other => emit(log, format_args!("[I915-FORCEWAKE FAIL why={}]", other.name())),
    }
}

/// The identity gate, BEFORE any register is touched: the vendor, the id in the devinfo table, and the GT gate
/// (`require_gt_port`: Gen9 and Cherryview, DECISIONS.md D8). Any other known row is refused by generation; every other id is refused as
/// unknown. Writes nothing, reads nothing.
pub fn gate<L: Log>(identity: &PciIdentity, log: &mut L) -> Result<PlatformDescriptor, I915Error> {
    let refuse = |log: &mut L, e: I915Error, extra: &str| {
        emit(log, format_args!("[I915-PROOF-OF-LIFE REFUSED id={:04x} why={}{} writes=0]", identity.device, e.name(), extra));
        e
    };
    if identity.vendor != VENDOR_INTEL {
        return Err(refuse(log, I915Error::UnknownDevice { devid: identity.device }, " vendor-not-intel"));
    }
    let desc = match lookup(identity.device) {
        Ok(d) => d,
        Err(e) => return Err(refuse(log, e, "")),
    };
    if let Err(e) = desc.require_gt_port() {
        emit(log, format_args!("[I915-PROOF-OF-LIFE REFUSED id={:04x} why={} platform={} gen={} writes=0]", identity.device, e.name(), desc.name(), desc.gen()));
        return Err(e);
    }
    Ok(desc)
}

/// Run the proof of life on an already-gated platform, by row: Cherryview's arm or the Gen9 one. See the module note for the
/// sequence and the markers.
pub fn proof_of_life<M: Mmio, L: Log>(mmio: &mut M, desc: &PlatformDescriptor, identity: &PciIdentity, log: &mut L) -> Result<Proof, I915Error> {
    match desc.platform() {
        i915_devinfo_core::Platform::Cherryview => proof_of_life_chv(mmio, desc, identity, log),
        _ => proof_of_life_gen9(mmio, desc, identity, log),
    }
}

fn proof_of_life_gen9<M: Mmio, L: Log>(mmio: &mut M, desc: &PlatformDescriptor, identity: &PciIdentity, log: &mut L) -> Result<Proof, I915Error> {
    let mut g = WriteGuard::new(ByRef(mmio), &WRITE_ALLOWLIST);
    let mut uncore = Uncore::for_platform(desc)?;
    emit(log, format_args!(
        "[I915-PROOF-OF-LIFE id={:04x} rev={:02x} platform={} gen={} llc={} display_ver={} gt={}]",
        identity.device, identity.revision, desc.name(), desc.gen(), desc.info.has_llc as u8, desc.info.display_ver, desc.gt()));

    // 1. take the three domains, one at a time, each ack checked
    for d in ORDER {
        let res = uncore.forcewake_get(&mut g, d).and_then(|()| g.check());
        if let Err(e) = res {
            fail(log, e); // earlier domains stay awake: the device stops and writes nothing further
            return Err(e);
        }
    }

    // 2. the reads, all under forcewake (uncore.read32 refuses a GT register without it)
    let rd = |uncore: &Uncore, g: &mut WriteGuard<ByRef<'_, M>>, reg: Reg| -> Result<u32, I915Error> {
        let v = uncore.read32(g, reg)?;
        if v == u32::MAX {
            // an all-ones fuse / clock register is a dead BAR, not a fuse value
            return Err(I915Error::MmioUnreliable { domain: FORCEWAKE_GT, reg });
        }
        Ok(v)
    };
    let reads = (|| -> Result<([u32; 3], u32, u32, u32, u32, u32), I915Error> {
        let fuse2 = rd(&uncore, &mut g, r::GEN8_FUSE2)?;
        let eu = [rd(&uncore, &mut g, r::gen9_eu_disable(0))?, rd(&uncore, &mut g, r::gen9_eu_disable(1))?, rd(&uncore, &mut g, r::gen9_eu_disable(2))?];
        let rpm0 = rd(&uncore, &mut g, r::RPM_CONFIG0)?;
        let ctc = rd(&uncore, &mut g, r::CTC_MODE)?;
        let cap = rd(&uncore, &mut g, r::GEN6_RP_STATE_CAP)?;
        let ts_hz = match gen9_clock_from_ctc(ctc) {
            Gen9Clock::Hz(h) => h,
            Gen9Clock::NeedsTimestampOverride => reference_ts_freq(rd(&uncore, &mut g, r::GEN9_TIMESTAMP_OVERRIDE)?),
        };
        Ok((eu, fuse2, rpm0, ctc, cap, ts_hz))
    })();
    let (eu, fuse2, rpm0, ctc, cap, ts_hz) = match reads {
        Ok(v) => v,
        Err(e) => {
            fail(log, e);
            return Err(e);
        }
    };
    let sseu = gen9_sseu_info(fuse2, eu);
    emit(log, format_args!(
        "[I915-GT-FUSE fuse2={:#x} eu_dis={:#x}/{:#x}/{:#x} slices={} subslices={} eu={} ts_hz={} rp_state_cap={:#x} rpm_config0={:#x} ctc_mode={:#x}]",
        fuse2, eu[0], eu[1], eu[2], sseu.slice_total(), sseu.subslice_total(), sseu.eu_total, ts_hz, cap, rpm0, ctc));

    // 3. release, and OBSERVE the release (the ack clears)
    for d in ORDER {
        let res = uncore.forcewake_put(&mut g, d).and_then(|()| g.check());
        if let Err(e) = res {
            fail(log, e);
            return Err(e);
        }
    }
    for d in ORDER {
        let res = uncore.wait_released(&mut g, d).and_then(|()| g.check());
        if let Err(e) = res {
            fail(log, e);
            emit(log, format_args!("[I915-FORCEWAKE domains=GT,RENDER,MEDIA ack=ok released=0]"));
            return Err(e);
        }
    }
    emit(log, format_args!("[I915-FORCEWAKE domains=GT,RENDER,MEDIA ack=ok released=1 writes={} refused={}]", g.writes(), g.refusals()));
    Ok(Proof { desc: *desc, fuse2, eu_disable: eu, rpm_config0: rpm0, ctc_mode: ctc, rp_state_cap: cap, sseu, ts_hz, writes: g.writes() })
}

/// Cherryview's domains, taken and released in `fw_domain_init` order (intel_uncore.c:2220-2225): RENDER, MEDIA.
const ORDER_CHV: [u32; 2] = [FORCEWAKE_RENDER, FORCEWAKE_MEDIA];

/// The Cherryview arm (module note). Markers: `[I915-PROOF-OF-LIFE ...]`, `[I915-GT-WELLS ...]`, `[I915-GT-FUSE chv_fuse_gt=...]`,
/// `[I915-FORCEWAKE domains=RENDER,MEDIA ...]`.
fn proof_of_life_chv<M: Mmio, L: Log>(mmio: &mut M, desc: &PlatformDescriptor, identity: &PciIdentity, log: &mut L) -> Result<Proof, I915Error> {
    use crate::exec::power_chv::{allow_gt_wake, wait_for_gt_wells_on, GtWake};
    let mut g = WriteGuard::new(ByRef(mmio), &crate::exec::allow::CHV_GT_REGS);
    let mut uncore = Uncore::for_platform(desc)?;
    emit(log, format_args!(
        "[I915-PROOF-OF-LIFE id={:04x} rev={:02x} platform={} gen={} llc={} display_ver={} gt={}]",
        identity.device, identity.revision, desc.name(), desc.gen(), desc.info.has_llc as u8, desc.info.display_ver, desc.gt()));

    // 1. the GT wake gate (vlv_allow_gt_wake): open unless the firmware left it open
    let wake = match allow_gt_wake(&mut g).and_then(|w| g.check().map(|()| w)) {
        Ok(w) => w,
        Err(e) => {
            fail(log, e);
            return Err(e);
        }
    };
    // 2. RENDER then MEDIA, each ack checked; then the wells (debug-only in Bspec: reported, never fatal)
    for d in ORDER_CHV {
        let res = uncore.forcewake_get(&mut g, d).and_then(|()| g.check());
        if let Err(e) = res {
            fail(log, e);
            return Err(e);
        }
    }
    let wells = wait_for_gt_wells_on(&mut g);
    if let Err(e) = g.check() {
        fail(log, e);
        return Err(e);
    }
    let (pw, wells_word) = match wells {
        Ok(s) => (s, "on"),
        Err(I915Error::AckTimeout { got, .. }) => (crate::exec::power_chv::PwStatus(got), "TIMEOUT"),
        Err(e) => {
            fail(log, e);
            return Err(e);
        }
    };
    emit(log, format_args!(
        "[I915-GT-WELLS gt_wake={} render={} media={} wells={} pw_status={:#x} (vlv_suspend.c)]",
        if wake == GtWake::AlreadyAllowed { "already-allowed" } else { "allowed" }, pw.render_on() as u8, pw.media_on() as u8, wells_word, pw.0));

    // 3. the fuse (GUnit: no forcewake) and the fixed Gen8 timestamp clock
    let fuse = match uncore.read32(&mut g, r::CHV_FUSE_GT) {
        Ok(v) if v != u32::MAX => v,
        Ok(_) => {
            let e = I915Error::MmioUnreliable { domain: 0, reg: r::CHV_FUSE_GT };
            fail(log, e);
            return Err(e);
        }
        Err(e) => {
            fail(log, e);
            return Err(e);
        }
    };
    let sseu = i915_devinfo_core::sseu::chv_sseu_info(fuse);
    let ts_hz = i915_devinfo_core::clock::GEN6_TS_HZ;
    emit(log, format_args!(
        "[I915-GT-FUSE chv_fuse_gt={:#x} slices={} subslices={} eu={} eu_per_subslice={} ts_hz={}]",
        fuse, sseu.slice_total(), sseu.subslice_total(), sseu.eu_total, sseu.eu_per_subslice, ts_hz));

    // 4. release, and OBSERVE the release
    for d in ORDER_CHV {
        let res = uncore.forcewake_put(&mut g, d).and_then(|()| g.check());
        if let Err(e) = res {
            fail(log, e);
            return Err(e);
        }
    }
    for d in ORDER_CHV {
        let res = uncore.wait_released(&mut g, d).and_then(|()| g.check());
        if let Err(e) = res {
            fail(log, e);
            emit(log, format_args!("[I915-FORCEWAKE domains=RENDER,MEDIA ack=ok released=0]"));
            return Err(e);
        }
    }
    emit(log, format_args!("[I915-FORCEWAKE domains=RENDER,MEDIA ack=ok released=1 writes={} refused={}]", g.writes(), g.refusals()));
    Ok(Proof { desc: *desc, fuse2: fuse, eu_disable: [0; 3], rpm_config0: 0, ctc_mode: 0, rp_state_cap: 0, sseu, ts_hz, writes: g.writes() })
}

/// The flush workarounds of this part (`gen9_emit_flush`'s quirks, `i915-gt-core::emit::FlushQuirks::for_context`), from the devinfo
/// row and the stepping the PCI revision names (`intel_step_init`, intel_step.c:228-251 - only Kaby Lake has a revid table here; any
/// other platform is `STEP_NONE`, outside every Kaby Lake range). The host never supplies them: a per-generation fact is a table's.
pub fn flush_quirks(desc: &PlatformDescriptor, revision: u8) -> i915_gt_core::emit::FlushQuirks {
    use i915_gt_core::step::{kbl_graphics_step, Step};
    let step = match desc.platform() {
        i915_devinfo_core::Platform::Kabylake => kbl_graphics_step(revision),
        _ => Step::NONE,
    };
    let ctx = i915_gt_core::wa_tables::WaContext { platform: desc.platform(), graphics_ver: desc.gen(), has_llc: desc.info.has_llc, step };
    i915_gt_core::emit::FlushQuirks::for_context(&ctx)
}
