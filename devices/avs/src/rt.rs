// SPDX-License-Identifier: GPL-2.0-only
//! The production [`Machine`]: coralos-rt's system calls, the windows the loader granted, and nothing else. The ONLY module with `unsafe`
//! (the CPU's view of DMA memory, and `Box::leak` for the `'static` write lists the rt windows take); the rest of the crate is safe Rust
//! over the [`Machine`] seam.
//!
//! Slot numbering is the loader's convention for this device (the launch row): MMIO slot 0 is BAR0 (the HD-audio controller), 1 BAR4 (the
//! DSP), 2 the PCI configuration page, 3 the codec bus's I2C controller, 4.. one window per GPIO pad community; IRQ slot 0 is the cAVS line.
//! Every window starts READ-ONLY. The write grants are narrowed to the device's own lists as it learns them
//! ([`Machine::board_loaded`], [`Machine::layout_ready`]), so even the device's own bug cannot write outside them.
#![allow(unsafe_code)]
use crate::allow::{Allow, I2C_WRITE_REGS};
use crate::gpio::{PADBAR, PAD_STRIDE};
use crate::i2c::{DwMaster, I2cHost, IrqWait, Timing};
use crate::machine::*;
use crate::wire::*;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicPtr, Ordering};
use alloc::vec::Vec;
use avs_fw_core::effect::{Space, Width};
use avs_wire::board::Board;
use coralos_hal::{Coherence, Dma, DmaRefusal, DmaRegion, Irq};
use coralos_rt::mmio::{Kernel, MmioError, ReadWindow, WriteRange, WriteWindow};
use coralos_rt::word::{PEER_DEAD, SEND_OK, SEND_RING_FULL, WAIT_IRQ_DENIED, WAIT_IRQ_FIRED, WAIT_TIMEOUT};
use coralos_rt::session::{decode_announce, ep_set, is_tombstone};
use coralos_rt::{RtDma, RtIrq};

/// The span of each window the device reads: the HD-audio header and the capability chain are found by discovery; the DSP window needs its
/// register block; the configuration page its header; the DesignWare register file ends at `COMP_TYPE` (0xfc).
const HDA_NEED: u64 = 0x40;
const DSP_NEED: u64 = 0x100;
const CFG_NEED: u64 = 0x40;
const I2C_NEED: u64 = 0x100;

pub const SLOT_HDA: u32 = 0;
pub const SLOT_DSP: u32 = 1;
pub const SLOT_CFG: u32 = 2;
pub const SLOT_I2C: u32 = 3;
pub const SLOT_PAD0: u32 = 4;
/// Pad communities a launch row may grant (a limit of the slot numbering, not of the hardware: Sunrise Point-LP has three).
pub const MAX_PAD_COMMUNITIES: u32 = 4;
/// The IRQ slot of the cAVS line.
pub const IRQ_SLOT: u32 = 0;
/// The IRQ slot of the codec bus controller's line (optional: without it the transfer sleeps between status reads).
pub const IRQ_SLOT_I2C: u32 = 1;

enum Win {
    None,
    Read(ReadWindow),
    Write(WriteWindow),
}

impl Win {
    fn open_read(slot: u32, need: u64) -> Win {
        ReadWindow::open(&mut Kernel, slot, need).map_or(Win::None, Win::Read)
    }
    fn len(&self) -> u32 {
        match self {
            Win::None => 0,
            Win::Read(w) => w.len() as u32,
            Win::Write(w) => w.len() as u32,
        }
    }
    fn read(&self, off: u32, width: Width) -> Result<u32, MmioError> {
        let o = off as u64;
        match (self, width) {
            (Win::None, _) => Err(MmioError::NoWindow { slot: 0 }),
            (Win::Read(w), Width::B8) => w.read8(o).map(u32::from),
            (Win::Read(w), Width::W16) => w.read16(o).map(u32::from),
            (Win::Read(w), Width::D32) => w.read32(o),
            (Win::Write(w), Width::B8) => w.read8(o).map(u32::from),
            (Win::Write(w), Width::W16) => w.read16(o).map(u32::from),
            (Win::Write(w), Width::D32) => w.read32(o),
        }
    }
    fn write(&mut self, off: u32, width: Width, v: u32) -> Result<(), MmioError> {
        let o = off as u64;
        match self {
            Win::Write(w) => match width {
                Width::B8 => w.write8(o, v as u8),
                Width::W16 => w.write16(o, v as u16),
                Width::D32 => w.write32(o, v),
            },
            _ => Err(MmioError::WriteNotAllowed { off: o, width: 4 }),
        }
    }
}

