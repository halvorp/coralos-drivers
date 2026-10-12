//! SESSION-OPEN / SESSION-CLOSE. A session is one client of the device (one `drm_file`): its handles, contexts, address spaces,
//! mappings, syncobjs and descriptors die with it. SESSION-OPEN is sent with `session_incarnation = 0`; the reply's header and body
//! carry the incarnation every later frame must present. A frame with another incarnation is answered `StaleSession`.
use super::msg;

/// `CAP_SYS_ADMIN` of the opening client (reset counts, priority above normal): what `drm_file`'s caller had.
pub const CAP_SYS_ADMIN: u32 = 1 << 0;
/// `CAP_SYS_NICE`: context priority above `I915_CONTEXT_DEFAULT_PRIORITY`.
pub const CAP_SYS_NICE: u32 = 1 << 1;
/// `perfmon_capable()`: the SYSTEM region's `unallocated_size` in the memory-regions query.
pub const CAP_PERFMON: u32 = 1 << 2;
pub const CAPS_KNOWN: u32 = CAP_SYS_ADMIN | CAP_SYS_NICE | CAP_PERFMON;

msg!(SessionOpenReq { caps: u32, _pad: super::Pad32 });
msg!(SessionOpenRep { incarnation: u64 });
