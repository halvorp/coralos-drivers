// SPDX-License-Identifier: GPL-2.0-only
//! The register-definition convention: a typed offset, bit/field helpers, and the masked-write encoding.
//!
//! A register is `pub const NAME: Reg = Reg::new(0x....);` followed by its fields as `pub const NAME_FIELD: u32 =
//! genmask(hi, lo);` / `bit(n)`, each line citing `<linux file>:<line>` (PORT_CONVENTIONS.md §1). `Reg` mirrors Linux's
//! `i915_reg_t` (i915_reg_defs.h): an offset into BAR0, nothing more.
use coralos_hal::Mmio;

/// A BAR0 register offset (`i915_reg_t`, i915_reg_defs.h:187-191, `_MMIO(x)`). 32-bit registers only: every register this
/// family names is accessed with 32-bit reads/writes, as Linux does (`intel_uncore_read` / `intel_uncore_write`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Reg(u32);

impl Reg {
    pub const fn new(offset: u32) -> Reg {
        Reg(offset)
    }
    pub const fn offset(self) -> u32 {
        self.0
    }
    /// The register `n` entries along an evenly spaced array (`_PICK_EVEN`-style: `base + n * stride`).
    pub const fn nth(base: u32, n: u32, stride: u32) -> Reg {
        Reg(base + n * stride)
    }
}

/// `REG_BIT(n)` (i915_reg_defs.h:21). `n` must be below 32.
pub const fn bit(n: u32) -> u32 {
    assert!(n < 32, "REG_BIT: bit index out of range");
    1u32 << n
}

/// `REG_GENMASK(high, low)` (i915_reg_defs.h:16): bits `low..=high` set.
pub const fn genmask(high: u32, low: u32) -> u32 {
    assert!(high < 32 && low <= high, "REG_GENMASK: bad bounds");
    (u32::MAX >> (31 - high)) & (u32::MAX << low)
}

/// `REG_FIELD_GET(mask, val)` (i915_reg_defs.h:75): the field under `mask`, shifted down.
pub const fn field_get(mask: u32, val: u32) -> u32 {
    (val & mask) >> mask.trailing_zeros()
}

/// `REG_FIELD_PREP(mask, val)` (i915_reg_defs.h:41): `val` shifted into the field and clipped to it.
pub const fn field_prep(mask: u32, val: u32) -> u32 {
    (val << mask.trailing_zeros()) & mask
}

/// `__MASKED_FIELD(mask, value)` (i915_reg_defs.h:108): the "masked register" write encoding - the high half selects which
/// of the low 16 bits the write changes. `mask` and `value` must fit in 16 bits and `value` must lie within `mask`
/// (Linux's `_MASKED_FIELD` BUILD_BUG_ONs, i915_reg_defs.h:110-117); a violation here is a panic in `const` use, and
/// `None` from [`try_masked_field`].
pub const fn masked_field(mask: u32, value: u32) -> u32 {
    assert!(mask & 0xffff_0000 == 0, "Incorrect mask");
    assert!(value & 0xffff_0000 == 0, "Incorrect value");
    assert!(value & !mask == 0, "Incorrect value for mask");
    mask << 16 | value
}

/// The runtime-checked form of [`masked_field`].
pub const fn try_masked_field(mask: u32, value: u32) -> Option<u32> {
    if mask & 0xffff_0000 != 0 || value & 0xffff_0000 != 0 || value & !mask != 0 {
        None
    } else {
        Some(mask << 16 | value)
    }
}

/// `_MASKED_BIT_ENABLE(a)` (i915_reg_defs.h:118) = `_MASKED_FIELD(a, a)`.
pub const fn masked_bit_enable(a: u32) -> u32 {
    masked_field(a, a)
}

/// `_MASKED_BIT_DISABLE(a)` (i915_reg_defs.h:119) = `_MASKED_FIELD(a, 0)`.
pub const fn masked_bit_disable(a: u32) -> u32 {
    masked_field(a, 0)
}

/// Typed register access over `coralos_hal::Mmio` (the contract's one MMIO trait): `intel_uncore_read` /
/// `intel_uncore_write` for a [`Reg`].
pub trait RegIo {
    fn rd(&mut self, r: Reg) -> u32;
    fn wr(&mut self, r: Reg, v: u32);
}

impl<M: Mmio + ?Sized> RegIo for M {
    fn rd(&mut self, r: Reg) -> u32 {
        self.r32(r.offset())
    }
    fn wr(&mut self, r: Reg, v: u32) {
        self.w32(r.offset(), v)
    }
}