/// `'static` write lists, leaked once: a few dozen ranges for the life of the task.
fn leak_ranges(ranges: &[(u32, u32)]) -> Option<&'static [WriteRange]> {
    let mut v: Vec<WriteRange> = Vec::new();
    v.try_reserve_exact(ranges.len()).ok()?;
    v.extend(ranges.iter().map(|(o, l)| WriteRange { off: *o as u64, len: *l as u64 }));
    Some(Box::leak(v.into_boxed_slice()))
}

fn reopen_writable(slot: u32, need: u64, ranges: &[(u32, u32)]) -> Win {
    match leak_ranges(ranges).map(|r| WriteWindow::open(&mut Kernel, slot, need, r)) {
        Some(Ok(w)) => Win::Write(w),
        // the grant is read-only or the list does not fit the grant: stay read-only; every write then says NOT-ALLOWED
        _ => Win::open_read(slot, need),
    }
}

struct I2cWin {
    win: Win,
    master: DwMaster,
}

struct Host<'a> {
    win: &'a mut Win,
    irq: Option<&'a mut RtIrq>,
}

impl I2cHost for Host<'_> {
    fn rd(&mut self, off: u32) -> u32 {
        self.win.read(off, Width::D32).unwrap_or(u32::MAX)
    }
    fn wr(&mut self, off: u32, v: u32) {
        let _ = self.win.write(off, Width::D32, v);
    }
    fn relax(&mut self) {
        coralos_rt::wait_any_irq(0, 0, 1, 0);
    }
    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }
    fn has_irq(&self) -> bool {
        self.irq.is_some()
    }
    fn wait_irq(&mut self, timeout_ms: u64) -> IrqWait {
        let Some(i) = self.irq.as_mut() else { return IrqWait::NoLine };
        match coralos_rt::wait_any_irq(0, 0, timeout_ms.max(1), i.line) {
            WAIT_IRQ_FIRED => IrqWait::Fired,
            WAIT_TIMEOUT => IrqWait::TimedOut,
            _ => IrqWait::NoLine,
        }
    }
    fn ack_irq(&mut self) {
        if let Some(i) = self.irq.as_mut() {
            let _ = i.ack();
        }
    }
}

pub struct RtMachine {
    hda: Win,
    dsp: Win,
    cfg: Win,
    i2c: I2cWin,
    pads: [Win; MAX_PAD_COMMUNITIES as usize],
    irq: Option<RtIrq>,
    /// The codec bus controller's interrupt line, if the launch row grants one (IRQ slot 1).
    irq_i2c: Option<RtIrq>,
    dma: RtDma,
    irq_denied: bool,
    /// The open client session: (request endpoint we RECV, event endpoint we SEND).
    session: Option<(u16, u16)>,
}

impl RtMachine {
    /// Map what the loader granted, read-only.
    pub fn new() -> Self {
        Self {
            hda: Win::open_read(SLOT_HDA, HDA_NEED),
            dsp: Win::open_read(SLOT_DSP, DSP_NEED),
            cfg: Win::open_read(SLOT_CFG, CFG_NEED),
            i2c: I2cWin { win: Win::open_read(SLOT_I2C, I2C_NEED), master: DwMaster::new(Timing::default()) },
            pads: [Win::None, Win::None, Win::None, Win::None],
            irq: coralos_rt::irq_line(IRQ_SLOT).map(|line| RtIrq { line, level: true }),
            irq_i2c: coralos_rt::irq_line(IRQ_SLOT_I2C).map(|line| RtIrq { line, level: true }),
            dma: RtDma,
            irq_denied: false,
            session: None,
        }
    }

    fn space(&mut self, s: Space) -> &mut Win {
        match s {
            Space::Hda => &mut self.hda,
            Space::Dsp => &mut self.dsp,
            Space::PciConfig => &mut self.cfg,
        }
    }
}

impl Default for RtMachine {
    fn default() -> Self {
        Self::new()
    }
}

