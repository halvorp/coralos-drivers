// SPDX-License-Identifier: GPL-2.0-only
//! The Linux errno values the i915 uAPI returns to userspace (include/uapi/asm-generic/errno-base.h and errno.h). The wire carries
//! the NEGATIVE value, exactly what `ioctl()` would have returned to libdrm as `-errno`.

/// A Linux errno, with its x86-64 value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Errno {
    EPERM = 1,       // errno-base.h:5
    ENOENT = 2,      // errno-base.h:6
    EIO = 5,         // errno-base.h:9
    E2BIG = 7,       // errno-base.h:11
    ENOEXEC = 8,     // errno-base.h:12
    ENOMEM = 12,     // errno-base.h:16
    EFAULT = 14,     // errno-base.h:18
    ENODEV = 19,     // errno-base.h:23
    EINVAL = 22,     // errno-base.h:26
    ENOSPC = 28,     // errno-base.h:32
    ENOPKG = 65,     // errno.h:48
    EOPNOTSUPP = 95, // errno.h:79
}

impl Errno {
    /// The positive errno number.
    pub const fn code(self) -> i32 {
        self as i32
    }
    /// What the ioctl returns: `-errno`.
    pub const fn neg(self) -> i32 {
        -(self as i32)
    }
    pub const fn name(self) -> &'static str {
        match self {
            Errno::EPERM => "EPERM",
            Errno::ENOENT => "ENOENT",
            Errno::EIO => "EIO",
            Errno::E2BIG => "E2BIG",
            Errno::ENOEXEC => "ENOEXEC",
            Errno::ENOMEM => "ENOMEM",
            Errno::EFAULT => "EFAULT",
            Errno::ENODEV => "ENODEV",
            Errno::EINVAL => "EINVAL",
            Errno::ENOSPC => "ENOSPC",
            Errno::ENOPKG => "ENOPKG",
            Errno::EOPNOTSUPP => "EOPNOTSUPP",
        }
    }
    pub const fn from_code(code: i32) -> Option<Errno> {
        Some(match code {
            1 => Errno::EPERM,
            2 => Errno::ENOENT,
            5 => Errno::EIO,
            7 => Errno::E2BIG,
            8 => Errno::ENOEXEC,
            12 => Errno::ENOMEM,
            14 => Errno::EFAULT,
            19 => Errno::ENODEV,
            22 => Errno::EINVAL,
            28 => Errno::ENOSPC,
            65 => Errno::ENOPKG,
            95 => Errno::EOPNOTSUPP,
            _ => return None,
        })
    }
}
