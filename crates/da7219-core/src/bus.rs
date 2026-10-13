// SPDX-License-Identifier: GPL-2.0-only
//! The vocabulary of a DA7219 plan: a [`Step`] is one register operation or bounded wait; a [`Plan`] is a straight-line list of them.
//! The crate performs no I/O. The `.device` performs each [`Op`] over I2C (the wire form is [`wire`]) and reports back to the
//! [`crate::runner::Runner`].
//!
//! Ported from Linux `sound/soc/codecs/da7219.c` (regmap: `reg_bits = 8`, `val_bits = 8`, da7219.c:2440-2441). Copyright (c) 2015
//! Dialog Semiconductor, author Adam Thomson; the Linux files are `GPL-2.0-or-later`, this port is distributed under `GPL-2.0-only`.
//!
//! WHICH CHIP A PLAN TALKS TO. A plan holds registers and values, never an address; an address enters only where a wire message is
//! built ([`wire`]), through a [`Wiring`], and that is the codec's. The speaker amplifiers on the same bus are refused by name
//! ([`BusError::SpeakerAmplifier`]) at that one place, so no plan can reach them even by a typo (Astra audio ruling: headphones first,
//! speakers refused until the protection path exists).

use i2c_core_msg_core::address::validate_7bit_strict;
use i2c_core_msg_core::flags::I2C_M_RD;
use i2c_designware_master_core::setup::Message;

/// BOARD wiring of the I2C bus the codec shares with other devices, handed in by whoever read the board's description (the `.device` from
/// the board row / ACPI `_CRS`; ZERO-HW-OUTSIDE-BSP: no board address lives in this crate). It names the codec's address and the addresses
/// of the speaker amplifiers on the same bus; a [`Wiring`] cannot be built with the codec at an amplifier's address, and no message can be
/// built for an amplifier's address through it ([`BusError::SpeakerAmplifier`]). The Atlas values (ATLAS-PERIPHERALS-FROM-RECOVERY.md
/// section 3.2, recovery-image ACPI) are DA7219 0x1A and MAX98373 0x31 / 0x32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Wiring<'a> {
    codec: u16,
    speaker_amps: &'a [u16],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusError {
    /// A plan tried to address a speaker amplifier, or the codec was declared at an amplifier's address.
    SpeakerAmplifier { addr: u16 },
    /// A plan tried to address something that is not the codec.
    NotTheCodec { addr: u16 },
    /// The address is not a valid strict 7-bit address (i2c-core-msg-core).
    BadAddress { addr: u16 },
}

impl<'a> Wiring<'a> {
    pub fn new(codec: u16, speaker_amps: &'a [u16]) -> Result<Self, BusError> {
        validate_7bit_strict(codec).map_err(|_| BusError::BadAddress { addr: codec })?;
        if speaker_amps.contains(&codec) {
            return Err(BusError::SpeakerAmplifier { addr: codec });
        }
        Ok(Self { codec, speaker_amps })
    }

    pub const fn codec(&self) -> u16 {
        self.codec
    }

    /// The single gate every wire message passes: `addr` must be exactly the codec's.
    pub fn check_target(&self, addr: u16) -> Result<(), BusError> {
        if self.speaker_amps.contains(&addr) {
            return Err(BusError::SpeakerAmplifier { addr });
        }
        if validate_7bit_strict(addr).is_err() {
            return Err(BusError::BadAddress { addr });
        }
        if addr != self.codec {
            return Err(BusError::NotTheCodec { addr });
        }
        Ok(())
    }
}

/// What the `.device` does for one step. Registers are 8-bit and so are values (da7219.c:2440).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    /// `snd_soc_component_write`.
    Write { reg: u8, value: u8 },
    /// `snd_soc_component_update_bits`: read the register, replace the `mask` bits with `value`, write it back.
    Update { reg: u8, mask: u8, value: u8 },
    /// `snd_soc_component_read`; answered with [`Input::Value`].
    Read { reg: u8 },
    /// Read the register and require `(value & mask) == want`. A mismatch is the named failure [`Cause::VerifyMismatch`].
    Verify { reg: u8, mask: u8, want: u8 },
    /// Read until `(value & mask) == want`, at most `max_polls` reads, `interval_ms` apart (the loop shape of da7219.c:832-844).
    Poll { reg: u8, mask: u8, want: u8, interval_ms: u16, max_polls: u8 },
    /// `msleep`.
    DelayMs(u16),
    /// `regmap_bulk_read` of two registers (auto-increment); answered with [`Input::Pair`].
    ReadPair { reg: u8 },
    /// `regmap_bulk_write` of two registers.
    WritePair { reg: u8, first: u8, second: u8 },
}

/// What the plan does when the step fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnFail {
    /// Stop the plan with the named failure.
    Abort,
    /// Linux ignores the result (a warning at most); the first such failure is remembered ([`crate::runner::Runner::tolerated`]).
    Tolerate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    /// Stable name for logs, failures and tests ("hp-l-amp-enable").
    pub label: &'static str,
    pub op: Op,
    pub on_fail: OnFail,
}

