//! Linux errno numbers (x86-64), as the POSITIVE values a reply's `errno` field carries negated. They are ABI facts the shim hands to
//! libdrm unchanged, so they live in the wire crate, not in any port crate.

pub const EPERM: i32 = 1;
pub const ENOENT: i32 = 2;
pub const EIO: i32 = 5;
pub const E2BIG: i32 = 7;
pub const EBADF: i32 = 9;
pub const EAGAIN: i32 = 11;
pub const ENOMEM: i32 = 12;
pub const EACCES: i32 = 13;
pub const EFAULT: i32 = 14;
pub const EBUSY: i32 = 16;
pub const ENODEV: i32 = 19;
pub const EINVAL: i32 = 22;
pub const ENOSPC: i32 = 28;
pub const EBADSLT: i32 = 57;
pub const ETIME: i32 = 62;
pub const EOPNOTSUPP: i32 = 95;

/// The value a reply's `errno` field holds for failure `e` (a positive errno).
pub const fn neg(e: i32) -> i32 {
    -e
}
