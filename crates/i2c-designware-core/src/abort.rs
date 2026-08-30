// SPDX-License-Identifier: GPL-2.0-only
//! Why a transfer was aborted — TX_ABRT_SOURCE decoded into named causes.
//!
//! Bit POSITIONS come from `i2c-designware-core.h`; the messages come from `abort_sources[]` in
//! `i2c-designware-common.c`. Both were extracted mechanically and PAIRED BY NAME, so a position
//! and its message cannot drift apart the way two hand-copied lists would.
//!
//! A GAP THAT A PREFIX-BASED EXTRACTION WOULD HAVE SHIPPED: bit 12 is `ARB_LOST`, named WITHOUT the
//! `ABRT_` prefix every other cause carries. Keying off the prefix yields 13 causes and silently
//! loses arbitration-loss — a real failure that would then decode as "unknown". The extraction is
//! therefore driven from `abort_sources[]`, which is authoritative about which causes exist.
//!
//! Bits 6 and 8 are genuinely undefined in Linux; they are absent here for that reason, not
//! overlooked.

/// One decoded abort cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbortCause {
    /// Bit position within TX_ABRT_SOURCE.
    pub bit: u8,
    /// Linux's own name for the cause, minus the `ABRT_` prefix.
    pub name: &'static str,
    /// Linux's own message. Carried verbatim so a refusal can NAME what refused rather than
    /// collapsing into a generic error.
    pub message: &'static str,
}

/// Every cause Linux defines, in bit order.
pub const ABORT_CAUSES: &[AbortCause] = &[
    AbortCause { bit: 0, name: "7B_ADDR_NOACK", message: "slave address not acknowledged (7bit mode)" }, // core.h:166
    AbortCause { bit: 1, name: "10ADDR1_NOACK", message: "first address byte not acknowledged (10bit mode)" }, // core.h:167
    AbortCause { bit: 2, name: "10ADDR2_NOACK", message: "second address byte not acknowledged (10bit mode)" }, // core.h:168
    AbortCause { bit: 3, name: "TXDATA_NOACK", message: "data not acknowledged" }, // core.h:169
    AbortCause { bit: 4, name: "GCALL_NOACK", message: "no acknowledgement for a general call" }, // core.h:170
    AbortCause { bit: 5, name: "GCALL_READ", message: "read after general call" }, // core.h:171
    AbortCause { bit: 7, name: "SBYTE_ACKDET", message: "start byte acknowledged" }, // core.h:172
    AbortCause { bit: 9, name: "SBYTE_NORSTRT", message: "trying to send start byte when restart is disabled" }, // core.h:173
    AbortCause { bit: 10, name: "10B_RD_NORSTRT", message: "trying to read when restart is disabled (10bit mode)" }, // core.h:174
    AbortCause { bit: 11, name: "MASTER_DIS", message: "trying to use disabled adapter" }, // core.h:175
    AbortCause { bit: 12, name: "ARB_LOST", message: "lost arbitration" }, // core.h:176
    AbortCause { bit: 13, name: "SLAVE_FLUSH_TXFIFO", message: "read command so flush old data in the TX FIFO" }, // core.h:177
    AbortCause { bit: 14, name: "SLAVE_ARBLOST", message: "slave lost the bus while transmitting data to a remote master" }, // core.h:178
    AbortCause { bit: 15, name: "SLAVE_RD_INTX", message: "incorrect slave-transmitter mode configuration" }, // core.h:179
];

/// The causes present in a raw TX_ABRT_SOURCE word.
///
/// Returns them in bit order. Bits Linux does not define are NOT reported here — see
/// [`undecoded`], which exists so an unknown bit cannot vanish silently.
pub fn causes_in(abort_source: u32) -> impl Iterator<Item = &'static AbortCause> {
    ABORT_CAUSES.iter().filter(move |c| abort_source & (1 << c.bit) != 0)
}

/// The bits set in `abort_source` that no known cause explains.
///
/// A decoder that silently dropped these would turn "the controller reported something we have
/// never seen" into "nothing happened", which is the failure this project treats as worse than a
/// crash: the raw value survives so a reader can go and look it up.
pub fn undecoded(abort_source: u32) -> u32 {
    let known: u32 = ABORT_CAUSES.iter().map(|c| 1u32 << c.bit).fold(0, |a, b| a | b);
    abort_source & !known
}

/// What a caller is told when a transfer aborts.
///
/// From `i2c_dw_handle_tx_abort` (i2c-designware-common.c:764-:785). The distinctions are the
/// point: a NAK is not a bus fault, and a lost arbitration is RETRYABLE where the others are not.
/// Collapsing these into one error throws away the only diagnosis the controller offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortVerdict {
    /// `-EREMOTEIO` (:775) — the device did not acknowledge. Linux logs these at DEBUG, not error,
    /// because a NAK while probing an address is expected traffic rather than a fault.
    NoAck,
    /// `-EAGAIN` (:780) — arbitration lost to another master. The transfer may simply be retried;
    /// reporting it as a generic I/O error turns a recoverable collision into a hard failure.
    ArbitrationLost,
    /// `-EINVAL` (:782) — a general-call read, which Linux comments as "wrong msgs[] data": the
    /// CALLER built an impossible request, and no amount of retrying will help.
    BadRequest,
    /// `-EIO` (:784) — anything else.
    Io,
}

/// Map a raw TX_ABRT_SOURCE to what the caller is told.
///
/// THE ORDER OF THESE CHECKS IS THE CONTRACT, not a style choice. Linux tests NOACK FIRST and
/// returns immediately (:769-:775), so a word carrying BOTH a NAK and a lost arbitration is
/// reported as `NoAck` — NOT as the retryable `ArbitrationLost`. Reordering them silently converts
/// a permanent failure into an infinite retry loop, or the reverse.
pub fn verdict(abort_source: u32) -> AbortVerdict {
    use crate::regs::bits;
    if abort_source & bits::TX_ABRT_NOACK != 0 {
        return AbortVerdict::NoAck;
    }
    if abort_source & bits::TX_ARB_LOST != 0 {
        return AbortVerdict::ArbitrationLost;
    }
    if abort_source & bits::TX_ABRT_GCALL_READ != 0 {
        return AbortVerdict::BadRequest;
    }
    AbortVerdict::Io
}

/// Whether the abort log belongs at debug level rather than error level.
///
/// i2c-designware-common.c:769-:772 vs :776-:777 — a NAK is logged at DEBUG because probing an
/// absent address produces one on every scan, and an error-level line per probe is noise that
/// trains the reader to ignore the log.
pub fn is_expected_traffic(abort_source: u32) -> bool {
    abort_source & crate::regs::bits::TX_ABRT_NOACK != 0
}

/// The order in which the abort registers must be touched.
///
/// i2c-designware-master.c:611-:618, with Linux's own comment: "The IC_TX_ABRT_SOURCE register is
/// cleared whenever the IC_CLR_TX_ABRT is read. Preserve it beforehand."
///
/// Reading CLR_TX_ABRT first destroys the diagnosis — all fourteen causes become zero and the
/// transfer fails with nothing to say. The clear is a READ, not a write, which is exactly why this
/// is easy to get wrong: a reader scanning for `write(CLR_...)` finds nothing and concludes the
/// register is never cleared.
pub const CAPTURE_THEN_CLEAR: [u32; 2] =
    [crate::regs::off::TX_ABRT_SOURCE, crate::regs::off::CLR_TX_ABRT];
