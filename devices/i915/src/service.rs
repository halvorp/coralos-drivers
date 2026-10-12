// SPDX-License-Identifier: GPL-2.0-only
//! The device's entry: the real windows, the task log, and the parked request loop.
use crate::log::{emit, Log};
use crate::regio::{BarMmio, Clock, BAR0_NEED_BYTES, BAR0_SLOT, BAR0_WRITE_RANGES, CFG_NEED_BYTES, CFG_SLOT};
#[cfg(feature = "p3-exec")]
use crate::regio::{RegRead, GMCH_CTRL_DWORD};
use crate::startup::bringup;
use crate::wire::{exit_code, handle, PACE_MS, REPLY_EP, REQUEST_BUF, SERVICE_EP};
use coralos_rt::mmio::{Kernel, ReadWindow, WriteWindow};
use coralos_rt::{exit, recv, send, write};

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
    let _ = start(&mut lg);
    parked(lg)
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
/// `EP_I915_REQ`/`EP_I915_REPLY`, until the task ends.
#[cfg(feature = "p3-exec")]
pub fn service_with<M: crate::exec::dma::CpuMem>(host: UapiHost<M>) -> ! {
    serve(host)
}

/// The production [`crate::live::Transport`]: the request endpoint [`SERVICE_EP`], the reply endpoint [`REPLY_EP`], the kernel clock, and
/// `WAIT_ANY_IRQ` as the one blocking wait (a request, the engine line if one is held, or the deadline). Nothing here polls.
#[cfg(feature = "p3-exec")]
pub struct RtTransport {
    /// The engine interrupt line, if the device has armed one. None today: the interrupt-enable registers are not on the write
    /// allow-list until the owner-attended silicon increment, so completion is found by the service's own timer while work is in flight.
    pub irq: Option<u32>,
}

#[cfg(feature = "p3-exec")]
impl crate::live::Transport for RtTransport {
    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }
    fn irq_armed(&self) -> bool {
        self.irq.is_some()
    }
    fn wait(&mut self, deadline_ms: Option<u64>, accept_requests: bool) -> crate::live::Wake {
        use coralos_rt::word::{PEER_DEAD, WAIT_IRQ_DENIED, WAIT_IRQ_FIRED, WAIT_TIMEOUT};
        // the endpoint set is a 128-bit mask; endpoint 65 is bit 1 of the high word
        let eps_hi = if accept_requests { 1u64 << (SERVICE_EP as u32 - 64) } else { 0 };
        let now = coralos_rt::ticks();
        let timeout = deadline_ms.map_or(0, |d| d.saturating_sub(now).max(1)); // 0 = no bound
        match coralos_rt::wait_any_irq(0, eps_hi, timeout, self.irq.unwrap_or(0)) {
            WAIT_TIMEOUT => crate::live::Wake::Deadline,
            WAIT_IRQ_FIRED => crate::live::Wake::Interrupt,
            WAIT_IRQ_DENIED => {
                self.irq = None; // the line is not ours: stop asking, and let the deadline logic carry completion
                crate::live::Wake::Deadline
            }
            PEER_DEAD => crate::live::Wake::PeerDead,
            _ => crate::live::Wake::Request,
        }
    }
    fn recv(&mut self, buf: &mut [u8]) -> crate::live::Recv {
        use crate::live::Recv;
        let n = coralos_rt::try_recv(SERVICE_EP as u64, buf);
        match n {
            0 => Recv::Empty,
            coralos_rt::word::PEER_DEAD => Recv::PeerDead,
            n if n <= buf.len() as u64 => Recv::Frame(n as usize),
            n if n < (1 << 32) => Recv::Oversized(n),
            n => Recv::Error(n),
        }
    }
    fn send(&mut self, frame: &[u8]) -> crate::live::SendOutcome {
        use crate::live::SendOutcome;
        match send(REPLY_EP as u64, frame) {
            coralos_rt::word::SEND_OK => SendOutcome::Sent,
            coralos_rt::word::SEND_RING_FULL => SendOutcome::RingFull,
            coralos_rt::word::PEER_DEAD => SendOutcome::PeerDead,
            _ => SendOutcome::Denied,
        }
    }
}

/// Open the windows and run the read-only bring-up and the forcewake proof of life; exits (named) on any refusal.
fn start(lg: &mut TaskLog) -> (BarMmio<Ticks>, ReadWindow, crate::gt::Proof) {
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
        Ok((_, proof)) => (bar, cfg, proof),
        // the failing stage already printed its named line
        Err(crate::startup::BringupError::Proof(_)) | Err(crate::startup::BringupError::Window(_)) => exit(exit_code::PROOF_FAILED),
        Err(_) => exit(exit_code::STARTUP_FAILED),
    }
}

/// The parked loop: every request is answered, none is served.
fn parked(mut lg: TaskLog) -> ! {
    emit(&mut lg, format_args!("[I915-KBL-DEV PARKED] every request is answered UNSUPPORTED (HELLO excepted); request ep {} reply ep {}", SERVICE_EP, REPLY_EP));
    let mut buf = [0u8; REQUEST_BUF];
    loop {
        let n = recv(SERVICE_EP as u64, &mut buf);
        // An ABI-1.1 frame (magic "I915"): the uAPI service is not wired to this endpoint yet (src/uapi is driven by its own tests), so
        // every such request gets the named answer - Unsupported, reason INCREMENT-NOT-ENABLED - instead of the old protocol's BAD_VERSION.
        if n >= 4 && n <= buf.len() as u64 && buf[..4] == *b"I915" {
            let mut out = [0u8; 96];
            if let Ok(len) = crate::uapi::frame::answer_without_service(&buf[..n as usize], &mut out) {
                let _ = send(REPLY_EP as u64, &out[..len]);
            }
            continue;
        }
        match handle(n, &buf) {
            Some(reply) => {
                let _ = send(REPLY_EP as u64, &reply);
            }
            // nothing usable arrived: pace the loop with a timed wait on nothing instead of spinning
            None => {
                coralos_rt::wait_any_irq(0, 0, PACE_MS, 0);
            }
        }
    }
}

#[cfg(feature = "p3-exec")]
fn serve<M: crate::exec::dma::CpuMem>(host: UapiHost<M>) -> ! {
    use crate::live::{assemble::{assemble, UapiFacts}, Live, NoRegions};
    let mut lg = TaskLog;
    let (bar, mut cfg, proof) = start(&mut lg);
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
    };
    let a = match assemble(bar, coralos_rt::RtDma, host.mem, &proof, &facts, &mut lg) {
        Ok(a) => a,
        Err(f) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=UAPI-ASSEMBLE why={} errno={}]", f.name, f.errno));
            exit(exit_code::UAPI_FAILED)
        }
    };
    let mut live = match Live::new(RtTransport { irq: None }, NoRegions, a.svc, a.ex, a.gm, TaskLog) {
        Ok(l) => l,
        Err(e) => {
            emit(&mut lg, format_args!("[I915-KBL-DEV FAIL step=LIVE-LOOP why={}]", e.name()));
            exit(exit_code::UAPI_FAILED)
        }
    };
    emit(&mut lg, format_args!("[I915-KBL-DEV UAPI-LIVE] serving ABI {}.{} on request ep {} reply ep {}; blocks on request|deadline only (no engine interrupt armed); payload regions: unavailable", i915_wire::ABI_MAJOR, i915_wire::ABI_MINOR, SERVICE_EP, REPLY_EP));
    live.run();
    emit(&mut lg, format_args!("[I915-KBL-DEV UAPI-LIVE END waits={}]", live.stats.waits));
    exit(0)
}
