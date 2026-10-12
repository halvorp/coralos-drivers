// SPDX-License-Identifier: GPL-2.0-only
//! The SKL-class DP AUX channel: message packing and the `DP_AUX_CH_CTL` send / receive state machine.
//!
//! Ported from Linux drivers/gpu/drm/i915/display/intel_dp_aux.c (v7.0-rc6): `intel_dp_aux_pack/unpack`
//! (:35-55), `skl_get_aux_clock_divider` (:131-139), `intel_dp_aux_sync_len` / `intel_dp_aux_fw_sync_len` (:141-166),
//! `skl_get_aux_send_ctl` (:202-236), `intel_dp_aux_xfer` (:238-455), `intel_dp_aux_header` (:460-468) and
//! `intel_dp_aux_transfer` (:484-555). Copyright (c) 2020-2021 Intel Corporation.
//!
//! SHAPE. [`AuxTransfer`] is a pull state machine: call [`AuxTransfer::step`] with [`Feed::Start`], perform the returned
//! [`AuxOp`], call `step` again with what the hardware answered ([`Feed::Value`] for a read / poll, [`Feed::Ack`]
//! otherwise), until it returns [`AuxStep::Done`]. No MMIO, no clock.
//!
//! DEVIATIONS FROM LINUX, each named:
//!  * `WAIT_DONE_POLLED`: Linux sleeps on `display->gmbus.wait_queue` for up to 10 ms, woken by the AUX interrupt
//!    (:66-77). With no interrupt here, the wait is a [`PollPlan`] on `SEND_BUSY` clear: 10 ms, read every 50 us, and
//!    the executor returns the LAST status read, exactly as `intel_dp_aux_wait_done()` returns its last `status`.
//!  * `NO_POWER_DOMAIN_NO_PPS_NO_QOS`: the AUX power domain, the PPS lock/VDD (`intel_pps_vdd_on_unlocked`), the PM-QoS request and
//!    the `intel_digital_port_connected_locked` early-out are the CALLER's job (the device owns power; eDP skips the
//!    connected check, :267). The caller must hold VDD/panel power first (`pps::Pps::vdd_on`).
//!  * `NO_DPCD_QUIRKS`: `QUIRK_FW_SYNC_LEN` (+2 precharge, :162) is not ported; the unquirked 10 is used.
//!  * `NO_TBT`: `DP_AUX_CH_CTL_TBT_IO` (Type-C TBT alt mode, :225) and the display-ver-14 power-request bit (:232) do not apply to KBL.
use crate::dpdefs::*;
use crate::plan::PollPlan;
use crate::regs::*;
use i915_common_core::reg::field_get;
use i915_common_core::Reg;
use i915_vbt_core::child::AuxCh;

/// "Only 5 data registers!" — intel_dp_aux.c:337-341. Max packet in either direction.
pub const AUX_MAX_BYTES: usize = 20;
/// `BARE_ADDRESS_SIZE` — intel_dp_aux.c:457.
pub const BARE_ADDRESS_SIZE: usize = 3;
/// `HEADER_SIZE` — intel_dp_aux.c:458.
pub const HEADER_SIZE: usize = BARE_ADDRESS_SIZE + 1;
/// "Try to wait for any previous AUX channel activity": `for (try = 0; try < 3; try++)` — intel_dp_aux.c:314.
pub const PRE_BUSY_TRIES: u32 = 3;
/// "Must try at least 3 times according to DP spec": `for (try = 0; try < 5; try++)` — intel_dp_aux.c:350-351.
pub const SEND_TRIES: u32 = 5;
/// `const unsigned int timeout_ms = 10;` — intel_dp_aux.c:62.
pub const WAIT_DONE_TIMEOUT_US: u32 = 10_000;
/// Poll interval of the polled replacement for the interrupt wait (see module doc, `WAIT_DONE_POLLED`).
pub const WAIT_DONE_INTERVAL_US: u32 = 50;