impl Step {
    pub const NOP: Step = Step { label: "nop", op: Op::DelayMs(0), on_fail: OnFail::Tolerate };
    pub const fn new(label: &'static str, op: Op, on_fail: OnFail) -> Self {
        Self { label, op, on_fail }
    }
    /// An aborting step: the common case.
    pub const fn abort(label: &'static str, op: Op) -> Self {
        Self::new(label, op, OnFail::Abort)
    }
    /// The longest this step can wait, in milliseconds, with every bound at its limit.
    pub const fn max_wait_ms(&self) -> u32 {
        match self.op {
            Op::DelayMs(ms) => ms as u32,
            Op::Poll { interval_ms, max_polls, .. } => interval_ms as u32 * max_polls as u32,
            _ => 0,
        }
    }
}

/// Steps one plan can hold. FIXED ON PURPOSE (DECISIONS.md D10): a plan is a power SEQUENCE this crate writes down - not a table a
/// client or a file drives - and the longest one is the codec initialisation. Every builder is tested to fit; an overflowing builder is
/// refused by name ([`Plan::overflowed`], [`crate::PlanError::Overflow`]).
pub const PLAN_CAP: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    steps: [Step; PLAN_CAP],
    len: usize,
    overflow: bool,
}

impl Plan {
    pub const fn new() -> Self {
        Self { steps: [Step::NOP; PLAN_CAP], len: 0, overflow: false }
    }
    pub fn push(&mut self, s: Step) -> &mut Self {
        if self.len < PLAN_CAP {
            self.steps[self.len] = s;
            self.len += 1;
        } else {
            self.overflow = true;
        }
        self
    }
    pub fn extend(&mut self, other: &Plan) -> &mut Self {
        for s in other.as_slice() {
            self.push(*s);
        }
        self.overflow |= other.overflow;
        self
    }
    pub fn as_slice(&self) -> &[Step] {
        &self.steps[..self.len]
    }
    pub const fn len(&self) -> usize {
        self.len
    }
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub const fn overflowed(&self) -> bool {
        self.overflow
    }
    /// The worst-case time the whole plan can spend waiting (delays plus every poll read at its interval).
    pub fn max_wait_ms(&self) -> u32 {
        self.as_slice().iter().map(Step::max_wait_ms).sum()
    }
    /// `Err(Overflow)` if a builder ran past [`PLAN_CAP`]; otherwise the plan.
    pub fn checked(self) -> Result<Plan, crate::PlanError> {
        if self.overflow {
            Err(crate::PlanError::Overflow { cap: PLAN_CAP })
        } else {
            Ok(self)
        }
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}

/// The I2C wire form of an operation. A register write is ONE message `[reg, value]`; a register read is a write of `[reg]` then a read of
/// `len` bytes, joined by a repeated START (regmap-i2c, `reg_bits = 8`). All messages are checked by [`Wiring::check_target`].
pub mod wire {
    use super::*;

    /// At most two messages (a read is a write plus a read).
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Transaction {
        pub msgs: [Message; 2],
        pub count: usize,
        /// The bytes of the write message (`[reg, value]`, `[reg]`, or `[reg, a, b]`).
        pub out: [u8; 3],
    }

    impl Transaction {
        pub fn messages(&self) -> &[Message] {
            &self.msgs[..self.count]
        }
    }

    const EMPTY: Message = Message { addr: 0, flags: 0, len: 0 };

    /// The `_to` forms take the target explicitly so the gate itself can be driven with a speaker-amplifier address; the plain forms
    /// always use the wiring's codec address.
    pub fn write_to(w: &Wiring<'_>, addr: u16, reg: u8, value: u8) -> Result<Transaction, BusError> {
        w.check_target(addr)?;
        Ok(Transaction { msgs: [Message { addr, flags: 0, len: 2 }, EMPTY], count: 1, out: [reg, value, 0] })
    }

    pub fn write_pair_to(w: &Wiring<'_>, addr: u16, reg: u8, first: u8, second: u8) -> Result<Transaction, BusError> {
        w.check_target(addr)?;
        Ok(Transaction { msgs: [Message { addr, flags: 0, len: 3 }, EMPTY], count: 1, out: [reg, first, second] })
    }

    /// Read `len` registers starting at `reg`.
    pub fn read_to(w: &Wiring<'_>, addr: u16, reg: u8, len: usize) -> Result<Transaction, BusError> {
        w.check_target(addr)?;
        Ok(Transaction { msgs: [Message { addr, flags: 0, len: 1 }, Message { addr, flags: I2C_M_RD, len }], count: 2, out: [reg, 0, 0] })
    }

    pub fn write(w: &Wiring<'_>, reg: u8, value: u8) -> Result<Transaction, BusError> {
        write_to(w, w.codec(), reg, value)
    }

    pub fn write_pair(w: &Wiring<'_>, reg: u8, first: u8, second: u8) -> Result<Transaction, BusError> {
        write_pair_to(w, w.codec(), reg, first, second)
    }

    pub fn read(w: &Wiring<'_>, reg: u8, len: usize) -> Result<Transaction, BusError> {
        read_to(w, w.codec(), reg, len)
    }
}