fn reg_fault(space: Space, off: u32, e: MmioError) -> RegFault {
    match e {
        MmioError::NoWindow { .. } => RegFault::NoWindow { space },
        MmioError::WriteNotAllowed { .. } | MmioError::AllowListInvalid { .. } => RegFault::NotAllowed { space, off },
        _ => RegFault::Range { space, off },
    }
}

impl Machine for RtMachine {
    fn reg_read(&mut self, space: Space, off: u32, width: Width) -> Result<u32, RegFault> {
        self.space(space).read(off, width).map_err(|e| reg_fault(space, off, e))
    }
    fn reg_write(&mut self, space: Space, off: u32, width: Width, value: u32) -> Result<(), RegFault> {
        self.space(space).write(off, width, value).map_err(|e| reg_fault(space, off, e))
    }
    fn window_len(&mut self, space: Space) -> u32 {
        self.space(space).len()
    }

    fn dma_alloc(&mut self, size: usize, align: usize) -> Result<DmaRegion, DmaRefusal> {
        // a PCIe bus-master's DMA snoops the CPU caches: write-back memory, no cache maintenance
        self.dma.alloc_coherent(size, align, Coherence::Snooped)
    }
    fn dma_write(&mut self, region: &DmaRegion, off: usize, data: &[u8]) {
        if off.checked_add(data.len()).is_none_or(|e| e > region.len) {
            return;
        }
        // SAFETY: `region` came from `dma_alloc` (the kernel mapped `len` bytes at `virt` for this task) and the range was checked above.
        unsafe { core::ptr::copy_nonoverlapping(data.as_ptr(), (region.virt as *mut u8).add(off), data.len()) };
        // publish the stores before the device is told (a doorbell or a start follows)
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
    }
    fn dma_read(&mut self, region: &DmaRegion, off: usize, out: &mut [u8]) {
        if off.checked_add(out.len()).is_none_or(|e| e > region.len) {
            return;
        }
        // SAFETY: as `dma_write`.
        unsafe { core::ptr::copy_nonoverlapping((region.virt as *const u8).add(off), out.as_mut_ptr(), out.len()) };
    }

    fn i2c_write(&mut self, addr: u16, data: &[u8]) -> Result<(), I2cFault> {
        let I2cWin { win, master } = &mut self.i2c;
        master.write(&mut Host { win, irq: self.irq_i2c.as_mut() }, addr, data)
    }
    fn i2c_write_read(&mut self, addr: u16, write: &[u8], read: &mut [u8]) -> Result<(), I2cFault> {
        let I2cWin { win, master } = &mut self.i2c;
        master.write_read(&mut Host { win, irq: self.irq_i2c.as_mut() }, addr, write, read)
    }

    fn pad_read(&mut self, reg: PadReg) -> Result<u32, PadFault> {
        let w = self.pads.get(reg.community as usize).ok_or(PadFault::NoWindow { community: reg.community })?;
        w.read(reg.off, Width::D32).map_err(|e| match e {
            MmioError::NoWindow { .. } => PadFault::NoWindow { community: reg.community },
            _ => PadFault::Range { community: reg.community, off: reg.off },
        })
    }
    fn pad_write(&mut self, reg: PadReg, value: u32) -> Result<(), PadFault> {
        let w = self.pads.get_mut(reg.community as usize).ok_or(PadFault::NoWindow { community: reg.community })?;
        w.write(reg.off, Width::D32, value).map_err(|e| match e {
            MmioError::NoWindow { .. } => PadFault::NoWindow { community: reg.community },
            MmioError::WriteNotAllowed { .. } => PadFault::NotAllowed { community: reg.community, off: reg.off },
            _ => PadFault::Range { community: reg.community, off: reg.off },
        })
    }

    fn now_ms(&mut self) -> u64 {
        coralos_rt::ticks()
    }

