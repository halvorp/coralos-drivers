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
