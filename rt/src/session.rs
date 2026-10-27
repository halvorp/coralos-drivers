//! SERVING `connect()`: the two kernel-posted messages a server reads, and the wait set a server builds over its sessions.
//!
//! A client `connect(listen_ep)`s and the kernel mints it a per-session pair (`req`, `evt`) from the dynamic endpoint pool, grants the
//! serving process RECV(`req`) / SEND(`evt`), and posts an ANNOUNCE on the LISTEN endpoint. When the client closes the session or dies,
//! the kernel posts a TOMBSTONE on `req`, and `req` then reads PEER_DEAD forever (it is READY in a wait, and a `try_recv` returns the
//! `PEER_DEAD` word) until the server releases the pair with [`crate::session_release`].
//!
//! The layouts are the kernel's (`coral_firmament_abi::session_wire`, sched/connect.rs); this file restates only what a driver decodes.
//! A request protocol whose first byte can be [`TOMBSTONE_TAG`] in exactly [`TOMBSTONE_LEN`] bytes cannot tell the two apart: a server
//! that fragments frames over several messages (i915-wire `frag`) recognises a tombstone only BETWEEN frames, where a message starts
//! with the protocol's own magic, and treats it as data inside one (the PEER_DEAD that follows reaps the session either way).

/// First byte of the connect announce.
pub const ANNOUNCE_TAG: u8 = 0xC0;
/// Length of the connect announce.
pub const ANNOUNCE_LEN: usize = 13;
/// First byte of the end-of-session tombstone.
pub const TOMBSTONE_TAG: u8 = 0xDD;
/// Length of the end-of-session tombstone.
pub const TOMBSTONE_LEN: usize = 9;
/// Endpoints a wait can name: the wait set is a 128-bit bitmap (`WAIT_ANY_IRQ`'s two words). This is the KERNEL's width (its endpoint
/// pool is smaller), not a limit of this crate: an endpoint at or past it cannot be waited on and is refused by name.
pub const WAIT_SET_EPS: u16 = 128;

/// A decoded connect announce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Announce {
    /// The session's request endpoint: the server RECVs the client's messages here.
    pub req: u16,
    /// The session's event endpoint: the server SENDs replies here.
    pub evt: u16,
    /// The connecting task's node (a packed task reference); opaque here.
    pub client: u64,
}

/// Decode a message received on a LISTEN endpoint. `None` for anything that is not a whole announce.
pub fn decode_announce(m: &[u8]) -> Option<Announce> {
    if m.len() < ANNOUNCE_LEN || m[0] != ANNOUNCE_TAG {
        return None;
    }
    let mut c = [0u8; 8];
    c.copy_from_slice(&m[5..13]);
    Some(Announce { req: u16::from_le_bytes([m[1], m[2]]), evt: u16::from_le_bytes([m[3], m[4]]), client: u64::from_le_bytes(c) })
}

/// Is `m`, received on a session's request endpoint, the end-of-session tombstone? (Exactly nine bytes starting `0xDD`.)
pub fn is_tombstone(m: &[u8]) -> bool {
    m.len() == TOMBSTONE_LEN && m[0] == TOMBSTONE_TAG
}

/// The wait set of `eps` as the two 64-bit words `wait_any_irq` takes (`lo` = endpoints 0..64, `hi` = 64..128). `Err(ep)` names the first
/// endpoint that does not fit: a shift past 63 wraps on x86, so naming such an endpoint would silently wait on a DIFFERENT one.
pub fn ep_set(eps: impl IntoIterator<Item = u16>) -> Result<(u64, u64), u16> {
    let (mut lo, mut hi) = (0u64, 0u64);
    for ep in eps {
        match ep {
            0..=63 => lo |= 1u64 << ep,
            64..=127 => hi |= 1u64 << (ep - 64),
            _ => return Err(ep),
        }
    }
    Ok((lo, hi))
}

/// Words of the wide wait set: the kernel's `EP_WORDS` (435 endpoints -> 7 words) with headroom; words past the kernel's width are not read.
pub const EP_WORDS: usize = 8;

/// The wait set of `eps` as `EP_WORDS` words for `WAIT_ANY_SET`, ep `e` at bit `e % 64` of word `e / 64`. Session endpoints are minted
/// from a pool that grows past 127 once a desktop's services have connected (the GPU's display session was refused
/// at req>=128 by the 128-bit set), so a server waits with THIS, not [`ep_set`]. `Err(ep)` names the first endpoint past the width.
pub fn ep_words(eps: impl IntoIterator<Item = u16>) -> Result<[u64; EP_WORDS], u16> {
    let mut w = [0u64; EP_WORDS];
    for ep in eps {
        let i = (ep / 64) as usize;
        if i >= EP_WORDS {
            return Err(ep);
        }
        w[i] |= 1u64 << (ep % 64);
    }
    Ok(w)
}
