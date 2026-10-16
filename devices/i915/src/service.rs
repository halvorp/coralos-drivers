// SPDX-License-Identifier: GPL-2.0-only
//! The device's entry: the real windows, the task log, and the parked request loop.
use crate::log::{emit, Log};
use crate::regio::{BarMmio, Clock, BAR0_NEED_BYTES, BAR0_SLOT, BAR0_WRITE_RANGES, CFG_NEED_BYTES, CFG_SLOT};
#[cfg(feature = "p3-exec")]
use crate::regio::{RegRead, GMCH_CTRL_DWORD};
use crate::startup::bringup;
use crate::conn::RtTransport;
use crate::wire::{exit_code, SERVICE_EP};
use coralos_rt::mmio::{Kernel, ReadWindow, WriteWindow};
use coralos_rt::{exit, write};

/// The kernel's `TICKS` (system call 24), read as milliseconds (kernel/src/arch/x86_64/apic.rs documents `TIMER_TICKS` as ms).
struct Ticks;
impl Clock for Ticks {
    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }
    /// A timed wait on nothing (the same call the parked loop paces itself with): a poll never spins.
    fn relax(&mut self) {
        coralos_rt::wait_any_irq(0, 0, 1, 0);
    }
    fn park_ms(&mut self, ms: u64) {
        coralos_rt::wait_any_irq(0, 0, ms, 0);
    }
}

struct TaskLog;
impl Log for TaskLog {
    fn line(&mut self, s: &str) {
        write(s.as_bytes());
    }
}

/// A panic: say so and end the task, so a supervisor sees it die rather than hold a half-initialised device.
pub fn on_panic() -> ! {
    write(b"[I915-KBL-DEV PANIC] - device exiting [I915-KBL-DEV FAIL step=PANIC]\n");
    exit(exit_code::PANIC)
}

/// The device. Exit codes are in [`exit_code`]; the log line before an exit names the step.
///
/// With feature `p3-exec` this entry still serves only the parked answers: the uAPI service needs facts only the HOST has (a CPU
/// view of granted memory - which takes `unsafe`, forbidden in this library - the aperture size, the machine's memory), so the
/// CoralOS-side wrapper that has them calls `service_with` instead.
pub fn service() -> ! {
    let mut lg = TaskLog;
    let (_, _, _, adoption) = start(&mut lg);
    parked(lg, adoption)
}

/// The host's contribution to the uAPI service (feature `p3-exec`); see [`service_with`].
#[cfg(feature = "p3-exec")]
pub struct UapiHost<M: crate::exec::dma::CpuMem> {
    /// The CPU's view of granted DMA memory (the loads, stores and flush of a `cpu_mapping`).
    pub mem: M,
    /// The aperture BAR's length, from the loader.
    pub gmadr_len: u64,
    /// The machine's memory as the kernel accounts it.
    pub system_memory_bytes: u64,
    pub quirks: i915_gt_core::emit::FlushQuirks,
    /// The engine's stall bound in milliseconds.
    pub stall_bound_ms: u64,
}

/// The device with the uAPI service LIVE on its endpoints (feature `p3-exec`): proof of life, then [`crate::live::Live`] on
/// `EP_I915_REQ` (the LISTEN endpoint), until the task ends.
#[cfg(feature = "p3-exec")]
pub fn service_with<M: crate::exec::dma::CpuMem>(host: UapiHost<M>) -> ! {
    serve(host)
}

/// Open the windows and run the read-only bring-up and the forcewake proof of life; exits (named) on any refusal.
fn start(lg: &mut TaskLog) -> (BarMmio<Ticks>, ReadWindow, crate::gt::Proof, crate::display::Adoption) {
    emit(lg, format_args!("[I915-KBL-DEV LAUNCH] bar0=mmio-slot-{} (writes: forcewake set x{} only) config=mmio-slot-{} (read-only)", BAR0_SLOT, BAR0_WRITE_RANGES.len(), CFG_SLOT));
    emit(lg, format_args!("[I915-CONTRACT abi={} wire={}.{} write_allow={}]", i915_common_core::CONTRACT_ABI, i915_wire::ABI_MAJOR, i915_wire::ABI_MINOR, BAR0_WRITE_RANGES.len()));
    let mut cfg = match ReadWindow::open(&mut Kernel, CFG_SLOT, CFG_NEED_BYTES) {
        Ok(w) => w,
        Err(e) => {
            emit(lg, format_args!("[I915-KBL-DEV FAIL step=CONFIG-WINDOW why={}]", e.name()));
            exit(exit_code::NO_WINDOW)
        }
    };
    let mut bar = match WriteWindow::open(&mut Kernel, BAR0_SLOT, BAR0_NEED_BYTES, &BAR0_WRITE_RANGES) {
        Ok(w) => BarMmio::new(w, Ticks),
        Err(e) => {
            emit(lg, format_args!("[I915-KBL-DEV FAIL step=BAR0-WINDOW why={}]", e.name()));
            exit(exit_code::NO_WINDOW)
        }
    };
    match bringup(&mut bar, &mut cfg, lg) {
        Ok((ready, proof)) => (bar, cfg, proof, ready.adoption),
        // the failing stage already printed its named line
        Err(crate::startup::BringupError::Proof(_)) | Err(crate::startup::BringupError::Window(_)) => exit(exit_code::PROOF_FAILED),
        Err(_) => exit(exit_code::STARTUP_FAILED),
    }
}