/// `intel_dp_aux_pack()` — intel_dp_aux.c:35-45: up to 4 bytes, first byte in the MSB.
pub fn aux_pack(src: &[u8]) -> u32 {
    let n = if src.len() > 4 { 4 } else { src.len() };
    let mut v = 0u32;
    let mut i = 0;
    while i < n {
        v |= (src[i] as u32) << ((3 - i) * 8);
        i += 1;
    }
    v
}

/// `intel_dp_aux_unpack()` — intel_dp_aux.c:47-55: `dst_bytes` (at most 4) bytes, MSB first.
pub fn aux_unpack(src: u32, dst: &mut [u8]) {
    let n = if dst.len() > 4 { 4 } else { dst.len() };
    let mut i = 0;
    while i < n {
        dst[i] = (src >> ((3 - i) * 8)) as u8;
        i += 1;
    }
}

/// `skl_get_aux_clock_divider()` — intel_dp_aux.c:131-139. SKL has one divider ("hardware derives the clock from CDCLK"):
/// index 0 gives 1, any other index 0 (which ends the divider loop, :343).
pub const fn skl_aux_clock_divider(index: u32) -> u32 {
    if index != 0 {
        0
    } else {
        1
    }
}

/// `intel_dp_aux_sync_len()` — intel_dp_aux.c:141-147: precharge 16 + preamble 16.
pub const fn aux_sync_len() -> u32 {
    16 + 16
}

/// `intel_dp_aux_fw_sync_len()` — intel_dp_aux.c:149-166: precharge 10 + preamble 8 (no quirk).
pub const fn aux_fw_sync_len() -> u32 {
    10 + 8
}

/// `skl_get_aux_send_ctl()` — intel_dp_aux.c:202-236, for a non-Type-C, display-ver-9 port.
pub const fn skl_aux_send_ctl(send_bytes: u32) -> u32 {
    DP_AUX_CH_CTL_SEND_BUSY
        | DP_AUX_CH_CTL_DONE
        | DP_AUX_CH_CTL_INTERRUPT
        | DP_AUX_CH_CTL_TIME_OUT_ERROR
        | DP_AUX_CH_CTL_TIME_OUT_MAX
        | DP_AUX_CH_CTL_RECEIVE_ERROR
        | dp_aux_ch_ctl_message_size(send_bytes)
        | dp_aux_ch_ctl_fw_sync_pulse_skl(aux_fw_sync_len())
        | dp_aux_ch_ctl_sync_pulse_skl(aux_sync_len())
}

/// `-E…` of a failed transfer, by name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuxErrno {
    /// `-EBUSY`: channel never idle, not done, or a forbidden receive size.
    Busy,
    /// `-E2BIG`: more than 5 data registers' worth.
    TooBig,
    /// `-EIO`: receive error.
    Io,
    /// `-ETIMEDOUT`: sink did not answer.
    TimedOut,
    /// `-EINVAL`: an unknown request code.
    Invalid,
}

/// A failed AUX transfer: which errno, why, and the last `DP_AUX_CH_CTL` status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuxFail {
    pub errno: AuxErrno,
    pub why: &'static str,
    pub status: u32,
}

impl AuxFail {
    /// Stable marker token: `AUX-EBUSY`, `AUX-ETIMEDOUT` ... (`why` says which check).
    pub const fn name(&self) -> &'static str {
        match self.errno {
            AuxErrno::Busy => "AUX-EBUSY",
            AuxErrno::TooBig => "AUX-E2BIG",
            AuxErrno::Io => "AUX-EIO",
            AuxErrno::TimedOut => "AUX-ETIMEDOUT",
            AuxErrno::Invalid => "AUX-EINVAL",
        }
    }
}

const fn fail(errno: AuxErrno, why: &'static str, status: u32) -> AuxFail {
    AuxFail { errno, why, status }
}