    fn wait(&mut self, deadline_ms: Option<u64>, want: Want) -> Wake {
        let mut eps = [0u16; 3];
        let mut n = 0;
        let mut add = |ep: u16| {
            eps[n] = ep;
            n += 1;
        };
        if want.accept {
            add(AVS_LISTEN);
        }
        if let (true, Some((req, _))) = (want.requests, self.session) {
            add(req);
        }
        if want.jack {
            add(AVS_JACK);
        }
        // endpoints past the 128-endpoint wait set cannot be named (a shift past 63 would wait on a different one): refused by leaving them out
        let (lo, hi) = ep_set(eps[..n].iter().copied()).unwrap_or((0, 0));
        let now = coralos_rt::ticks();
        let timeout = deadline_ms.map_or(0, |d| d.saturating_sub(now).max(1)); // 0 = no bound
        let irq = if want.avs_irq && !self.irq_denied { self.irq.as_ref().map_or(0, |i| i.line) } else { 0 };
        match coralos_rt::wait_any_irq(lo, hi, timeout, irq) {
            WAIT_TIMEOUT => Wake::Deadline,
            WAIT_IRQ_FIRED => Wake::AvsIrq,
            WAIT_IRQ_DENIED => {
                // the line is not ours: stop asking; the watchdog carries a running stream's liveness
                self.irq_denied = true;
                Wake::Deadline
            }
            PEER_DEAD => {
                // one dead word for two endpoints: ask the session's own which it was
                let mut b = [0u8; 16];
                match self.session {
                    Some((req, _)) if want.requests && coralos_rt::try_recv(req as u64, &mut b) == PEER_DEAD => Wake::PeerDead,
                    _ => Wake::JackGone,
                }
            }
            ep if ep == AVS_JACK as u64 => Wake::Jack,
            ep if ep == AVS_LISTEN as u64 => Wake::Accept,
            _ => Wake::Request,
        }
    }
    fn rearm_avs_irq(&mut self) {
        if let Some(i) = self.irq.as_mut() {
            // an edge/MSI vector has nothing to re-arm; a refused arm is not a fault of the device
            let _ = i.ack();
        }
    }
    fn take_jack_event(&mut self) -> bool {
        let mut b = [0u8; 16];
        let n = coralos_rt::try_recv(AVS_JACK as u64, &mut b);
        n != 0 && n != PEER_DEAD
    }
    fn rearm_jack(&mut self) {
        let mut f = [0u8; 4];
        f.copy_from_slice(&JACK_ACK_MAGIC);
        coralos_rt::send(AVS_JACK_ACK as u64, &f);
    }

    fn accept(&mut self) -> bool {
        let mut b = [0u8; 32];
        let n = coralos_rt::try_recv(AVS_LISTEN as u64, &mut b);
        if n == 0 || n == PEER_DEAD || n > b.len() as u64 {
            return false;
        }
        match decode_announce(&b[..n as usize]) {
            Some(a) if self.session.is_none() => {
                self.session = Some((a.req, a.evt));
                true
            }
            _ => false,
        }
    }
    fn release_session(&mut self) {
        if let Some((req, evt)) = self.session.take() {
            // false when the kernel still sees a live client side; the session is forgotten here either way and the pair is not reused
            let _ = coralos_rt::session_release(req, evt);
        }
    }
    fn recv_request(&mut self, buf: &mut [u8]) -> Recv {
        let Some((req, _)) = self.session else { return Recv::Empty };
        let n = coralos_rt::try_recv(req as u64, buf);
        match n {
            0 => Recv::Empty,
            PEER_DEAD => Recv::PeerDead,
            n if n <= buf.len() as u64 && is_tombstone(&buf[..n as usize]) => Recv::PeerDead,
            n if n <= buf.len() as u64 => Recv::Frame(n as usize),
            n if n < (1 << 32) => Recv::Oversized(n),
            n => Recv::Error(n),
        }
    }
    fn send_to_client(&mut self, frame: &[u8]) -> SendOutcome {
        let Some((_, evt)) = self.session else { return SendOutcome::PeerDead };
        match coralos_rt::send(evt as u64, frame) {
            SEND_OK => SendOutcome::Sent,
            SEND_RING_FULL => SendOutcome::RingFull,
            PEER_DEAD => SendOutcome::PeerDead,
            _ => SendOutcome::Denied,
        }
    }

    fn fw_exchange(&mut self, req: &[u8], reply: &mut [u8], timeout_ms: u32) -> FwRecv {
        match coralos_rt::send(AVS_FW_REQ as u64, req) {
            SEND_OK => {}
            _ => return FwRecv::Unavailable,
        }
        let deadline = coralos_rt::ticks().saturating_add(timeout_ms as u64);
        loop {
            let n = coralos_rt::try_recv(AVS_FW_REPLY as u64, reply);
            if n == PEER_DEAD {
                return FwRecv::Unavailable;
            }
            if n != 0 && n <= reply.len() as u64 {
                return FwRecv::Frame(n as usize);
            }
            let now = coralos_rt::ticks();
            if now >= deadline {
                return FwRecv::Timeout;
            }
            // block until the reply endpoint has something, or the exchange's bound
            coralos_rt::wait_any_irq(0, 1u64 << (AVS_FW_REPLY - 64), deadline - now, 0);
        }
    }

