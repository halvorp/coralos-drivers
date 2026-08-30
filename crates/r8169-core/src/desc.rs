// SPDX-License-Identifier: GPL-2.0-only
//! The RTL8169 RX/TX descriptor format.
//!
//! From `struct TxDesc` / `struct RxDesc` (r8169_main.c:646-656) and the generic descriptor bits
//! (r8169_main.c:579-582). Both descriptors have the SAME shape — two 32-bit words then a 64-bit
//! address — which is why one layout constant set serves both.
//!
//! The layout is not ours to choose: the NIC walks this memory itself, so a wrong size or a wrong
//! field offset means the hardware reads a different structure than the driver writes. That failure
//! is silent at the API level and shows up as corrupted frames or a ring that never advances.

/// `struct {TxDesc,RxDesc} { __le32 opts1; __le32 opts2; __le64 addr; }` — 16 bytes.
/// (r8169_main.c:646-656)
pub const DESC_BYTES: usize = 16;

/// Byte offsets within a descriptor. `addr` is 64-bit and 8-byte aligned at offset 8, which is why
/// the two option words come first.
pub const OFF_OPTS1: usize = 0;
pub const OFF_OPTS2: usize = 4;
pub const OFF_ADDR: usize = 8;

/// Descriptor is owned by the NIC. The driver must not touch a descriptor while this is set; it is
/// the handshake the whole ring protocol rests on. (r8169_main.c:579)
pub const DESC_OWN: u32 = 1 << 31;
/// End of the descriptor ring — set on the LAST descriptor so the NIC wraps instead of running off
/// the end. (r8169_main.c:580)
pub const RING_END: u32 = 1 << 30;
/// First segment of a packet. (r8169_main.c:581)
pub const FIRST_FRAG: u32 = 1 << 29;
/// Final segment of a packet. (r8169_main.c:582)
pub const LAST_FRAG: u32 = 1 << 28;

/// The low 13 bits of `opts1` are the buffer length; the flags above occupy the top nibble.
/// Derived from Linux's use in `rtl8169_mark_to_asic`, which writes `DescOwn | eor |
/// R8169_RX_BUF_SIZE` into opts1 as a single word (r8169_main.c:4151).
pub const OPTS1_LEN_MASK: u32 = 0x1fff;

/// Build an RX descriptor's `opts1`: hand the buffer to the NIC, preserving the ring-end bit.
///
/// Mirrors `rtl8169_mark_to_asic` (r8169_main.c:4146-4151), which reads back the existing
/// `RingEnd` bit and ORs it into the new word rather than recomputing it — because the ring-end
/// flag belongs to the descriptor's POSITION and must survive every recycle. Losing it on the last
/// descriptor makes the NIC walk past the end of the ring.
pub fn rx_opts1_hand_to_nic(existing_opts1: u32, buf_len: u32) -> u32 {
    let eor = existing_opts1 & RING_END;
    DESC_OWN | eor | (buf_len & OPTS1_LEN_MASK)
}

/// Does this descriptor belong to the NIC?
pub fn is_owned_by_nic(opts1: u32) -> bool {
    opts1 & DESC_OWN != 0
}