/// What the executor is asked to do next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuxOp {
    /// Read the register; answer `Feed::Value`.
    Read(Reg),
    /// Write the register; answer `Feed::Ack`.
    Write(Reg, u32),
    /// Poll until the plan holds or its read cap is reached; answer `Feed::Value(last value read)` either way.
    WaitNotBusy(PollPlan),
    /// `msleep(ms)`; answer `Feed::Ack`.
    SleepMs(u32),
    /// `usleep_range(min, max)`; answer `Feed::Ack`.
    SleepUs { min: u32, max: u32 },
}

/// What the executor answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    Start,
    Ack,
    Value(u32),
}

/// A finished raw transfer (`intel_dp_aux_xfer`'s return value): `len` bytes received, clamped to `recv_size`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuxRx {
    pub len: usize,
    pub data: [u8; AUX_MAX_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuxStep {
    Op(AuxOp),
    Done(Result<AuxRx, AuxFail>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum St {
    Start,
    /// Waiting for the value of the pre-transfer idle read; `n` reads have been made.
    PreRead { n: u32 },
    /// The msleep(1) after a busy read; `n` reads have been made.
    PreSleep { n: u32 },
    /// The re-read for the "not started" report.
    PreFinal,
    /// Writing send data register `i` (byte offset `4 * i`).
    Load { try_n: u32, i: usize },
    WriteCtl { try_n: u32 },
    Wait { try_n: u32 },
    Clear { try_n: u32, status: u32 },
    RecvSleep { try_n: u32 },
    /// Reading receive data register `i`.
    Unload { i: usize },
    Finished,
}

/// One `intel_dp_aux_xfer()`.
#[derive(Clone, Copy, Debug)]
pub struct AuxXfer {
    ch: AuxCh,
    send: [u8; AUX_MAX_BYTES],
    send_bytes: usize,
    recv_size: usize,
    flags: u32,
    st: St,
    status: u32,
    recv_bytes: usize,
    rx: [u8; AUX_MAX_BYTES],
}

impl AuxXfer {
    /// `send_bytes` bytes of `send` go out; at most `recv_size` bytes come back. `aux_send_ctl_flags` is OR-ed into the
    /// control word (`DP_AUX_CH_CTL_AUX_AKSV_SELECT` for the HDCP Aksv write). Sizes are validated at their Linux point
    /// (after the idle wait), not here.
    pub fn new(ch: AuxCh, send: &[u8], recv_size: usize, aux_send_ctl_flags: u32) -> AuxXfer {
        let mut buf = [0u8; AUX_MAX_BYTES];
        let n = if send.len() > AUX_MAX_BYTES { AUX_MAX_BYTES } else { send.len() };
        buf[..n].copy_from_slice(&send[..n]);
        // `send.len()` beyond the buffer is remembered as-is so the E2BIG check still fires.
        AuxXfer { ch, send: buf, send_bytes: send.len(), recv_size, flags: aux_send_ctl_flags, st: St::Start, status: 0, recv_bytes: 0, rx: [0; AUX_MAX_BYTES] }
    }

    fn ctl(&self) -> Reg {
        dp_aux_ch_ctl(self.ch)
    }

    fn poll(&self) -> PollPlan {
        PollPlan::with_timeout(self.ctl(), DP_AUX_CH_CTL_SEND_BUSY, 0, WAIT_DONE_INTERVAL_US, WAIT_DONE_TIMEOUT_US, "AUX CH did not complete")
    }

    fn misuse(&mut self, why: &'static str) -> AuxStep {
        self.st = St::Finished;
        AuxStep::Done(Err(fail(AuxErrno::Invalid, why, self.status)))
    }

    /// Next write of send data, or the control word if all data is loaded.
    fn next_load(&mut self, try_n: u32, i: usize) -> AuxStep {
        if i * 4 < self.send_bytes {
            let off = i * 4;
            let end = if self.send_bytes > AUX_MAX_BYTES { AUX_MAX_BYTES } else { self.send_bytes };
            let v = aux_pack(&self.send[off..end]);
            self.st = St::Load { try_n, i: i + 1 };
            AuxStep::Op(AuxOp::Write(dp_aux_ch_data(self.ch, i as u32), v))
        } else {
            self.st = St::WriteCtl { try_n };
            // divider = skl_get_aux_clock_divider(clock = 0) = 1: the send_ctl for this divider (intel_dp_aux.c:343-346).
            AuxStep::Op(AuxOp::Write(self.ctl(), skl_aux_send_ctl(self.send_bytes as u32) | self.flags))
        }
    }

    /// `done:` of intel_dp_aux.c:394-440 (also reached from the "tries exhausted" path with the last status).
    fn done(&mut self, status: u32) -> AuxStep {
        if status & DP_AUX_CH_CTL_RECEIVE_ERROR != 0 {
            self.st = St::Finished;
            return AuxStep::Done(Err(fail(AuxErrno::Io, "receive-error", status))); // :399-404
        }
        if status & DP_AUX_CH_CTL_TIME_OUT_ERROR != 0 {
            self.st = St::Finished;
            return AuxStep::Done(Err(fail(AuxErrno::TimedOut, "timeout", status))); // :410-415
        }
        let mut recv_bytes = field_get(DP_AUX_CH_CTL_MESSAGE_SIZE_MASK, status) as usize; // :418
        // "Message sizes of 0 or >20 are not allowed." — :420-431
        if recv_bytes == 0 || recv_bytes > AUX_MAX_BYTES {
            self.st = St::Finished;
            return AuxStep::Done(Err(fail(AuxErrno::Busy, "forbidden-recv-bytes", status)));
        }
        if recv_bytes > self.recv_size {
            recv_bytes = self.recv_size; // :433-434
        }
        self.recv_bytes = recv_bytes;
        self.next_unload(0)
    }

    fn next_unload(&mut self, i: usize) -> AuxStep {
        if i * 4 < self.recv_bytes {
            self.st = St::Unload { i };
            AuxStep::Op(AuxOp::Read(dp_aux_ch_data(self.ch, i as u32)))
        } else {
            self.st = St::Finished;
            AuxStep::Done(Ok(AuxRx { len: self.recv_bytes, data: self.rx }))
        }
    }

    /// After a try that neither timed out nor errored nor completed (or after one that did): next try or give up.
    fn next_try(&mut self, try_n: u32) -> AuxStep {
        let try_n = try_n + 1;
        if try_n < SEND_TRIES {
            return self.next_load(try_n, 0);
        }
        // The divider loop ends (skl_get_aux_clock_divider(1) == 0, :343); :387-392.
        if self.status & DP_AUX_CH_CTL_DONE == 0 {
            self.st = St::Finished;
            return AuxStep::Done(Err(fail(AuxErrno::Busy, "not-done", self.status)));
        }
        let status = self.status;
        self.done(status)
    }

    /// Advance. `Feed::Start` begins; afterwards pass what the previous [`AuxOp`] produced.
    pub fn step(&mut self, feed: Feed) -> AuxStep {
        match (self.st, feed) {
            (St::Start, Feed::Start) => {
                self.st = St::PreRead { n: 0 };
                AuxStep::Op(AuxOp::Read(self.ctl()))
            }
            (St::PreRead { n }, Feed::Value(status)) => {
                self.status = status;
                if status & DP_AUX_CH_CTL_SEND_BUSY == 0 {
                    // Idle. Size check (:337-341) comes after the wait, before any send.
                    if self.send_bytes > AUX_MAX_BYTES || self.recv_size > AUX_MAX_BYTES {
                        self.st = St::Finished;
                        return AuxStep::Done(Err(fail(AuxErrno::TooBig, "more-than-5-data-registers", status)));
                    }
                    return self.next_load(0, 0);
                }
                // Busy: msleep(1) after EVERY busy read, including the last (:314-319).
                self.st = St::PreSleep { n: n + 1 };
                AuxStep::Op(AuxOp::SleepMs(1))
            }
            (St::PreSleep { n }, Feed::Ack) => {
                if n >= PRE_BUSY_TRIES {
                    self.st = St::PreFinal; // `if (try == 3)` — :323
                    return AuxStep::Op(AuxOp::Read(self.ctl()));
                }
                self.st = St::PreRead { n };
                AuxStep::Op(AuxOp::Read(self.ctl()))
            }
            (St::PreFinal, Feed::Value(status)) => {
                self.st = St::Finished;
                AuxStep::Done(Err(fail(AuxErrno::Busy, "not-started", status))) // :323-334
            }
            (St::Load { try_n, i }, Feed::Ack) => self.next_load(try_n, i),
            (St::WriteCtl { try_n }, Feed::Ack) => {
                self.st = St::Wait { try_n };
                AuxStep::Op(AuxOp::WaitNotBusy(self.poll()))
            }
            (St::Wait { try_n }, Feed::Value(status)) => {
                self.status = status;
                self.st = St::Clear { try_n, status };
                // "Clear done status and any errors" — :363-367
                AuxStep::Op(AuxOp::Write(
                    self.ctl(),
                    status | DP_AUX_CH_CTL_DONE | DP_AUX_CH_CTL_TIME_OUT_ERROR | DP_AUX_CH_CTL_RECEIVE_ERROR,
                ))
            }
            (St::Clear { try_n, status }, Feed::Ack) => {
                if status & DP_AUX_CH_CTL_TIME_OUT_ERROR != 0 {
                    return self.next_try(try_n); // :375-376 (the HW timeout already meets the 400 us rule)
                }
                if status & DP_AUX_CH_CTL_RECEIVE_ERROR != 0 {
                    self.st = St::RecvSleep { try_n };
                    return AuxStep::Op(AuxOp::SleepUs { min: 400, max: 500 }); // :378-381
                }
                if status & DP_AUX_CH_CTL_DONE != 0 {
                    return self.done(status); // :382-383
                }
                self.next_try(try_n)
            }
            (St::RecvSleep { try_n }, Feed::Ack) => self.next_try(try_n),
            (St::Unload { i }, Feed::Value(v)) => {
                let off = i * 4;
                let end = self.recv_bytes;
                aux_unpack(v, &mut self.rx[off..end]); // :436-438: dst_bytes = recv_bytes - i
                self.next_unload(i + 1)
            }
            _ => self.misuse("aux step fed the wrong kind of answer"),
        }
    }
}

/// A native/I2C-over-AUX message (`struct drm_dp_aux_msg`): `request`, 20-bit `address`, `size`, and for writes up to
/// 16 payload bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuxMsg {
    pub request: u8,
    pub address: u32,
    pub size: usize,
    pub data: [u8; 16],
}

/// `intel_dp_aux_header()` — intel_dp_aux.c:460-468. `size - 1` wraps for a zero-size (address-only) message, which
/// is sent as the bare 3-byte address anyway.
pub fn aux_header(msg: &AuxMsg) -> [u8; HEADER_SIZE] {
    [
        (msg.request << 4) | ((msg.address >> 16) & 0xf) as u8,
        ((msg.address >> 8) & 0xff) as u8,
        (msg.address & 0xff) as u8,
        (msg.size as u8).wrapping_sub(1),
    ]
}

/// `intel_dp_aux_xfer_flags()` — intel_dp_aux.c:470-482: the HDCP Aksv write sets the Aksv-select bit.
pub fn aux_xfer_flags(msg: &AuxMsg) -> u32 {
    if (msg.request & !DP_AUX_I2C_MOT) == DP_AUX_NATIVE_WRITE && msg.address == DP_AUX_HDCP_AKSV {
        DP_AUX_CH_CTL_AUX_AKSV_SELECT
    } else {
        0
    }
}

/// A finished message (`intel_dp_aux_transfer`'s `msg->reply` and return value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuxReply {
    /// `msg->reply` = `rxbuf[0] >> 4`.
    pub reply: u8,
    /// The return value: payload bytes written (short write) / read.
    pub ret: usize,
    /// For reads, the payload (`ret` valid bytes).
    pub data: [u8; AUX_MAX_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferStep {
    Op(AuxOp),
    Done(Result<AuxReply, AuxFail>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Write,
    Read,
}

/// One `intel_dp_aux_transfer()`: a message in, a reply out.
#[derive(Clone, Copy, Debug)]
pub struct AuxTransfer {
    xfer: AuxXfer,
    kind: Kind,
    size: usize,
    done: bool,
}

impl AuxTransfer {
    /// Build the transfer. `Err` for an unknown request (`-EINVAL`, :549-551) or sizes the 5 data registers cannot carry
    /// (`-E2BIG`, :503-504, :531-532) — both before anything is sent.
    pub fn new(ch: AuxCh, msg: &AuxMsg) -> Result<AuxTransfer, AuxFail> {
        let mut txbuf = [0u8; AUX_MAX_BYTES];
        let hdr = aux_header(msg);
        txbuf[..HEADER_SIZE].copy_from_slice(&hdr);
        let flags = aux_xfer_flags(msg);
        match msg.request & !DP_AUX_I2C_MOT {
            DP_AUX_NATIVE_WRITE | DP_AUX_I2C_WRITE | DP_AUX_I2C_WRITE_STATUS_UPDATE => {
                let txsize = if msg.size != 0 { HEADER_SIZE + msg.size } else { BARE_ADDRESS_SIZE };
                if txsize > AUX_MAX_BYTES {
                    return Err(fail(AuxErrno::TooBig, "write-payload", 0));
                }
                txbuf[HEADER_SIZE..HEADER_SIZE + msg.size].copy_from_slice(&msg.data[..msg.size]);
                Ok(AuxTransfer { xfer: AuxXfer::new(ch, &txbuf[..txsize], 2, flags), kind: Kind::Write, size: msg.size, done: false })
            }
            DP_AUX_NATIVE_READ | DP_AUX_I2C_READ => {
                let txsize = if msg.size != 0 { HEADER_SIZE } else { BARE_ADDRESS_SIZE };
                let rxsize = msg.size + 1;
                if rxsize > AUX_MAX_BYTES {
                    return Err(fail(AuxErrno::TooBig, "read-payload", 0));
                }
                Ok(AuxTransfer { xfer: AuxXfer::new(ch, &txbuf[..txsize], rxsize, flags), kind: Kind::Read, size: msg.size, done: false })
            }
            _ => Err(fail(AuxErrno::Invalid, "unknown-request", 0)),
        }
    }

    pub fn step(&mut self, feed: Feed) -> TransferStep {
        if self.done {
            return TransferStep::Done(Err(fail(AuxErrno::Invalid, "stepped-after-done", 0)));
        }
        match self.xfer.step(feed) {
            AuxStep::Op(op) => TransferStep::Op(op),
            AuxStep::Done(Err(e)) => {
                self.done = true;
                TransferStep::Done(Err(e))
            }
            AuxStep::Done(Ok(rx)) => {
                self.done = true;
                let mut data = [0u8; AUX_MAX_BYTES];
                let reply = rx.data[0] >> 4;
                let ret = match self.kind {
                    Kind::Write => {
                        // :513-523: rxbuf[1] is the short-write count when a second byte came back.
                        if rx.len > 1 {
                            let n = rx.data[1] as usize;
                            if n > self.size {
                                self.size
                            } else {
                                n
                            }
                        } else {
                            self.size
                        }
                    }
                    Kind::Read => {
                        // :536-546: the payload follows the reply byte.
                        let n = rx.len - 1;
                        data[..n].copy_from_slice(&rx.data[1..1 + n]);
                        n
                    }
                };
                TransferStep::Done(Ok(AuxReply { reply, ret, data }))
            }
        }
    }
}
