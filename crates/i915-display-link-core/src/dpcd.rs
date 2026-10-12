// SPDX-License-Identifier: GPL-2.0-only
//! `drm_dp_dpcd_read()` / `drm_dp_dpcd_write()` over the AUX channel: native-AUX messages, 32 retries, and the
//! throw-away probe read before every DPCD read.
//!
//! Ported from Linux drivers/gpu/drm/display/drm_dp_helper.c (v7.0-rc6): `drm_dp_dpcd_access` (:590-662),
//! `drm_dp_dpcd_probe` (:657-678), `dpcd_access_needs_probe` (:715-730), `drm_dp_dpcd_read` (:748-768),
//! `drm_dp_dpcd_write` (:786-797). Copyright (c) 2009 Keith Packard and the DRM authors.
//!
//! A [`DpcdXfer`] drives [`AuxTransfer`]s (itself a pull machine, see `aux`), so its ops are the same [`AuxOp`]s; the
//! only addition is the 500-600 us sleep between failed attempts (`AUX_RETRY_INTERVAL`, :588 / :642).
//!
//! DEVIATIONS: `aux->powered_down` (:604) and `aux->is_remote` (MST) are not modelled; the probe-before-read is always on
//! (`dpcd_probe_disabled` is never set by i915).
use crate::aux::*;
use crate::dpdefs::*;
use i915_vbt_core::child::AuxCh;

/// "bump to 32 which makes Dell 4k monitors happier": `for (retry = 0; retry < 32; retry++)` — drm_dp_helper.c:630.
pub const DPCD_ACCESS_RETRIES: u32 = 32;
/// `AUX_RETRY_INTERVAL 500 /* us */` — drm_dp_helper.c:588.
pub const AUX_RETRY_INTERVAL_US: u32 = 500;
/// The most payload one DPCD message carries here: a native write's 16 bytes (20 minus the 4 header bytes).
pub const DPCD_MAX_WRITE: usize = 16;
/// The most a read returns: 19 payload bytes (20 minus the reply byte).
pub const DPCD_MAX_READ: usize = AUX_MAX_BYTES - 1;

/// Why one attempt (and therefore, for the FIRST failure, the access) failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpcdErr {
    /// The AUX transfer itself failed (`ret < 0` from `aux->transfer`).
    Aux(AuxFail),
    /// `-EPROTO`: the sink ACKed but moved a different number of bytes.
    Short { got: usize, want: usize },
    /// `-EIO`: the sink NACKed or DEFERred (`reply` = `msg.reply & DP_AUX_NATIVE_REPLY_MASK`).
    Nack { reply: u8 },
}

/// The access failed `DPCD_ACCESS_RETRIES` times. `first` is "the error we received on the first transaction"
/// (drm_dp_helper.c:651-657).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpcdFail {
    pub first: DpcdErr,
    pub attempts: u32,
}

impl DpcdFail {
    pub const fn name(&self) -> &'static str {
        match self.first {
            DpcdErr::Aux(_) => "DPCD-AUX-FAILED",
            DpcdErr::Short { .. } => "DPCD-EPROTO",
            DpcdErr::Nack { .. } => "DPCD-EIO",
        }
    }
}

/// A DPCD access request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpcdRequest {
    Read { addr: u32, len: usize },
    Write { addr: u32, len: usize, data: [u8; DPCD_MAX_WRITE] },
}

impl DpcdRequest {
    /// A write of `bytes` (at most [`DPCD_MAX_WRITE`]; the extra is dropped by `AuxTransfer` as `-E2BIG` instead).
    pub fn write(addr: u32, bytes: &[u8]) -> DpcdRequest {
        let mut data = [0u8; DPCD_MAX_WRITE];
        let n = if bytes.len() > DPCD_MAX_WRITE { DPCD_MAX_WRITE } else { bytes.len() };
        data[..n].copy_from_slice(&bytes[..n]);
        DpcdRequest::Write { addr, len: bytes.len(), data }
    }
    pub const fn read(addr: u32, len: usize) -> DpcdRequest {
        DpcdRequest::Read { addr, len }
    }
}

/// A finished access: `len` bytes (reads: the data; writes: the count written).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DpcdOk {
    pub len: usize,
    pub data: [u8; AUX_MAX_BYTES],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DpcdStep {
    Op(AuxOp),
    Done(Result<DpcdOk, DpcdFail>),
}

/// One `drm_dp_dpcd_access()` (a single message with up to 32 attempts).
#[derive(Clone, Copy, Debug)]
struct Access {
    ch: AuxCh,
    msg: AuxMsg,
    retry: u32,
    first: Option<DpcdErr>,
    /// The previous attempt ended `-ETIMEDOUT`, the only failure that skips the retry sleep (:642).
    prev_timed_out: bool,
    xfer: Option<AuxTransfer>,
    sleeping: bool,
}

enum AccessStep {
    Op(AuxOp),
    Done(Result<DpcdOk, DpcdFail>),
}

impl Access {
    fn new(ch: AuxCh, msg: AuxMsg) -> Access {
        Access { ch, msg, retry: 0, first: None, prev_timed_out: false, xfer: None, sleeping: false }
    }