    fn log(&mut self, line: &str) {
        coralos_rt::write(line.as_bytes());
    }

    fn board_loaded(&mut self, board: &Board) {
        // the I2C controller with the board's timing, writable at exactly the transfer's registers
        let ranges: Vec<(u32, u32)> = I2C_WRITE_REGS.iter().map(|o| (*o, 4)).collect();
        self.i2c = I2cWin {
            win: reopen_writable(SLOT_I2C, I2C_NEED, &ranges),
            master: DwMaster::new(Timing { bus_hz: board.i2c_bus_hz, fs_hcnt: board.i2c_fs_hcnt, fs_lcnt: board.i2c_fs_lcnt, sda_hold: board.i2c_sda_hold }),
        };
        // the pad window of the amplifiers' reset pad: writable at that pad's PADCFG0 and nothing else
        let c = board.spkr_rst_pad.community as u32;
        if c < MAX_PAD_COMMUNITIES {
            let ro = Win::open_read(SLOT_PAD0 + c, PADBAR as u64 + 4);
            if let Ok(padbar) = ro.read(PADBAR, Width::D32) {
                if padbar != 0 && padbar != u32::MAX {
                    let off = padbar + board.spkr_rst_pad.pad as u32 * PAD_STRIDE;
                    self.pads[c as usize] = reopen_writable(SLOT_PAD0 + c, off as u64 + 4, &[(off, 4)]);
                } else {
                    self.pads[c as usize] = ro;
                }
            } else {
                self.pads[c as usize] = ro;
            }
        }
        // the jack pad's window is read-only (its level is read back)
        let j = board.jack_pad.community as u32;
        if j < MAX_PAD_COMMUNITIES && j != c {
            self.pads[j as usize] = Win::open_read(SLOT_PAD0 + j, PADBAR as u64 + 4);
        }
    }

    fn layout_ready(&mut self, allow: &Allow) {
        for (space, slot, need) in [(Space::Hda, SLOT_HDA, HDA_NEED), (Space::Dsp, SLOT_DSP, DSP_NEED), (Space::PciConfig, SLOT_CFG, CFG_NEED)] {
            if let Ok(r) = allow.window_ranges(space) {
                let w = reopen_writable(slot, need, &r);
                *self.space(space) = w;
            }
        }
    }
}

static EMERGENCY: AtomicPtr<crate::Device<RtMachine>> = AtomicPtr::new(core::ptr::null_mut());

/// The loader's `role` for a normal launch / for the safe-only launch after a task died without a clean exit.
pub const ROLE_SERVE: u64 = 0;
pub const ROLE_SAFE_ONLY: u64 = 1;

/// The safe-only role: make the hardware safe, say what was done, exit. Serves nothing.
pub fn safe_only() -> ! {
    let mut dev = crate::Device::new(RtMachine::new());
    let rep = dev.safe_only();
    coralos_rt::exit(if rep.codec_safe && rep.dma_stopped && rep.dsp_off { 0 } else { 1 })
}

/// The entry of the standalone ELF: build the machine, climb the ladder, serve until the task ends.
pub fn service() -> ! {
    let mut dev = crate::Device::new(RtMachine::new());
    // a panic handler has no other way to reach the device it must make safe
    EMERGENCY.store(&mut dev, Ordering::SeqCst);
    dev.start();
    loop {
        dev.step();
    }
}

/// A panic: put the hardware in its safe state if there is any to put, say so, and end the task so a supervisor sees it die.
pub fn on_panic() -> ! {
    let p = EMERGENCY.swap(core::ptr::null_mut(), Ordering::SeqCst);
    if !p.is_null() {
        // SAFETY: `service` stored the address of a `Device` that lives on its stack for as long as the task runs; the task is single
        // threaded and the panic abandons whatever was running on it, so nothing else touches the device from here on.
        unsafe { (*p).emergency_safe_state() };
    }
    coralos_rt::write(b"[AVS-DEV PANIC] - safe state attempted, device exiting [AVS-DEV FAIL step=PANIC]\n");
    coralos_rt::exit(crate::wire::exit_code::PANIC)
}