/// The parked loop: every request is answered, none is served. The device's endpoint is a LISTEN endpoint; each client `connect()`s and is
/// answered on its own pair, frames reassembled from messages exactly as the live loop does ([`crate::conn::Parked`]).
fn parked(mut lg: TaskLog, adoption: crate::display::Adoption) -> ! {
    emit(&mut lg, format_args!("[I915-KBL-DEV PARKED] every request is answered UNSUPPORTED (HELLO and DISPLAY-QUERY excepted); connect() sessions on LISTEN ep {}", SERVICE_EP));
    let Some(p) = crate::conn::Parked::new(RtTransport::new(SERVICE_EP, None), lg) else {
        write(b"[I915-KBL-DEV FAIL step=PARKED-BUFFERS why=LIVE-NOMEM]\n");
        exit(exit_code::STARTUP_FAILED)
    };
    let mut p = p.with_display(adoption);
    p.run();
    exit(0)
}

#[cfg(feature = "p3-exec")]
fn serve<M: crate::exec::dma::CpuMem>(host: UapiHost<M>) -> ! {
    use crate::live::{assemble::{assemble, UapiFacts}, rt_regions::RtRegionSys, Live, SessionRegions};
    use crate::uapi::rt_lease::RtLeaseSys;
    let mut lg = TaskLog;
    let (bar, mut cfg, proof, adoption) = start(&mut lg);
    let gmch_ctl = match RegRead::read32(&mut cfg, GMCH_CTRL_DWORD) {
        Ok(v) => (v & 0xffff) as u16,
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=GMCH-CTRL why={}]", e.name()));
            exit(exit_code::UAPI_FAILED)
        }
    };
    let facts = UapiFacts {
        pci_revision: match crate::identity::read_identity(&mut cfg) {
            Ok(i) => i.revision,
            Err(_) => exit(exit_code::STARTUP_FAILED),
        },
        gmch_ctl,
        bar0_len: RegRead::len(&bar),
        gmadr_len: host.gmadr_len,
        system_memory_bytes: host.system_memory_bytes,
        dma_endowment_bytes: (coralos_rt::dma_quota() as usize).saturating_mul(4096),
        quirks: host.quirks,
        stall_bound_ms: host.stall_bound_ms,
        adoption,
    };
    let mut a = match assemble(bar, coralos_rt::RtDma, host.mem, &proof, &facts, &mut lg) {
        Ok(a) => a,
        Err(f) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=UAPI-ASSEMBLE why={} errno={}]", f.name, f.errno));
            exit(exit_code::UAPI_FAILED)
        }
    };
    // GEM CPU mappings are kernel DMA leases (revoked after the GPU retires the object, or when the session ends)
    a.svc.set_lease_sys(alloc::boxed::Box::new(RtLeaseSys));
    let mut live = match Live::new(RtTransport::new(SERVICE_EP, None), SessionRegions::new(RtRegionSys), a.svc, a.ex, a.gm, TaskLog) {
        Ok(l) => l,
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=LIVE-LOOP why={}]", e.name()));
            exit(exit_code::UAPI_FAILED)
        }
    };
    emit(&mut lg, format_args!("[I915-KBL-DEV UAPI-LIVE] serving ABI {}.{} on LISTEN ep {} (one connect() session per client); blocks on connect|request|deadline only (no engine interrupt armed); payload regions: session REGION_MAP (request snapshotted, reply written); GEM mmap: DMA leases", i915_wire::ABI_MAJOR, i915_wire::ABI_MINOR, SERVICE_EP));
    live.run();
    emit(&mut lg, format_args!("[I915-KBL-DEV UAPI-LIVE END waits={}]", live.stats.waits));
    exit(0)
}