    /// Start attempt `self.retry`: sleep first if the last one failed with anything but a timeout, else transfer.
    fn begin_attempt(&mut self, sleep_first: bool) -> AccessStep {
        if sleep_first {
            self.sleeping = true;
            return AccessStep::Op(AuxOp::SleepUs { min: AUX_RETRY_INTERVAL_US, max: AUX_RETRY_INTERVAL_US + 100 });
        }
        self.start_transfer()
    }

    fn start_transfer(&mut self) -> AccessStep {
        match AuxTransfer::new(self.ch, &self.msg) {
            Ok(mut t) => {
                let step = t.step(Feed::Start);
                self.xfer = Some(t);
                self.after_transfer_step(step)
            }
            // A message the AUX channel cannot carry is refused at once: -E2BIG is not retried by the caller of
            // aux->transfer (it returns before sending), and every retry would refuse identically.
            Err(e) => self.fail_attempt(DpcdErr::Aux(e), false),
        }
    }

    fn after_transfer_step(&mut self, step: TransferStep) -> AccessStep {
        match step {
            TransferStep::Op(op) => AccessStep::Op(op),
            TransferStep::Done(Err(e)) => {
                let timed_out = e.errno == AuxErrno::TimedOut;
                self.fail_attempt(DpcdErr::Aux(e), timed_out)
            }
            TransferStep::Done(Ok(r)) => {
                let native_reply = r.reply & DP_AUX_NATIVE_REPLY_MASK;
                if native_reply == DP_AUX_NATIVE_REPLY_ACK {
                    if r.ret == self.msg.size {
                        return AccessStep::Done(Ok(DpcdOk { len: r.ret, data: r.data }));
                    }
                    self.fail_attempt(DpcdErr::Short { got: r.ret, want: self.msg.size }, false)
                } else {
                    self.fail_attempt(DpcdErr::Nack { reply: native_reply }, false)
                }
            }
        }
    }

    fn fail_attempt(&mut self, err: DpcdErr, timed_out: bool) -> AccessStep {
        if self.first.is_none() {
            self.first = Some(err);
        }
        self.prev_timed_out = timed_out;
        self.retry += 1;
        if self.retry >= DPCD_ACCESS_RETRIES {
            // "Too many retries, giving up. First error: %d" — :659-660
            return AccessStep::Done(Err(DpcdFail { first: self.first.unwrap_or(err), attempts: self.retry }));
        }
        self.begin_attempt(!timed_out)
    }

    fn step(&mut self, feed: Feed) -> AccessStep {
        if self.sleeping {
            self.sleeping = false;
            return self.start_transfer();
        }
        match self.xfer.as_mut() {
            Some(t) => {
                let s = t.step(feed);
                self.after_transfer_step(s)
            }
            None => AccessStep::Done(Err(DpcdFail { first: DpcdErr::Aux(AuxFail { errno: AuxErrno::Invalid, why: "dpcd step before start", status: 0 }), attempts: self.retry })),
        }
    }
}

/// `drm_dp_dpcd_read()` / `drm_dp_dpcd_write()`.
#[derive(Clone, Copy, Debug)]
pub struct DpcdXfer {
    ch: AuxCh,
    req: DpcdRequest,
    cur: Access,
    probing: bool,
}

impl DpcdXfer {
    pub fn new(ch: AuxCh, req: DpcdRequest) -> DpcdXfer {
        match req {
            // `dpcd_access_needs_probe()`: a read throws away a 1-byte read of DP_TRAINING_PATTERN_SET first.
            DpcdRequest::Read { .. } => {
                let probe = AuxMsg { request: DP_AUX_NATIVE_READ, address: DP_TRAINING_PATTERN_SET, size: 1, data: [0; 16] };
                DpcdXfer { ch, req, cur: Access::new(ch, probe), probing: true }
            }
            DpcdRequest::Write { addr, len, data } => {
                let msg = AuxMsg { request: DP_AUX_NATIVE_WRITE, address: addr, size: len, data };
                DpcdXfer { ch, req, cur: Access::new(ch, msg), probing: false }
            }
        }
    }

    /// Advance; `Feed::Start` first.
    pub fn step(&mut self, feed: Feed) -> DpcdStep {
        let s = if matches!(feed, Feed::Start) { self.cur.start_transfer() } else { self.cur.step(feed) };
        match s {
            AccessStep::Op(op) => DpcdStep::Op(op),
            AccessStep::Done(Err(e)) => DpcdStep::Done(Err(e)),
            AccessStep::Done(Ok(ok)) => {
                if self.probing {
                    // "return ret < 0 ? ret : 0" — the probe's result is only its success (:679-681).
                    self.probing = false;
                    if let DpcdRequest::Read { addr, len } = self.req {
                        let msg = AuxMsg { request: DP_AUX_NATIVE_READ, address: addr, size: len, data: [0; 16] };
                        self.cur = Access::new(self.ch, msg);
                        return match self.cur.start_transfer() {
                            AccessStep::Op(op) => DpcdStep::Op(op),
                            AccessStep::Done(r) => DpcdStep::Done(r),
                        };
                    }
                }
                DpcdStep::Done(Ok(ok))
            }
        }
    }
}
