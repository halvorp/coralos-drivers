// SPDX-License-Identifier: GPL-2.0-only
//! A refused call: the Linux errno the ioctl would have returned and the stable name of the refusal. Every port crate's error maps
//! here, and the name is what the device logs (`[I915-UAPI op=... refused name=... errno=...]`).
use crate::exec::ExecError;
use i915_gem_core::GemError;
use i915_syncobj_core::SyncError;
use i915_uapi_core::UapiError;
use i915_wire::{errno, WireError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fail {
    /// Positive errno.
    pub errno: i32,
    pub name: &'static str,
    /// The payload did not decode (the reply status is `Malformed`, not `Ok`).
    pub malformed: bool,
}

impl Fail {
    pub const fn new(errno: i32, name: &'static str) -> Fail {
        Fail { errno, name, malformed: false }
    }
    /// A table or the DMA endowment could not grow: refused BEFORE the thing exists (RUNTIME-TABLES-HAVE-NO-CAP).
    pub const fn oom(_table: &'static str) -> Fail {
        Fail { errno: errno::ENOMEM, name: "OUT-OF-MEMORY", malformed: false }
    }
}

impl From<GemError> for Fail {
    fn from(e: GemError) -> Fail {
        Fail { errno: e.errno(), name: e.name(), malformed: false }
    }
}

impl From<UapiError> for Fail {
    fn from(e: UapiError) -> Fail {
        let malformed = matches!(e, UapiError::Truncated { .. } | UapiError::TrailingBytes { .. } | UapiError::ReservedNonZero { .. } | UapiError::WireCountMismatch { .. } | UapiError::LimitExceeded { .. });
        Fail { errno: e.errno().code(), name: e.name(), malformed }
    }
}

impl From<SyncError> for Fail {
    fn from(e: SyncError) -> Fail {
        Fail { errno: e.errno(), name: e.name(), malformed: false }
    }
}

impl From<WireError> for Fail {
    fn from(e: WireError) -> Fail {
        Fail { errno: e.errno(), name: e.name(), malformed: true }
    }
}

impl From<ExecError> for Fail {
    fn from(e: ExecError) -> Fail {
        let errno = match e {
            ExecError::Dma(_) | ExecError::EndowmentExhausted { .. } | ExecError::GrantTableFull { .. } => errno::ENOMEM,
            ExecError::AwaitRetireTimeout { .. } => errno::EBUSY,
            _ => errno::EIO,
        };
        let name = match e {
            ExecError::Mem(m) => m.name(),
            ExecError::Submit(s) => s.name(),
            other => other.name(),
        };
        Fail { errno, name, malformed: false }
    }
}
